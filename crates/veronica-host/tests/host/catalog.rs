use std::fs;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::Value;
use tempfile::TempDir;
use veronica_core::paths::{AppDirectories, EnvOverrides};
use veronica_host::catalog::{
    is_stale, seed, CatalogState, CatalogStore, Provenance, RollbackError, SEED,
};
use veronica_host::trust::{verify_envelope, verify_with, TrustError, DEVELOPMENT_KEY, KEYS};
use veronica_host::Layout;
use veronica_pack::signing;
use veronica_protocol::catalog::{Catalog, CATALOG_URL};

use crate::common::{
    catalog_of, development_key, metadata, now, sign_payload, signed, FakeTransport, REVISION,
};

fn new_store() -> (TempDir, Layout, CatalogStore) {
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(&AppDirectories::with_env(
        home.path(),
        EnvOverrides::default(),
    ));
    (home, layout.clone(), CatalogStore::new(layout))
}

fn sample(revision: u64) -> Catalog {
    catalog_of(
        vec![metadata("probe", "1.0.1", &"a".repeat(64), 10)],
        revision,
    )
}

fn trust_error(raw: &[u8]) -> TrustError {
    verify_envelope(raw).unwrap_err()
}

fn reencode(raw: &[u8], edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut envelope: Value = serde_json::from_slice(raw).unwrap();
    edit(&mut envelope);
    serde_json::to_vec(&envelope).unwrap()
}

#[test]
fn the_development_key_and_seed_are_consistent() {
    let key = development_key().verifying_key();
    assert_eq!(signing::key_id(&key), DEVELOPMENT_KEY.id);
    assert_eq!(signing::encode_public(&key), DEVELOPMENT_KEY.public);
    for key in KEYS.iter().flatten() {
        key.verifying_key().unwrap();
    }
    let seed = seed();
    assert_eq!(verify_envelope(SEED).unwrap(), seed);
    assert!(seed.extensions.is_empty());
    let envelope = serde_json::from_slice(SEED).unwrap();
    signing::verify(&envelope, &key, DEVELOPMENT_KEY.id).unwrap();
}

#[test]
fn catalogs_signed_by_vr_pack_verify_here() {
    let catalog = sample(REVISION);
    assert_eq!(verify_envelope(&signed(&catalog)).unwrap(), catalog);
    let rotated = [None, Some(DEVELOPMENT_KEY)];
    assert_eq!(verify_with(&signed(&catalog), &rotated).unwrap(), catalog);
    assert_eq!(
        verify_with(&signed(&catalog), &[None, None]).unwrap_err(),
        TrustError::UnknownKey
    );
}

#[test]
fn tampered_envelopes_are_rejected() {
    let catalog = sample(REVISION);
    let good = signed(&catalog);

    let flipped = reencode(&good, |envelope| {
        let mut payload = STANDARD
            .decode(envelope["payload"].as_str().unwrap())
            .unwrap();
        let last = payload.len() - 2;
        payload[last] ^= 1;
        envelope["payload"] = STANDARD.encode(payload).into();
    });
    assert_eq!(trust_error(&flipped), TrustError::BadSignature);

    let stranger = signing::generate().unwrap();
    let payload = catalog.to_payload_bytes().unwrap();
    let wrong_key = sign_payload(&payload, &stranger, DEVELOPMENT_KEY.id);
    assert_eq!(trust_error(&wrong_key), TrustError::BadSignature);

    let unknown = sign_payload(&payload, &development_key(), "ext-000000000000");
    assert_eq!(trust_error(&unknown), TrustError::UnknownKey);

    let mut newer: Value = serde_json::from_slice(&payload).unwrap();
    newer["schema"] = 2.into();
    newer["shiny"] = true.into();
    let newer = sign_payload(
        &serde_json::to_vec(&newer).unwrap(),
        &development_key(),
        DEVELOPMENT_KEY.id,
    );
    assert_eq!(trust_error(&newer), TrustError::CatalogSchema(2));

    let envelope_v2 = reencode(&good, |envelope| envelope["schema"] = 2.into());
    assert_eq!(trust_error(&envelope_v2), TrustError::EnvelopeSchema(2));

    let garbage = reencode(&good, |envelope| envelope["payload"] = "%%%".into());
    assert!(matches!(trust_error(&garbage), TrustError::Malformed(_)));
    assert!(matches!(trust_error(b"[]"), TrustError::Malformed(_)));
}

#[test]
fn the_seed_serves_offline_until_a_catalog_is_fetched() {
    let (_home, layout, store) = new_store();
    let transport = FakeTransport::default();
    assert!(store.refresh(&transport, now()).is_err());
    let loaded = store.load(now()).unwrap();
    assert_eq!(loaded.provenance, Provenance::Seed);
    assert_eq!(loaded.catalog, seed());
    assert!(!layout.catalog_file().exists());
    assert_eq!(store.state().unwrap(), CatalogState::default());
}

#[test]
fn a_fetched_catalog_is_cached_and_revalidated_with_its_etag() {
    let (_home, layout, store) = new_store();
    let transport = FakeTransport::default();
    let catalog = sample(REVISION);
    transport.serve(CATALOG_URL, signed(&catalog), Some("\"r1\""));

    let fetched = store.refresh(&transport, now()).unwrap();
    assert_eq!(fetched.provenance, Provenance::Network);
    assert_eq!(fetched.catalog, catalog);
    assert!(!fetched.stale);
    assert_eq!(transport.last_etag(), None);
    assert_eq!(fs::read(layout.catalog_file()).unwrap(), signed(&catalog));
    let state = store.state().unwrap();
    assert_eq!(state.revision, REVISION);
    assert_eq!(state.etag.as_deref(), Some("\"r1\""));
    assert_eq!(state.last_success_at(), Some(now()));

    let later = now() + chrono::Duration::hours(7);
    let revalidated = store.refresh(&transport, later).unwrap();
    assert_eq!(transport.last_etag().as_deref(), Some("\"r1\""));
    assert_eq!(revalidated.provenance, Provenance::Cache);
    assert_eq!(revalidated.catalog, catalog);
    assert_eq!(store.state().unwrap().last_success_at(), Some(later));

    assert_eq!(store.load(now()).unwrap().provenance, Provenance::Cache);
    fs::remove_file(layout.catalog_file()).unwrap();
    let refetched = store.refresh(&transport, now()).unwrap();
    assert_eq!(transport.last_etag(), None);
    assert_eq!(refetched.provenance, Provenance::Network);
}

#[test]
fn older_revisions_are_refused_as_rollbacks() {
    let (_home, layout, store) = new_store();
    let transport = FakeTransport::default();
    let current = sample(REVISION + 10);
    transport.serve(CATALOG_URL, signed(&current), None);
    store.refresh(&transport, now()).unwrap();

    transport.serve(CATALOG_URL, signed(&sample(REVISION + 10)), None);
    store.refresh(&transport, now()).unwrap();

    transport.serve(CATALOG_URL, signed(&sample(REVISION)), None);
    let error = store.refresh(&transport, now()).unwrap_err();
    assert_eq!(
        error.downcast_ref::<RollbackError>(),
        Some(&RollbackError {
            offered: REVISION,
            floor: REVISION + 10,
        })
    );
    assert_eq!(fs::read(layout.catalog_file()).unwrap(), signed(&current));
    assert_eq!(store.state().unwrap().revision, REVISION + 10);

    let (_home, _layout, fresh) = new_store();
    transport.serve(CATALOG_URL, signed(&sample(seed().revision - 1)), None);
    let error = fresh.refresh(&transport, now()).unwrap_err();
    assert!(error.downcast_ref::<RollbackError>().is_some());
}

#[test]
fn a_tampered_cache_falls_back_to_the_seed() {
    let (_home, layout, store) = new_store();
    let transport = FakeTransport::default();
    transport.serve(CATALOG_URL, signed(&sample(REVISION)), Some("\"r1\""));
    store.refresh(&transport, now()).unwrap();
    let cached = fs::read(layout.catalog_file()).unwrap();
    let tampered = reencode(&cached, |envelope| {
        envelope["signatures"][0]["signature"] = STANDARD.encode([0_u8; 64]).into();
    });
    fs::write(layout.catalog_file(), tampered).unwrap();
    assert_eq!(store.load(now()).unwrap().provenance, Provenance::Seed);
    store.refresh(&transport, now()).unwrap();
    assert_eq!(transport.last_etag(), None);
}

#[test]
fn an_expired_catalog_is_served_with_a_stale_warning() {
    let (_home, _layout, store) = new_store();
    let transport = FakeTransport::default();
    let catalog = sample(REVISION);
    transport.serve(CATALOG_URL, signed(&catalog), None);
    let expiry = catalog.expires_at.parse().unwrap();
    assert!(!is_stale(&catalog, expiry));
    assert!(is_stale(&catalog, expiry + chrono::Duration::seconds(1)));
    let late = expiry + chrono::Duration::days(3);
    let loaded = store.refresh(&transport, late).unwrap();
    assert!(loaded.stale);
    assert_eq!(loaded.catalog, catalog);
    assert!(store.load(late).unwrap().stale);
    assert!(!store.load(now()).unwrap().stale);
}
