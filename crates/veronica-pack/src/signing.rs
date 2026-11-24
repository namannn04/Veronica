use std::fs;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{bail, ensure, Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use veronica_protocol::catalog::{
    Catalog, EnvelopeSignature, SignatureAlgorithm, SignedEnvelope, CATALOG_SCHEMA, ENVELOPE_SCHEMA,
};

use crate::sha256_hex;

const KEY_ID_PREFIX: &str = "ext-";
const KEY_ID_HEX_LENGTH: usize = 12;
const ENV_PREFIX: &str = "env:";

pub fn generate() -> Result<SigningKey> {
    let mut seed = [0_u8; ed25519_dalek::SECRET_KEY_LENGTH];
    getrandom::fill(&mut seed).map_err(|error| anyhow::anyhow!("no system randomness: {error}"))?;
    Ok(SigningKey::from_bytes(&seed))
}

pub fn key_id(public: &VerifyingKey) -> String {
    let digest = sha256_hex(public.as_bytes());
    format!("{KEY_ID_PREFIX}{}", &digest[..KEY_ID_HEX_LENGTH])
}

pub fn encode_public(public: &VerifyingKey) -> String {
    STANDARD.encode(public.as_bytes())
}

pub fn write_private(key: &SigningKey, path: &Path) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    writeln!(file, "{}", STANDARD.encode(key.to_bytes()))?;
    Ok(())
}

pub fn decode_private(text: &str) -> Result<SigningKey> {
    let bytes = STANDARD
        .decode(text.trim())
        .context("private key is not base64")?;
    let seed: [u8; ed25519_dalek::SECRET_KEY_LENGTH] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("private key must be 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}

pub fn load_private(source: &str) -> Result<SigningKey> {
    let text = if let Some(name) = source.strip_prefix(ENV_PREFIX) {
        std::env::var(name).with_context(|| format!("environment variable {name} is not set"))?
    } else {
        fs::read_to_string(source).with_context(|| format!("reading {source}"))?
    };
    decode_private(&text)
}

pub fn decode_public(text: &str) -> Result<VerifyingKey> {
    let bytes = STANDARD
        .decode(text.trim())
        .context("public key is not base64")?;
    let raw: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key must be 32 bytes"))?;
    VerifyingKey::from_bytes(&raw).context("public key is not a valid ed25519 point")
}

fn parse_catalog(payload: &[u8]) -> Result<Catalog> {
    let catalog: Catalog = serde_json::from_slice(payload).context("payload is not a catalog")?;
    ensure!(
        catalog.schema == CATALOG_SCHEMA,
        "catalog schema {} is not supported",
        catalog.schema
    );
    Ok(catalog)
}

pub fn sign(payload: &[u8], key: &SigningKey, key_id: &str) -> Result<SignedEnvelope> {
    parse_catalog(payload)?;
    Ok(SignedEnvelope {
        schema: ENVELOPE_SCHEMA,
        payload: STANDARD.encode(payload),
        signatures: vec![EnvelopeSignature {
            key_id: key_id.to_string(),
            algorithm: SignatureAlgorithm::Ed25519,
            signature: STANDARD.encode(key.sign(payload).to_bytes()),
        }],
    })
}

pub fn verify(envelope: &SignedEnvelope, public: &VerifyingKey, key_id: &str) -> Result<Catalog> {
    ensure!(
        envelope.schema == ENVELOPE_SCHEMA,
        "envelope schema {} is not supported",
        envelope.schema
    );
    let payload = STANDARD
        .decode(&envelope.payload)
        .context("payload is not base64")?;
    let Some(entry) = envelope
        .signatures
        .iter()
        .find(|entry| entry.key_id == key_id)
    else {
        bail!("no signature from key {key_id}");
    };
    let SignatureAlgorithm::Ed25519 = entry.algorithm;
    let signature = Signature::from_slice(
        &STANDARD
            .decode(&entry.signature)
            .context("signature is not base64")?,
    )
    .context("signature is not 64 bytes")?;
    public
        .verify_strict(&payload, &signature)
        .context("signature does not match the payload")?;
    parse_catalog(&payload)
}
