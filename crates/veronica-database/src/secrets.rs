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
pub const APPLICATION: &str = "io.github.namannn04.Veronica";
const REPLACE_EXISTING_ITEM: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Backing {
    SecretService,
    File,
}

impl Backing {
    pub fn title(self) -> &'static str {
        match self {
            Self::SecretService => "the desktop keyring",
            Self::File => "a 0600 file under the state directory",
        }
    }

    pub fn is_preferred(self) -> bool {
        self == Self::SecretService
    }
}

fn attributes(identifier: Uuid, purpose: SecretPurpose) -> HashMap<String, String> {
    HashMap::from([
        ("application".to_string(), APPLICATION.to_string()),
        ("identifier".to_string(), identifier.to_string()),
        ("purpose".to_string(), purpose.key().to_string()),
    ])
}

fn label(purpose: SecretPurpose) -> String {
    format!("Veronica database {}", purpose.key())
}

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
        let properties: HashMap<&str, ZValue<'_>> = HashMap::from([
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
                &(properties, value, REPLACE_EXISTING_ITEM),
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
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(store)?)?;
    std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&temp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SecretStore {
    fallback_path: PathBuf,
    service: std::sync::Arc<tokio::sync::OnceCell<Option<std::sync::Arc<Service>>>>,
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

    pub fn file_only(path: impl Into<PathBuf>) -> Self {
        let store = Self::new(path);
        store
            .service
            .set(None)
            .expect("a new store has not opened the keyring yet");
        store
    }

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
        let store = read_file(&self.fallback_path)?;
        match store.secrets.get(&file_key(identifier, purpose)) {
            Some(encoded) => Ok(Some(
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
                    .context("a stored secret is not valid base64")?,
            )),
            None => Ok(None),
        }
    }

    pub async fn forget(&self, identifier: Uuid, purpose: SecretPurpose) -> Result<()> {
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

    pub async fn signing_key(&self, identifier: Uuid, purpose: SecretPurpose) -> Result<Vec<u8>> {
        if let Some(existing) = self.load(identifier, purpose).await? {
            if existing.len() == crate::guard::SIGNING_KEY_BYTES {
                return Ok(existing);
            }
        }

        let _minting = self.minting.lock().await;
        if let Some(existing) = self.load(identifier, purpose).await? {
            if existing.len() == crate::guard::SIGNING_KEY_BYTES {
                return Ok(existing);
            }
        }

        let mut proposed = vec![0u8; crate::guard::SIGNING_KEY_BYTES];
        getrandom(&mut proposed)?;
        self.store(identifier, purpose, &proposed).await?;

        match self.load(identifier, purpose).await? {
            Some(stored) if stored.len() == crate::guard::SIGNING_KEY_BYTES => Ok(stored),
            _ => Ok(proposed),
        }
    }
}

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
        let attributes = attributes(Uuid::from_u128(7), SecretPurpose::Password);
        let rendered = format!("{attributes:?}");
        assert!(rendered.contains(APPLICATION));
        assert!(rendered.contains("password"));
        assert_eq!(attributes.len(), 3, "only application, identifier, purpose");
        assert_eq!(label(SecretPurpose::Password), "Veronica database password");
    }

    #[test]
    fn two_purposes_for_one_connection_are_stored_separately() {
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
        let store = SecretStore::file_only(scratch("roundtrip"));
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
        let store = SecretStore::file_only(scratch("bytes"));
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
        let store = SecretStore::file_only(scratch("signing"));
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
        let store = std::sync::Arc::new(SecretStore::file_only(scratch("race")));
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
