//! Where database credentials actually live.
//!
//! Edith puts them in the macOS Keychain. The Linux counterpart is the Secret
//! Service — `org.freedesktop.secrets`, which GNOME Keyring implements and
//! which is already running on any Ubuntu desktop session — so Veronica talks
//! to it over D-Bus. No password is ever written into a connection definition,
//! a settings file, a log line or a shell history.
//!
//! There is a fallback, and it is deliberately grudging. Over SSH there is no
//! keyring to unlock, and a database client that cannot store a password on a
//! headless box is not much use; so secrets go to a file under the state
//! directory with mode 0600. That is worse than a keyring and the diagnostics
//! say so, rather than pretending the two are equivalent.
//!
//! Both stores are addressed the same way: an application id, a purpose and a
//! UUID. Nothing about the database appears in the attributes, so the keyring's
//! own listing does not leak which hosts you connect to.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value as ZValue};

use crate::connection::SecretPurpose;

pub const SECRET_SERVICE: &str = "org.freedesktop.secrets";
pub const SERVICE_PATH: &str = "/org/freedesktop/secrets";
pub const DEFAULT_COLLECTION: &str = "/org/freedesktop/secrets/aliases/default";
/// The attribute every Veronica secret carries, so they can be found and
/// removed as a set.
pub const APPLICATION: &str = "io.github.namannn04.Veronica";

/// Which store answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Backing {
    /// The desktop keyring. What you want.
    SecretService,
    /// A 0600 file. Honest, and worse.
    File,
}

impl Backing {
    pub fn title(self) -> &'static str {
        match self {
            Backing::SecretService => "the desktop keyring",
            Backing::File => "a 0600 file under the state directory",
        }
    }

    /// Whether this is the store the feature is designed around.
    pub fn is_preferred(self) -> bool {
        self == Backing::SecretService
    }
}

/// The attributes one secret is filed under.
fn attributes(identifier: Uuid, purpose: SecretPurpose) -> HashMap<String, String> {
    HashMap::from([
        ("application".to_string(), APPLICATION.to_string()),
        ("identifier".to_string(), identifier.to_string()),
        ("purpose".to_string(), purpose.key().to_string()),
    ])
}

/// A label a person would recognise in Seahorse, naming no host or database.
fn label(purpose: SecretPurpose) -> String {
    format!("Veronica database {}", purpose.key())
}

/// Talk to the Secret Service.
#[derive(Debug)]
struct Service {
    connection: zbus::Connection,
    session: OwnedObjectPath,
}

impl Service {
    async fn open() -> Result<Self> {
        let connection = zbus::Connection::session()
            .await
            .context("no session bus, so no keyring")?;

        // The "plain" algorithm sends the secret unencrypted over the session
        // bus. That bus is a unix socket owned by this user with 0600
        // permissions, so the encrypted handshake would protect against nothing
        // an attacker who could read it is not already past.
        let reply = connection
            .call_method(
                Some(SECRET_SERVICE),
                SERVICE_PATH,
                Some("org.freedesktop.Secret.Service"),
                "OpenSession",
                &("plain", ZValue::from("")),
            )
            .await
            .context("the Secret Service refused a session")?;
        let (_output, session): (zbus::zvariant::OwnedValue, OwnedObjectPath) =
            reply.body().deserialize()?;
        Ok(Self {
            connection,
            session,
        })
    }

    async fn search(&self, attributes: &HashMap<String, String>) -> Result<Vec<OwnedObjectPath>> {
        let reply = self
            .connection
            .call_method(
                Some(SECRET_SERVICE),
                SERVICE_PATH,
                Some("org.freedesktop.Secret.Service"),
                "SearchItems",
                attributes,
            )
            .await?;
        let (unlocked, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) =
            reply.body().deserialize()?;
        if !unlocked.is_empty() {
            return Ok(unlocked);
        }
        if locked.is_empty() {
            return Ok(Vec::new());
        }
        // A locked keyring needs the user to type their password. The prompt is
        // the desktop's own, which is the point of using the keyring at all.
        let reply = self
            .connection
            .call_method(
                Some(SECRET_SERVICE),
                SERVICE_PATH,
                Some("org.freedesktop.Secret.Service"),
                "Unlock",
                &(&locked,),
            )
            .await?;
        let (unlocked, _prompt): (Vec<OwnedObjectPath>, OwnedObjectPath) =
            reply.body().deserialize()?;
        Ok(unlocked)
    }

    async fn read(&self, item: &ObjectPath<'_>) -> Result<Vec<u8>> {
        let reply = self
            .connection
            .call_method(
                Some(SECRET_SERVICE),
                item,
                Some("org.freedesktop.Secret.Item"),
                "GetSecret",
                &(&self.session,),
            )
            .await?;
        let (_session, _parameters, value, _content_type): (
            OwnedObjectPath,
            Vec<u8>,
            Vec<u8>,
            String,
        ) = reply.body().deserialize()?;
        Ok(value)
    }

    async fn write(
        &self,
        attributes: HashMap<String, String>,
        label: String,
        secret: &[u8],
    ) -> Result<()> {
        let properties: HashMap<&str, ZValue> = HashMap::from([
            ("org.freedesktop.Secret.Item.Label", ZValue::from(label)),
            (
                "org.freedesktop.Secret.Item.Attributes",
                ZValue::from(attributes),
            ),
        ]);
        let value = (
            &self.session,
            Vec::<u8>::new(),
            secret.to_vec(),
            "text/plain",
        );
        self.connection
            .call_method(
                Some(SECRET_SERVICE),
                DEFAULT_COLLECTION,
                Some("org.freedesktop.Secret.Collection"),
                "CreateItem",
                // `replace` is true: storing a rotated password should replace
                // the old one rather than leave two and read whichever comes
                // back first.
                &(properties, value, true),
            )
            .await
            .context("the keyring refused to store the secret")?;
        Ok(())
    }

    async fn delete(&self, item: &ObjectPath<'_>) -> Result<()> {
        self.connection
            .call_method(
                Some(SECRET_SERVICE),
                item,
                Some("org.freedesktop.Secret.Item"),
                "Delete",
                &(),
            )
            .await?;
        Ok(())
    }
}

/// The file fallback. Base64 so a secret with a newline in it survives.
#[derive(Debug, Default, Serialize, Deserialize)]
struct FileStore {
    #[serde(default)]
    secrets: HashMap<String, String>,
}

fn file_key(identifier: Uuid, purpose: SecretPurpose) -> String {
    format!("{identifier}:{}", purpose.key())
}

fn read_file(path: &Path) -> Result<FileStore> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileStore::default()),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn write_file(path: &Path, store: &FileStore) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        // The directory too: a secret file inside a world-readable directory is
        // still listable, which says who you connect to even if not how.
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(store)?)?;
    // Set before the rename, so the file is never briefly readable.
    std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&temp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

/// Store and fetch secrets.
///
/// Which of the two stores answers is resolved once per instance rather than
/// per call. Asking the bus every time looked harmless and was not: a keyring
/// that is briefly unreachable - a busy session bus, a `Service::open` that
/// loses a race - used to move a single caller from the keyring to the file
/// and back between one call and the next. `store` then wrote one place while
/// `load` read the other, and [`SecretStore::signing_key`] answered with a key
/// the store did not have.
#[derive(Debug, Clone)]
pub struct SecretStore {
    fallback_path: PathBuf,
    /// The keyring session, opened at most once and then reused; `None` once
    /// this machine has been found not to have one. Shared across clones,
    /// because clones are the same store.
    ///
    /// Opening a session per call was the original shape and it was the bug:
    /// under any load - a batch of operations, a test suite - some of those
    /// opens lose, and the store silently changes which of its two backings
    /// answers between one call and the next.
    service: std::sync::Arc<tokio::sync::OnceCell<Option<std::sync::Arc<Service>>>>,
    /// Held while a signing key is minted, so racing callers in one process
    /// converge on the first key written rather than each on their own.
    minting: std::sync::Arc<tokio::sync::Mutex<()>>,
}

impl SecretStore {
    pub fn new(fallback_path: impl Into<PathBuf>) -> Self {
        Self {
            fallback_path: fallback_path.into(),
            service: std::sync::Arc::new(tokio::sync::OnceCell::new()),
            minting: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// The keyring session, or `None` on a machine without one.
    async fn service(&self) -> Option<&Service> {
        self.service
            .get_or_init(|| async {
                match Service::open().await {
                    Ok(service) => Some(std::sync::Arc::new(service)),
                    Err(_) => None,
                }
            })
            .await
            .as_deref()
    }

    /// Which store is in use here. Reported by diagnostics, because "your
    /// passwords are in a file" is something the user is entitled to know.
    pub async fn backing(&self) -> Backing {
        match self.service().await {
            Some(_) => Backing::SecretService,
            None => Backing::File,
        }
    }

    pub async fn store(
        &self,
        identifier: Uuid,
        purpose: SecretPurpose,
        secret: &[u8],
    ) -> Result<Backing> {
        if let Some(service) = self.service().await {
            if service
                .write(attributes(identifier, purpose), label(purpose), secret)
                .await
                .is_ok()
            {
                return Ok(Backing::SecretService);
            }
            // The keyring is there and has refused this one write. The file is
            // still a safe place to land, because `load` reads it whenever the
            // keyring holds nothing.
        }
        let mut store = read_file(&self.fallback_path)?;
        store.secrets.insert(
            file_key(identifier, purpose),
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, secret),
        );
        write_file(&self.fallback_path, &store)?;
        Ok(Backing::File)
    }

    pub async fn load(&self, identifier: Uuid, purpose: SecretPurpose) -> Result<Option<Vec<u8>>> {
        if let Some(service) = self.service().await {
            // An item that is there but will not be read is a failure, not an
            // answer. Reporting `Ok(None)` here is what let `signing_key`
            // conclude the key was missing and mint a second one, and a token
            // signed with the first then failed to verify on apply.
            if let Some(item) = service
                .search(&attributes(identifier, purpose))
                .await
                .context("the keyring would not answer a search")?
                .first()
            {
                return service
                    .read(&item.as_ref())
                    .await
                    .map(Some)
                    .context("the keyring holds this secret but would not hand it over");
            }
        }
        // Not in the keyring, or this machine has none: a secret written
        // before a keyring existed still has to be readable.
        let store = read_file(&self.fallback_path)?;
        match store.secrets.get(&file_key(identifier, purpose)) {
            Some(encoded) => Ok(Some(
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                    .context("a stored secret is not valid base64")?,
            )),
            None => Ok(None),
        }
    }

    /// Remove a secret from wherever it is. Both stores are cleared, so a
    /// secret written before a keyring existed does not survive the delete.
    pub async fn forget(&self, identifier: Uuid, purpose: SecretPurpose) -> Result<()> {
        // Both stores are swept, so a secret written before a keyring existed
        // does not survive the delete.
        if let Some(service) = self.service().await {
            if let Ok(items) = service.search(&attributes(identifier, purpose)).await {
                for item in items {
                    let _ = service.delete(&item.as_ref()).await;
                }
            }
        }
        let mut store = read_file(&self.fallback_path)?;
        if store
            .secrets
            .remove(&file_key(identifier, purpose))
            .is_some()
        {
            write_file(&self.fallback_path, &store)?;
        }
        Ok(())
    }

    /// Fetch a key, creating one only if nobody else already has.
    ///
    /// Edith calls this `storeIfAbsent`, and the name is the important part.
    /// The naive version — read, and if empty generate and write — has a race:
    /// two callers both find nothing, both generate a *different* key, and both
    /// write. Whichever lands last is what the store holds, so the other caller
    /// walks away with a key that will not verify its own tokens. Reading back
    /// after writing is what makes both callers converge on the same key.
    pub async fn signing_key(&self, identifier: Uuid, purpose: SecretPurpose) -> Result<Vec<u8>> {
        if let Some(existing) = self.load(identifier, purpose).await? {
            if existing.len() == crate::guard::SIGNING_KEY_BYTES {
                return Ok(existing);
            }
        }

        // Nothing there a moment ago. Take the mint lock and look again: while
        // this caller was reading, another may have written, and read-back
        // alone cannot settle it because the keyring's own `CreateItem`
        // replaces rather than refuses. Whoever holds the lock first writes;
        // everyone behind them finds that key on this second read.
        let _minting = self.minting.lock().await;
        if let Some(existing) = self.load(identifier, purpose).await? {
            if existing.len() == crate::guard::SIGNING_KEY_BYTES {
                return Ok(existing);
            }
        }

        let mut proposed = vec![0u8; crate::guard::SIGNING_KEY_BYTES];
        getrandom(&mut proposed)?;
        self.store(identifier, purpose, &proposed).await?;

        // Whatever is in the store now is the key, which may be another
        // caller's rather than the one just proposed.
        match self.load(identifier, purpose).await? {
            Some(stored) if stored.len() == crate::guard::SIGNING_KEY_BYTES => Ok(stored),
            _ => Ok(proposed),
        }
    }
}

/// Random bytes from the kernel.
///
/// `/dev/urandom` rather than a userspace generator: this key is what stands
/// between a token and a forged one, and the kernel's pool is the thing that is
/// actually seeded properly.
fn getrandom(buffer: &mut [u8]) -> Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .context("cannot open /dev/urandom")?
        .read_exact(buffer)
        .context("cannot read from /dev/urandom")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "veronica-secrets-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("secrets.json")
    }

    #[test]
    fn the_attributes_name_no_host_and_no_database() {
        // The keyring's own listing must not say which servers you connect to.
        let attributes = attributes(Uuid::from_u128(7), SecretPurpose::Password);
        let rendered = format!("{attributes:?}");
        assert!(rendered.contains(APPLICATION));
        assert!(rendered.contains("password"));
        assert_eq!(attributes.len(), 3, "only application, identifier, purpose");
        assert_eq!(label(SecretPurpose::Password), "Veronica database password");
    }

    #[test]
    fn two_purposes_for_one_connection_are_stored_separately() {
        // A password and a client key are different secrets, and reading one
        // when the other was asked for would be a confusing failure at best.
        let identifier = Uuid::from_u128(1);
        assert_ne!(
            attributes(identifier, SecretPurpose::Password),
            attributes(identifier, SecretPurpose::ClientPrivateKey)
        );
        assert_ne!(
            file_key(identifier, SecretPurpose::Password),
            file_key(identifier, SecretPurpose::ClientPrivateKey)
        );
    }

    #[tokio::test]
    async fn a_secret_round_trips_and_can_be_forgotten() {
        // Against whichever store this machine has; both must behave the same.
        let store = SecretStore::new(scratch("roundtrip"));
        let identifier = Uuid::new_v4();
        assert!(store
            .load(identifier, SecretPurpose::Password)
            .await
            .unwrap()
            .is_none());

        store
            .store(identifier, SecretPurpose::Password, b"hunter2")
            .await
            .unwrap();
        assert_eq!(
            store
                .load(identifier, SecretPurpose::Password)
                .await
                .unwrap(),
            Some(b"hunter2".to_vec())
        );

        store
            .forget(identifier, SecretPurpose::Password)
            .await
            .unwrap();
        assert!(store
            .load(identifier, SecretPurpose::Password)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn a_secret_with_awkward_bytes_survives() {
        // A key file is binary and a passphrase can contain a newline.
        let store = SecretStore::new(scratch("bytes"));
        let identifier = Uuid::new_v4();
        let secret = vec![0u8, 10, 13, 255, 128, b'"', b'\\'];
        store
            .store(identifier, SecretPurpose::ClientPrivateKey, &secret)
            .await
            .unwrap();
        assert_eq!(
            store
                .load(identifier, SecretPurpose::ClientPrivateKey)
                .await
                .unwrap(),
            Some(secret)
        );
        store
            .forget(identifier, SecretPurpose::ClientPrivateKey)
            .await
            .unwrap();
    }

    #[test]
    fn the_fallback_file_is_written_unreadable_to_anyone_else() {
        use std::os::unix::fs::PermissionsExt;

        let path = scratch("modes");
        let mut store = FileStore::default();
        store.secrets.insert("a".into(), "b".into());
        write_file(&path, &store).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the file is world-readable");
        let directory = std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory, 0o700, "the directory is world-listable");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_missing_or_damaged_fallback_file_is_empty_rather_than_fatal() {
        let path = scratch("damaged");
        assert!(read_file(&path).unwrap().secrets.is_empty());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        assert!(read_file(&path).unwrap().secrets.is_empty());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[tokio::test]
    async fn a_signing_key_is_generated_once_and_then_reused() {
        // A key that changed between runs would invalidate every outstanding
        // preview on restart.
        let store = SecretStore::new(scratch("signing"));
        let identifier = Uuid::new_v4();
        let first = store
            .signing_key(identifier, SecretPurpose::ConfirmationSigningKey)
            .await
            .unwrap();
        let second = store
            .signing_key(identifier, SecretPurpose::ConfirmationSigningKey)
            .await
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), crate::guard::SIGNING_KEY_BYTES);
        store
            .forget(identifier, SecretPurpose::ConfirmationSigningKey)
            .await
            .unwrap();
    }

    #[test]
    fn random_bytes_are_actually_random() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        getrandom(&mut first).unwrap();
        getrandom(&mut second).unwrap();
        assert_ne!(first, second);
        assert_ne!(first, [0u8; 32]);
    }

    #[test]
    fn the_backing_says_plainly_which_store_is_better() {
        assert!(Backing::SecretService.is_preferred());
        assert!(!Backing::File.is_preferred());
        assert!(Backing::File.title().contains("0600"));
    }

    #[tokio::test]
    async fn racing_callers_end_up_with_the_same_signing_key() {
        // The naive read-then-write leaves one caller holding a key the store
        // does not have, and its own tokens then fail to verify.
        let store = std::sync::Arc::new(SecretStore::new(scratch("race")));
        let identifier = Uuid::new_v4();
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let store = store.clone();
            tasks.push(tokio::spawn(async move {
                store
                    .signing_key(identifier, SecretPurpose::ConfirmationSigningKey)
                    .await
                    .unwrap()
            }));
        }
        let keys: Vec<Vec<u8>> = futures_util::future::join_all(tasks)
            .await
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert!(
            keys.windows(2).all(|pair| pair[0] == pair[1]),
            "callers disagreed about the signing key"
        );
        store
            .forget(identifier, SecretPurpose::ConfirmationSigningKey)
            .await
            .unwrap();
    }
}
