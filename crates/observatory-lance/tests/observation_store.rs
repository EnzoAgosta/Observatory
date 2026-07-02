//! Integration tests for `LanceObservationStore` against a real on-disk Lance
//! dataset.
//!
//! Each test creates its dataset in a fresh tempdir, so they are hermetic and
//! parallel-safe. They exercise the public API only (the trait surface from
//! `observatory-store`, plus Lance-specific lifecycle and maintenance), which
//! is also the contract the trait distilled from the concrete Lance store.

use std::time::{Duration, SystemTime};

use futures::stream;
use lance::dataset::optimize::CompactionOptions;
use lance_index::optimize::OptimizeOptions;
use observatory_core::identity::{AtomId, id_from_atom};
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_lance::LanceObservationStore;
use observatory_observations::Kind;
use observatory_observations::identity::id_from_observation;
use observatory_observations::{Observation, ObservationId};
use observatory_store::ObservationStore;
use proptest::prelude::*;
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
    assert!(LanceObservationStore::open(&uri).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn create_errors_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    LanceObservationStore::create(&uri).await.unwrap();
    assert!(
        LanceObservationStore::create(&uri).await.is_err(),
        "create must not clobber an existing dataset"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn put_then_get_by_id_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();

    let obs = translation_observation();
    let id = id_from_observation(&obs);
    store.put_observations(stream::iter([obs.clone()])).await.unwrap();

    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs));
}

#[tokio::test(flavor = "multi_thread")]
async fn put_then_get_by_kind_returns_only_matching() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();

    let translation = translation_observation();
    let approval = approval_observation();
    store
        .put_observations(stream::iter([translation.clone(), approval]))
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
        let mut store = LanceObservationStore::create(&uri).await.unwrap();
        store.put_observations(stream::iter([obs.clone()])).await.unwrap();
    }

    let store = LanceObservationStore::open(&uri).await.unwrap();
    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs));
}

#[tokio::test(flavor = "multi_thread")]
async fn re_put_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();
    let obs = translation_observation();
    let id = id_from_observation(&obs);

    store.put_observations(stream::iter([obs.clone()])).await.unwrap();
    store.put_observations(stream::iter([obs.clone()])).await.unwrap();

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
    let mut store = LanceObservationStore::create(&uri).await.unwrap();
    let obs = translation_observation();
    let id = id_from_observation(&obs);

    store
        .put_observations(stream::iter([obs.clone(), obs.clone(), obs.clone()]))
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
    let mut store = LanceObservationStore::create(&uri).await.unwrap();
    let obs = translation_observation();
    let id = id_from_observation(&obs);

    store.put_observations(stream::empty::<Observation>()).await.unwrap();
    store.put_observations(stream::iter([obs.clone()])).await.unwrap();
    store.put_observations(stream::empty::<Observation>()).await.unwrap();

    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs));
}

#[tokio::test(flavor = "multi_thread")]
async fn get_by_unknown_id_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let store = LanceObservationStore::create(&uri).await.unwrap();
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
    let store = LanceObservationStore::create(&uri).await.unwrap();
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
    let mut store = LanceObservationStore::create(&uri).await.unwrap();

    let obs = Observation::property(
        kind("approved_by"),
        atom_id("en-US", "Hello"),
        SystemTime::UNIX_EPOCH + Duration::from_secs(86400),
        Some(SystemTime::UNIX_EPOCH - Duration::from_secs(60 * 60 * 24 * 365)),
        json!({ "reviewer": "alice" }),
    );
    let id = id_from_observation(&obs);
    store.put_observations(stream::iter([obs.clone()])).await.unwrap();

    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs));
}

#[tokio::test(flavor = "multi_thread")]
async fn distinct_observations_sharing_subjects_both_persist() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();

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
        .put_observations(stream::iter([approved.clone(), blacklisted.clone()]))
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

// --- indexes & maintenance ---

#[tokio::test(flavor = "multi_thread")]
async fn ensure_indexes_succeeds_after_puts() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();
    store
        .put_observations(stream::iter([translation_observation()]))
        .await
        .unwrap();
    store.ensure_indexes().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn ensure_indexes_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();
    store
        .put_observations(stream::iter([translation_observation()]))
        .await
        .unwrap();
    store.ensure_indexes().await.unwrap();
    store.ensure_indexes().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn get_observations_by_subject_returns_matching_observations() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();

    let en_hello = atom_id("en-US", "Hello");
    let fr_bonjour = atom_id("fr-FR", "Bonjour");
    let de_hallo = atom_id("de-DE", "Hallo");

    let translation = Observation::relationship(
        kind("translation_of"),
        vec![en_hello, fr_bonjour],
        SystemTime::UNIX_EPOCH,
        None,
        json!({}),
    );
    let approval = Observation::property(
        kind("approved_by"),
        en_hello,
        SystemTime::UNIX_EPOCH,
        None,
        json!({}),
    );
    let unrelated = Observation::property(
        kind("context_for"),
        de_hallo,
        SystemTime::UNIX_EPOCH,
        None,
        json!({}),
    );
    store
        .put_observations(stream::iter([translation.clone(), approval.clone(), unrelated]))
        .await
        .unwrap();

    let about_en_hello = store.get_observations_by_subject(en_hello).await.unwrap();
    assert_eq!(about_en_hello.len(), 2);
    let ids: Vec<ObservationId> = about_en_hello
        .iter()
        .map(id_from_observation)
        .collect();
    assert!(ids.contains(&id_from_observation(&translation)));
    assert!(ids.contains(&id_from_observation(&approval)));
}

#[tokio::test(flavor = "multi_thread")]
async fn get_observations_by_subject_unknown_atom_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let store = LanceObservationStore::create(&uri).await.unwrap();
    let absent = AtomId::from_digest([0u8; 32]);
    assert!(
        store
            .get_observations_by_subject(absent)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_observations_by_subject_returns_same_after_indexing() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();

    let en_hello = atom_id("en-US", "Hello");
    let obs = Observation::property(
        kind("approved_by"),
        en_hello,
        SystemTime::UNIX_EPOCH,
        None,
        json!({}),
    );
    store.put_observations(stream::iter([obs.clone()])).await.unwrap();

    let before = store.get_observations_by_subject(en_hello).await.unwrap();
    store.ensure_indexes().await.unwrap();
    let after = store.get_observations_by_subject(en_hello).await.unwrap();
    assert_eq!(before.len(), after.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn optimize_indexes_covers_new_writes() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();

    let en_hello = atom_id("en-US", "Hello");
    let first = Observation::property(
        kind("approved_by"),
        en_hello,
        SystemTime::UNIX_EPOCH,
        None,
        json!({}),
    );
    store.put_observations(stream::iter([first.clone()])).await.unwrap();
    store.ensure_indexes().await.unwrap();

    let second = Observation::property(
        kind("blacklisted"),
        en_hello,
        SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        None,
        json!({}),
    );
    store.put_observations(stream::iter([second.clone()])).await.unwrap();
    store
        .optimize_indexes(&OptimizeOptions::default())
        .await
        .unwrap();

    let got = store.get_observations_by_subject(en_hello).await.unwrap();
    assert_eq!(got.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn compact_preserves_all_observations() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();
    let observations: Vec<Observation> = (0..4)
        .map(|i| {
            Observation::property(
                kind("approved_by"),
                atom_id("en-US", &format!("atom_{i}")),
                SystemTime::UNIX_EPOCH + Duration::from_secs(i),
                None,
                json!({}),
            )
        })
        .collect();
    for obs in &observations {
        store.put_observations(stream::iter([obs.clone()])).await.unwrap();
    }

    store.compact(&CompactionOptions::default()).await.unwrap();

    for obs in &observations {
        let id = id_from_observation(obs);
        assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(obs.clone()));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cleanup_versions_preserves_latest_observations() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = LanceObservationStore::create(&uri).await.unwrap();
    let first = translation_observation();
    let second = approval_observation();
    let third = Observation::property(
        kind("context_for"),
        atom_id("en-US", "Hello"),
        SystemTime::UNIX_EPOCH + Duration::from_secs(3000),
        None,
        json!({}),
    );
    store.put_observations(stream::iter([first.clone()])).await.unwrap();
    store.put_observations(stream::iter([second.clone()])).await.unwrap();
    store.put_observations(stream::iter([third.clone()])).await.unwrap();

    let stats = store.cleanup_versions(1).await.unwrap();
    assert!(stats.old_versions >= 2);

    let id = id_from_observation(&third);
    assert_eq!(store.get_observation_by_id(id).await.unwrap(), Some(third));
}

// --- proptest ---

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    /// For any batch of observations, `get_observations_by_subject(subject)` returns
    /// exactly the observations whose `subjects` list contains `subject` —
    /// no more, no less.
    #[test]
    fn get_observations_by_subject_finds_exactly_matching(
        observations in prop::collection::vec(observation_strategy(), 0..8)
    ) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let passed = runtime.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let uri = uri_in(&dir);
            let mut store = LanceObservationStore::create(&uri).await.unwrap();
            store.put_observations(stream::iter(observations.clone())).await.unwrap();

            let mut all_subjects: Vec<AtomId> = Vec::new();
            for obs in &observations {
                all_subjects.extend(obs.subjects());
            }

            for subject in all_subjects {
                let got = store.get_observations_by_subject(subject).await.unwrap();
                let expected: usize = observations
                    .iter()
                    .filter(|obs| obs.subjects().contains(&subject))
                    .count();
                if got.len() != expected {
                    return false;
                }
            }
            true
        });

        prop_assert!(passed, "get_observations_by_subject returned the wrong count");
    }
}

fn observation_strategy() -> impl Strategy<Value = Observation> {
    let kinds = prop::sample::select(vec![
        "translation_of",
        "approved_by",
        "blacklisted",
        "context_for",
    ]);
    let subjects = prop::collection::vec(any::<[u8; 32]>(), 1..3);
    (kinds, subjects).prop_map(|(kind_label, subjects)| {
        Observation::property(
            Kind::new(kind_label).unwrap(),
            AtomId::from_digest(subjects[0]),
            SystemTime::UNIX_EPOCH,
            None,
            json!({}),
        )
    })
}
