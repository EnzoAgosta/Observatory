//! Integration tests for `ObservationStore` against a real on-disk Lance dataset.
//!
//! Each test creates its dataset in a fresh tempdir, so they are hermetic and
//! parallel-safe. They exercise the public API only (lifecycle, the
//! content-addressed dedup, and retrieval by id or kind), which is also the
//! contract a future store trait would be distilled from.

use std::time::{Duration, SystemTime};

use observatory_core::identity::{AtomId, id_from_atom};
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_observations::Kind;
use observatory_observations::identity::id_from_observation;
use observatory_observations::{Observation, ObservationId};
use observatory_store::ObservationStore;
use serde_json::json;
use tempfile::TempDir;

fn atom_id(lang: &str, text: &str) -> AtomId {
    id_from_atom(&Atom::new(
        LanguageTag::from_string(lang).unwrap(),
        [ContentNode::text(text)],
    ))
}

fn kind(label: &str) -> Kind {
    Kind::new(label).unwrap()
}

fn uri_in(dir: &TempDir) -> String {
    dir.path()
        .join("observations.lance")
        .to_str()
        .expect("tempdir path is valid UTF-8")
        .to_owned()
}

fn translation_observation() -> Observation {
    Observation::relationship(
        kind("translation_of"),
        vec![atom_id("fr-FR", "Bonjour"), atom_id("en-US", "Hello")],
        SystemTime::UNIX_EPOCH + Duration::from_secs(1000),
        None,
        json!({ "author": "deepl:v2", "confidence": 0.91 }),
    )
}

fn approval_observation() -> Observation {
    Observation::property(
        kind("approved_by"),
        atom_id("en-US", "Hello"),
        SystemTime::UNIX_EPOCH + Duration::from_secs(2000),
        None,
        json!({ "reviewer": "alice" }),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn open_errors_when_absent() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    assert!(ObservationStore::open(&uri).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn create_errors_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    ObservationStore::create(&uri).await.unwrap();
    assert!(
        ObservationStore::create(&uri).await.is_err(),
        "create must not clobber an existing dataset"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn put_then_get_by_id_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = ObservationStore::create(&uri).await.unwrap();

    let obs = translation_observation();
    let id = id_from_observation(&obs);
    store.put_observations(std::slice::from_ref(&obs)).await.unwrap();

    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs));
}

#[tokio::test(flavor = "multi_thread")]
async fn put_then_get_by_kind_returns_only_matching() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = ObservationStore::create(&uri).await.unwrap();

    let translation = translation_observation();
    let approval = approval_observation();
    store
        .put_observations(&[translation.clone(), approval])
        .await
        .unwrap();

    let translations = store
        .get_observations_of_kind(&kind("translation_of"))
        .await
        .unwrap();
    assert_eq!(translations, vec![translation]);
}

#[tokio::test(flavor = "multi_thread")]
async fn data_persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let obs = translation_observation();
    let id = id_from_observation(&obs);

    {
        let mut store = ObservationStore::create(&uri).await.unwrap();
        store.put_observations(std::slice::from_ref(&obs)).await.unwrap();
    }

    let store = ObservationStore::open(&uri).await.unwrap();
    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs));
}

#[tokio::test(flavor = "multi_thread")]
async fn re_put_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = ObservationStore::create(&uri).await.unwrap();
    let obs = translation_observation();
    let id = id_from_observation(&obs);

    store.put_observations(std::slice::from_ref(&obs)).await.unwrap();
    store.put_observations(std::slice::from_ref(&obs)).await.unwrap();

    assert_eq!(
        store.get_observation_by_id(id).await.unwrap(),
        Some(obs),
        "re-putting a byte-identical observation must not duplicate the row"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn within_batch_duplicates_collapse() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = ObservationStore::create(&uri).await.unwrap();
    let obs = translation_observation();
    let id = id_from_observation(&obs);

    store
        .put_observations(&[obs.clone(), obs.clone(), obs.clone()])
        .await
        .unwrap();

    assert_eq!(
        store.get_observation_by_id(id).await.unwrap(),
        Some(obs)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_put_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = ObservationStore::create(&uri).await.unwrap();
    let obs = translation_observation();
    let id = id_from_observation(&obs);

    store.put_observations(&[]).await.unwrap();
    store.put_observations(std::slice::from_ref(&obs)).await.unwrap();
    store.put_observations(&[]).await.unwrap();

    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs));
}

#[tokio::test(flavor = "multi_thread")]
async fn get_by_unknown_id_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let store = ObservationStore::create(&uri).await.unwrap();
    let absent = ObservationId::from_digest([0u8; 32]);
    assert!(store
        .get_observation_by_id(absent)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn get_by_unknown_kind_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let store = ObservationStore::create(&uri).await.unwrap();
    assert!(
        store
            .get_observations_of_kind(&kind("never_used"))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn bitemporal_pre_epoch_effective_at_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = ObservationStore::create(&uri).await.unwrap();

    let obs = Observation::property(
        kind("approved_by"),
        atom_id("en-US", "Hello"),
        SystemTime::UNIX_EPOCH + Duration::from_secs(86400),
        Some(SystemTime::UNIX_EPOCH - Duration::from_secs(60 * 60 * 24 * 365)),
        json!({ "reviewer": "alice" }),
    );
    let id = id_from_observation(&obs);
    store.put_observations(std::slice::from_ref(&obs)).await.unwrap();

    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs));
}

#[tokio::test(flavor = "multi_thread")]
async fn distinct_observations_sharing_subjects_both_persist() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = ObservationStore::create(&uri).await.unwrap();

    let subject = atom_id("en-US", "Hello");
    let approved = Observation::property(
        kind("approved_by"),
        subject,
        SystemTime::UNIX_EPOCH,
        None,
        json!({ "reviewer": "alice" }),
    );
    let blacklisted = Observation::property(
        kind("blacklisted"),
        subject,
        SystemTime::UNIX_EPOCH,
        None,
        json!({ "reason": "offensive" }),
    );

    let approved_id = id_from_observation(&approved);
    let blacklisted_id = id_from_observation(&blacklisted);
    assert_ne!(approved_id, blacklisted_id);

    store
        .put_observations(&[approved.clone(), blacklisted.clone()])
        .await
        .unwrap();

    assert_eq!(
        store.get_observation_by_id(approved_id).await.unwrap(),
        Some(approved)
    );
    assert_eq!(
        store.get_observation_by_id(blacklisted_id).await.unwrap(),
        Some(blacklisted)
    );
}
