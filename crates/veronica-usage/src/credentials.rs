use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use serde_json::Value;

const OAUTH_KEY: &str = "claudeAiOauth";

pub const REFRESH_LEEWAY_SECS: i64 = 60;

#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub refresh_token_expires_at: Option<DateTime<Utc>>,
    document: Value,
}

impl Credentials {
    pub fn default_path() -> Option<PathBuf> {
        crate::paths_home().map(|home| home.join(".claude/.credentials.json"))
    }

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let document: Value = serde_json::from_slice(bytes).ok()?;
        let oauth = document.get(OAUTH_KEY)?;
        let access_token = oauth
            .get("accessToken")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())?
            .to_string();

        Some(Self {
            access_token,
            refresh_token: oauth
                .get("refreshToken")
                .and_then(Value::as_str)
                .filter(|token| !token.is_empty())
                .map(str::to_string),
            expires_at: millis_to_time(oauth.get("expiresAt")),
            refresh_token_expires_at: millis_to_time(oauth.get("refreshTokenExpiresAt")),
            document,
        })
    }

    pub fn read(path: &Path) -> Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Self::parse(&bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    pub fn needs_refresh(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            None => false,
            Some(expires_at) => expires_at <= now + chrono::Duration::seconds(REFRESH_LEEWAY_SECS),
        }
    }

    pub fn usable_refresh_token(&self, now: DateTime<Utc>) -> Option<&str> {
        let token = self.refresh_token.as_deref()?;
        if let Some(expires_at) = self.refresh_token_expires_at {
            if expires_at <= now {
                return None;
            }
        }
        Some(token)
    }

    pub fn applied(&self, response: &RefreshResponse, now: DateTime<Utc>) -> Value {
        let mut document = self.document.clone();
        let oauth = document.as_object_mut().and_then(|root| {
            root.entry(OAUTH_KEY)
                .or_insert_with(|| Value::Object(serde_json::Map::default()))
                .as_object_mut()
        });
        let Some(oauth) = oauth else {
            return document;
        };

        oauth.insert(
            "accessToken".into(),
            Value::String(response.access_token.clone()),
        );
        oauth.insert(
            "expiresAt".into(),
            Value::from(time_to_millis(
                now + chrono::Duration::seconds(crate::truncate_to_i64(response.expires_in)),
            )),
        );
        if let Some(refresh_token) = response
            .refresh_token
            .as_deref()
            .filter(|token| !token.is_empty())
        {
            oauth.insert("refreshToken".into(), Value::String(refresh_token.into()));
        }
        if let Some(expires_in) = response.refresh_token_expires_in {
            oauth.insert(
                "refreshTokenExpiresAt".into(),
                Value::from(time_to_millis(
                    now + chrono::Duration::seconds(crate::truncate_to_i64(expires_in)),
                )),
            );
        }
        document
    }

    pub fn persist(path: &Path, document: &Value) -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let body = serde_json::to_vec(document)?;
        let temp = path.with_extension("json.veronica-tmp");

        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temp)
                .with_context(|| format!("cannot create {}", temp.display()))?;
            file.write_all(&body)?;
            file.sync_all()?;
        }

        std::fs::rename(&temp, path)
            .with_context(|| format!("cannot replace {}", path.display()))?;
        Ok(())
    }

    pub fn describe(&self) -> String {
        match self.expires_at {
            Some(expires_at) => format!("token expiring {}", expires_at.to_rfc3339()),
            None => "token with no recorded expiry".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RefreshResponse {
    #[serde(rename = "access_token")]
    pub access_token: String,
    #[serde(rename = "refresh_token")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expires_in")]
    pub expires_in: f64,
    #[serde(rename = "refresh_token_expires_in")]
    pub refresh_token_expires_in: Option<f64>,
}

fn millis_to_time(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let millis = value?.as_f64()?;
    Utc.timestamp_millis_opt(crate::truncate_to_i64(millis))
        .single()
}

fn time_to_millis(time: DateTime<Utc>) -> i64 {
    time.timestamp_millis()
}

pub fn ensure_owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata =
        std::fs::metadata(path).with_context(|| format!("cannot inspect {}", path.display()))?;
    let uid = unsafe { libc_getuid() };
    if metadata.uid() != uid {
        bail!("{} belongs to another user", path.display());
    }
    Ok(())
}

unsafe fn libc_getuid() -> u32 {
    unsafe {
        extern "C" {
            fn getuid() -> u32;
        }
        getuid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_787_000_000 + secs, 0).unwrap()
    }

    fn document() -> String {
        format!(
            r#"{{
              "claudeAiOauth": {{
                "accessToken": "access-1",
                "refreshToken": "refresh-1",
                "expiresAt": {},
                "refreshTokenExpiresAt": {},
                "scopes": ["user:inference", "user:profile"],
                "subscriptionType": "team",
                "rateLimitTier": "default_raven"
              }},
              "somethingElse": {{"keep": true}}
            }}"#,
            at(3600).timestamp_millis(),
            at(600_000).timestamp_millis()
        )
    }

    #[test]
    fn parses_the_real_file_shape() {
        let credentials = Credentials::parse(document().as_bytes()).expect("should parse");
        assert_eq!(credentials.access_token, "access-1");
        assert_eq!(credentials.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(credentials.expires_at, Some(at(3600)));
        assert_eq!(credentials.refresh_token_expires_at, Some(at(600_000)));
    }

    #[test]
    fn a_file_without_a_token_is_not_signed_in_rather_than_an_error() {
        assert!(Credentials::parse(b"{}").is_none());
        assert!(Credentials::parse(br#"{"claudeAiOauth":{}}"#).is_none());
        assert!(Credentials::parse(br#"{"claudeAiOauth":{"accessToken":""}}"#).is_none());
        assert!(Credentials::parse(b"not json").is_none());
    }

    #[test]
    fn a_missing_file_is_absent_not_an_error() {
        assert!(
            Credentials::read(Path::new("/nonexistent/.credentials.json"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn refresh_is_due_within_the_leeway_but_not_before() {
        let credentials = Credentials::parse(document().as_bytes()).unwrap();
        assert!(!credentials.needs_refresh(at(0)));
        assert!(!credentials.needs_refresh(at(3600 - REFRESH_LEEWAY_SECS - 1)));
        assert!(credentials.needs_refresh(at(3600 - REFRESH_LEEWAY_SECS)));
        assert!(credentials.needs_refresh(at(4000)));
    }

    #[test]
    fn no_recorded_expiry_means_do_not_churn_the_token() {
        let credentials = Credentials::parse(br#"{"claudeAiOauth":{"accessToken":"a"}}"#).unwrap();
        assert!(!credentials.needs_refresh(at(0)));
    }

    #[test]
    fn an_expired_refresh_token_is_not_usable() {
        let credentials = Credentials::parse(document().as_bytes()).unwrap();
        assert_eq!(credentials.usable_refresh_token(at(0)), Some("refresh-1"));
        assert_eq!(credentials.usable_refresh_token(at(600_001)), None);
    }

    #[test]
    fn applying_a_refresh_preserves_every_other_field() {
        let credentials = Credentials::parse(document().as_bytes()).unwrap();
        let response = RefreshResponse {
            access_token: "access-2".into(),
            refresh_token: Some("refresh-2".into()),
            expires_in: 3600.0,
            refresh_token_expires_in: Some(1_000_000.0),
        };
        let updated = credentials.applied(&response, at(10));
        let oauth = &updated["claudeAiOauth"];

        assert_eq!(oauth["accessToken"], "access-2");
        assert_eq!(oauth["refreshToken"], "refresh-2");
        assert_eq!(
            oauth["expiresAt"].as_i64().unwrap(),
            at(3610).timestamp_millis()
        );
        assert_eq!(
            oauth["refreshTokenExpiresAt"].as_i64().unwrap(),
            at(1_000_010).timestamp_millis()
        );
        assert_eq!(oauth["subscriptionType"], "team");
        assert_eq!(oauth["rateLimitTier"], "default_raven");
        assert_eq!(oauth["scopes"][0], "user:inference");
        assert_eq!(updated["somethingElse"]["keep"], true);
    }

    #[test]
    fn a_response_without_a_new_refresh_token_keeps_the_existing_one() {
        let credentials = Credentials::parse(document().as_bytes()).unwrap();
        let response = RefreshResponse {
            access_token: "access-2".into(),
            refresh_token: None,
            expires_in: 3600.0,
            refresh_token_expires_in: None,
        };
        let updated = credentials.applied(&response, at(0));
        assert_eq!(updated["claudeAiOauth"]["refreshToken"], "refresh-1");
        assert_eq!(
            updated["claudeAiOauth"]["refreshTokenExpiresAt"]
                .as_i64()
                .unwrap(),
            at(600_000).timestamp_millis()
        );
    }

    #[test]
    fn an_empty_refresh_token_in_the_response_is_ignored() {
        let credentials = Credentials::parse(document().as_bytes()).unwrap();
        let response = RefreshResponse {
            access_token: "access-2".into(),
            refresh_token: Some(String::new()),
            expires_in: 3600.0,
            refresh_token_expires_in: None,
        };
        let updated = credentials.applied(&response, at(0));
        assert_eq!(updated["claudeAiOauth"]["refreshToken"], "refresh-1");
    }

    #[test]
    fn persisting_writes_owner_only_and_replaces_atomically() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("veronica-cred-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".credentials.json");
        std::fs::write(&path, document()).unwrap();

        let credentials = Credentials::read(&path).unwrap().unwrap();
        let response = RefreshResponse {
            access_token: "access-2".into(),
            refresh_token: None,
            expires_in: 3600.0,
            refresh_token_expires_in: None,
        };
        Credentials::persist(&path, &credentials.applied(&response, at(0))).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "a credentials file must not be readable by others"
        );

        let reloaded = Credentials::read(&path).unwrap().unwrap();
        assert_eq!(reloaded.access_token, "access-2");
        assert!(!path.with_extension("json.veronica-tmp").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn describe_never_reveals_the_token() {
        let credentials = Credentials::parse(document().as_bytes()).unwrap();
        let described = credentials.describe();
        assert!(!described.contains("access-1"));
        assert!(!described.contains("refresh-1"));
    }
}
