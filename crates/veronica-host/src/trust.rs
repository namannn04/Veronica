use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use veronica_protocol::catalog::{
    Catalog, EnvelopeSignature, SignatureAlgorithm, SignedEnvelope, CATALOG_SCHEMA, ENVELOPE_SCHEMA,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedKey {
    pub id: &'static str,
    pub public: &'static str,
}

pub const DEVELOPMENT_KEY: TrustedKey = TrustedKey {
    id: "ext-4401294f5a7a",
    public: "Sl7a5D3v0pfBjh2MXx1dGnviU5aoT6W8hncaQ9PbOHM=",
};

pub const KEYS: [Option<TrustedKey>; 2] = [Some(DEVELOPMENT_KEY), None];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrustError {
    #[error("catalog envelope is malformed: {0}")]
    Malformed(String),
    #[error("catalog envelope schema {0} is not supported")]
    EnvelopeSchema(u32),
    #[error("catalog schema {0} is not supported")]
    CatalogSchema(u32),
    #[error("catalog is not signed by a trusted key")]
    UnknownKey,
    #[error("catalog signature does not match its payload")]
    BadSignature,
}

#[derive(Deserialize)]
struct SchemaOnly {
    schema: u32,
}

impl TrustedKey {
    pub fn verifying_key(&self) -> Result<VerifyingKey, TrustError> {
        let bytes = STANDARD
            .decode(self.public)
            .map_err(|error| TrustError::Malformed(format!("key {}: {error}", self.id)))?;
        let raw: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| TrustError::Malformed(format!("key {} is not 32 bytes", self.id)))?;
        VerifyingKey::from_bytes(&raw)
            .map_err(|error| TrustError::Malformed(format!("key {}: {error}", self.id)))
    }
}

fn malformed(error: impl std::fmt::Display) -> TrustError {
    TrustError::Malformed(error.to_string())
}

fn schema_of(raw: &[u8]) -> Result<u32, TrustError> {
    serde_json::from_slice::<SchemaOnly>(raw)
        .map(|only| only.schema)
        .map_err(malformed)
}

fn signature_matches(entry: &EnvelopeSignature, key: &VerifyingKey, payload: &[u8]) -> bool {
    let SignatureAlgorithm::Ed25519 = entry.algorithm;
    let Ok(bytes) = STANDARD.decode(&entry.signature) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&bytes) else {
        return false;
    };
    key.verify_strict(payload, &signature).is_ok()
}

pub fn verify_envelope(raw: &[u8]) -> Result<Catalog, TrustError> {
    verify_with(raw, &KEYS)
}

pub fn verify_with(raw: &[u8], keys: &[Option<TrustedKey>]) -> Result<Catalog, TrustError> {
    let schema = schema_of(raw)?;
    if schema != ENVELOPE_SCHEMA {
        return Err(TrustError::EnvelopeSchema(schema));
    }
    let envelope: SignedEnvelope = serde_json::from_slice(raw).map_err(malformed)?;
    let payload = STANDARD.decode(&envelope.payload).map_err(malformed)?;
    let mut trusted = Vec::new();
    for entry in &envelope.signatures {
        if let Some(key) = keys.iter().flatten().find(|key| key.id == entry.key_id) {
            trusted.push((entry, key.verifying_key()?));
        }
    }
    if trusted.is_empty() {
        return Err(TrustError::UnknownKey);
    }
    if !trusted
        .iter()
        .any(|(entry, key)| signature_matches(entry, key, &payload))
    {
        return Err(TrustError::BadSignature);
    }
    let schema = schema_of(&payload)?;
    if schema != CATALOG_SCHEMA {
        return Err(TrustError::CatalogSchema(schema));
    }
    serde_json::from_slice(&payload).map_err(malformed)
}
