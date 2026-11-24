use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use veronica_pack::catalog::{build, CatalogOptions};
use veronica_pack::signing::{
    decode_private, decode_public, encode_public, generate, key_id, sign, verify, write_private,
};

fn payload() -> Vec<u8> {
    let options = CatalogOptions {
        revision: 1_795_564_800,
        expires_days: 30,
        commit: "feedbeef".to_string(),
    };
    build(Vec::new(), &options)
        .unwrap()
        .to_payload_bytes()
        .unwrap()
}

#[test]
fn a_signed_envelope_verifies_with_its_key() {
    let key = generate().unwrap();
    let public = decode_public(&encode_public(&key.verifying_key())).unwrap();
    let id = key_id(&public);
    assert!(id.starts_with("ext-") && id.len() == 16);
    let envelope = sign(&payload(), &key, &id).unwrap();
    assert_eq!(STANDARD.decode(&envelope.payload).unwrap(), payload());
    let catalog = verify(&envelope, &public, &id).unwrap();
    assert_eq!(catalog.revision, 1_795_564_800);
}

#[test]
fn private_keys_round_trip_through_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("signing.key");
    let key = generate().unwrap();
    write_private(&key, &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(decode_private(&text).unwrap().to_bytes(), key.to_bytes());
    assert!(write_private(&key, &path).is_err());
    let mode =
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions());
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn tampering_is_detected() {
    let key = generate().unwrap();
    let public = key.verifying_key();
    let id = key_id(&public);
    let envelope = sign(&payload(), &key, &id).unwrap();

    let mut flipped = envelope.clone();
    let mut bytes = payload();
    bytes[10] ^= 1;
    flipped.payload = STANDARD.encode(&bytes);
    let error = verify(&flipped, &public, &id).unwrap_err();
    assert_eq!(error.to_string(), "signature does not match the payload");

    let stranger = generate().unwrap().verifying_key();
    assert!(verify(&envelope, &stranger, &id).is_err());

    let error = verify(&envelope, &public, "ext-other").unwrap_err();
    assert_eq!(error.to_string(), "no signature from key ext-other");

    let mut relabeled = envelope;
    relabeled.signatures[0].key_id = "ext-other".to_string();
    assert!(verify(&relabeled, &public, &id).is_err());
}

#[test]
fn only_catalogs_are_signed() {
    let key = generate().unwrap();
    assert!(sign(b"{\"not\":\"a catalog\"}", &key, "ext-1").is_err());
}
