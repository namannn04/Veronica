use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{json, Value};
use veronica_pack::catalog::{self, CatalogOptions};
use veronica_pack::fingerprint::{self, stamp};
use veronica_pack::pack::{pack, PackInputs};
use veronica_pack::{audit, bundle, repo, signing, trust, validate};
use veronica_protocol::canonical;
use veronica_protocol::catalog::SignedEnvelope;
use veronica_protocol::manifest::Target;

#[derive(Parser)]
#[command(
    name = "vr-pack",
    about = "Build, package and sign Veronica extensions"
)]
struct Cli {
    #[arg(long, global = true, help = "Print the result as JSON")]
    json: bool,
    #[arg(
        long,
        global = true,
        help = "Repository root, found through git by default"
    )]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Validate manifests and the conflicts between them")]
    Validate { dirs: Vec<PathBuf> },
    #[command(about = "Hash every input an extension is built from")]
    Fingerprint { id: String },
    #[command(about = "Stamp the package version from the manifest and git history")]
    Version { id: String },
    #[command(about = "Assemble a package with its checksum and metadata")]
    Pack {
        id: String,
        #[arg(long)]
        backend: Option<PathBuf>,
        #[arg(long)]
        ui: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    #[command(about = "Build the catalog payload from release metadata")]
    Catalog {
        #[arg(long)]
        metadata: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        revision: u64,
        #[arg(long, default_value_t = 30)]
        expires_days: u32,
        #[arg(long)]
        commit: Option<String>,
    },
    #[command(about = "Create an ed25519 signing key")]
    Keygen {
        #[arg(long)]
        out_private: PathBuf,
    },
    #[command(about = "Wrap a catalog payload in a signed envelope")]
    Sign {
        #[arg(long, help = "Private key file, or env:NAME")]
        key: String,
        #[arg(long)]
        key_id: String,
        payload: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    #[command(about = "Check a signed envelope against a public key")]
    Verify {
        #[arg(long)]
        public: String,
        #[arg(long)]
        key_id: String,
        envelope: PathBuf,
    },
    #[command(about = "Gather built packages and their index for the desktop packages")]
    Bundle {
        #[arg(long)]
        packages: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    #[command(about = "Check a private key against the keys the host trusts")]
    KeyStatus {
        #[arg(long, help = "Private key file, or env:NAME")]
        key: String,
    },
    #[command(about = "Compare declared permissions with what the backend links")]
    Audit { package: PathBuf },
}

struct Outcome {
    report: Value,
    text: String,
    ok: bool,
}

impl Outcome {
    fn ok(report: impl Serialize, text: String) -> Result<Self> {
        Ok(Self {
            report: serde_json::to_value(report)?,
            text,
            ok: true,
        })
    }
}

fn root_of(cli_root: Option<&Path>) -> Result<PathBuf> {
    match cli_root {
        Some(root) => Ok(root.to_path_buf()),
        None => repo::find_root(&std::env::current_dir()?),
    }
}

fn run(cli: &Cli) -> Result<Outcome> {
    let root = || root_of(cli.root.as_deref());
    match &cli.command {
        Command::Validate { dirs } => {
            let dirs = if dirs.is_empty() {
                repo::extension_dirs(&root()?)?
            } else {
                dirs.clone()
            };
            let report = validate::validate_dirs(&dirs);
            let text = std::iter::once(format!("{} extension(s) checked", report.extensions.len()))
                .chain(
                    report
                        .problems
                        .iter()
                        .map(|problem| format!("{}: {}", problem.extension, problem.message)),
                )
                .collect::<Vec<_>>()
                .join("\n");
            Ok(Outcome {
                ok: report.is_ok(),
                report: serde_json::to_value(&report)?,
                text,
            })
        }
        Command::Fingerprint { id } => {
            let root = root()?;
            let inputs = fingerprint::input_paths(&root, id)?;
            let value = fingerprint::fingerprint(&root, &inputs)?;
            Outcome::ok(
                json!({ "id": id, "fingerprint": value, "inputs": inputs }),
                value,
            )
        }
        Command::Version { id } => {
            let stamp = stamp(&root()?, id)?;
            let text = stamp.version.to_string();
            Outcome::ok(stamp, text)
        }
        Command::Pack {
            id,
            backend,
            ui,
            out,
        } => {
            let root = root()?;
            let stamp = stamp(&root, id)?;
            let packed = pack(
                &PackInputs {
                    root: &root,
                    backend: backend.as_deref(),
                    ui: ui.as_deref(),
                    out,
                    target: Target::LinuxX86_64,
                },
                &stamp,
            )?;
            let text = packed.archive.display().to_string();
            Outcome::ok(
                json!({ "archive": text, "metadata": packed.metadata }),
                text,
            )
        }
        Command::Catalog {
            metadata,
            out,
            revision,
            expires_days,
            commit,
        } => {
            let commit = match commit {
                Some(commit) => commit.clone(),
                None => repo::head_commit(&root()?)?,
            };
            let catalog = catalog::build(
                catalog::read_metadata(metadata)?,
                &CatalogOptions {
                    revision: *revision,
                    expires_days: *expires_days,
                    commit,
                },
            )?;
            fs::write(out, catalog.to_payload_bytes()?)
                .with_context(|| format!("writing {}", out.display()))?;
            let ids: Vec<_> = catalog.extensions.iter().map(|item| &item.id).collect();
            Outcome::ok(
                json!({ "out": out, "revision": revision, "extensions": ids }),
                format!("{} extension(s) in {}", ids.len(), out.display()),
            )
        }
        Command::Keygen { out_private } => {
            let key = signing::generate()?;
            signing::write_private(&key, out_private)?;
            let public = signing::encode_public(&key.verifying_key());
            let key_id = signing::key_id(&key.verifying_key());
            let text = format!("public key {public}\nkey id {key_id}");
            Outcome::ok(
                json!({ "publicKey": public, "keyId": key_id, "privateKey": out_private }),
                text,
            )
        }
        Command::Sign {
            key,
            key_id,
            payload,
            out,
        } => {
            let bytes =
                fs::read(payload).with_context(|| format!("reading {}", payload.display()))?;
            let envelope = signing::sign(&bytes, &signing::load_private(key)?, key_id)?;
            fs::write(out, envelope.to_canonical_string()?)
                .with_context(|| format!("writing {}", out.display()))?;
            Outcome::ok(
                json!({ "out": out, "keyId": key_id }),
                format!("signed {}", out.display()),
            )
        }
        Command::Verify {
            public,
            key_id,
            envelope,
        } => {
            let text = fs::read_to_string(envelope)
                .with_context(|| format!("reading {}", envelope.display()))?;
            let parsed: SignedEnvelope =
                serde_json::from_str(&text).context("envelope is malformed")?;
            let catalog = signing::verify(&parsed, &signing::decode_public(public)?, key_id)?;
            Outcome::ok(
                json!({ "keyId": key_id, "revision": catalog.revision, "expiresAt": catalog.expires_at }),
                format!(
                    "valid signature from {key_id}, revision {}",
                    catalog.revision
                ),
            )
        }
        Command::Bundle { packages, out } => {
            let index = bundle::bundle(packages, out)?;
            let text = format!(
                "{} extension(s) bundled in {}",
                index.extensions.len(),
                out.display()
            );
            Outcome::ok(index, text)
        }
        Command::KeyStatus { key } => {
            let path = trust::source_path(&root()?);
            let source =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            let report = trust::status(&signing::load_private(key)?, &source)?;
            let text = if report.trusted {
                format!("{} is trusted by the host", report.key_id)
            } else if report.development {
                format!("{} is the development key", report.key_id)
            } else {
                format!("{} is not in a trust.rs KEYS slot", report.key_id)
            };
            Outcome::ok(report, text)
        }
        Command::Audit { package } => {
            let report = audit::audit(package)?;
            let text = std::iter::once(format!(
                "{}: {} problem(s)",
                report.id,
                report.problems.len()
            ))
            .chain(report.problems.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n");
            Ok(Outcome {
                ok: report.is_ok(),
                report: serde_json::to_value(&report)?,
                text,
            })
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(outcome) => {
            if cli.json {
                print!("{}", canonical::value_to_string_pretty(&outcome.report));
            } else {
                println!("{}", outcome.text);
            }
            if outcome.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            if cli.json {
                print!(
                    "{}",
                    canonical::value_to_string_pretty(&json!({ "error": format!("{error:#}") }))
                );
            }
            eprintln!("vr-pack: {error:#}");
            ExitCode::FAILURE
        }
    }
}
