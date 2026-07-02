//! Integration tests for `AtomStore` against a real on-disk Lance dataset.
//!
//! Each test creates its dataset in a fresh tempdir, so they are hermetic and
//! parallel-safe. They exercise the public API only (lifecycle, the `row_digest`
//! dedup, and retrieval by the non-unique `atom_id`), which is also the contract a
//! future store trait would be distilled from.

use observatory_core::identity::id_from_atom;
use observatory_core::ir::{Atom, ContentNode, LanguageTag};
use observatory_store::AtomStore;
use lance::dataset::optimize::CompactionOptions;
use lance_index::optimize::OptimizeOptions;
use proptest::prelude::*;
use tempfile::TempDir;

fn atom(language: &str, nodes: Vec<ContentNode>) -> Atom {
    Atom::new(LanguageTag::from_string(language).unwrap(), nodes)
}

/// The dataset URI inside a tempdir. The `TempDir` must be kept alive by the
/// caller — dropping it deletes the dataset.
fn uri_in(dir: &TempDir) -> String {
    dir.path()
        .join("atoms.lance")
        .to_str()
        .expect("tempdir path is valid UTF-8")
        .to_owned()
}

/// Sorts atoms by their reconstructed string so a multi-row result can be compared
/// deterministically — the store returns matches in no guaranteed order.
fn sorted(mut atoms: Vec<Atom>) -> Vec<Atom> {
    atoms.sort_by_key(Atom::reconstruct);
    atoms
}

#[tokio::test(flavor = "multi_thread")]
async fn open_errors_when_absent() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    assert!(AtomStore::open(&uri).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn create_errors_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    AtomStore::create(&uri).await.unwrap();
    assert!(
        AtomStore::create(&uri).await.is_err(),
        "create must not clobber an existing dataset"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_unknown_id_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let store = AtomStore::create(&uri).await.unwrap();
    let absent = atom("en-US", vec![ContentNode::text("never stored")]);
    assert!(
        store
            .get_atoms_by_id(id_from_atom(&absent))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn put_then_get_roundtrips_mixed_content() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();

    let a = atom(
        "de-DE",
        vec![
            ContentNode::text("Klicken Sie "),
            ContentNode::placeholder("<b>"),
            ContentNode::text("hier"),
            ContentNode::placeholder("</b>"),
        ],
    );
    store.put_atoms(std::slice::from_ref(&a)).await.unwrap();

    assert_eq!(
        store.get_atoms_by_id(id_from_atom(&a)).await.unwrap(),
        vec![a]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn data_persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let a = atom("fr-FR", vec![ContentNode::text("bonjour")]);

    {
        let mut store = AtomStore::create(&uri).await.unwrap();
        store.put_atoms(std::slice::from_ref(&a)).await.unwrap();
    }

    let store = AtomStore::open(&uri).await.unwrap();
    assert_eq!(
        store.get_atoms_by_id(id_from_atom(&a)).await.unwrap(),
        vec![a]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn re_put_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    let a = atom("en-US", vec![ContentNode::text("hello")]);

    store.put_atoms(std::slice::from_ref(&a)).await.unwrap();
    store.put_atoms(std::slice::from_ref(&a)).await.unwrap();

    assert_eq!(
        store.get_atoms_by_id(id_from_atom(&a)).await.unwrap(),
        vec![a],
        "re-putting a byte-identical atom must not duplicate the row"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn within_batch_duplicates_collapse() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    let a = atom("en-US", vec![ContentNode::text("hello")]);

    store
        .put_atoms(&[a.clone(), a.clone(), a.clone()])
        .await
        .unwrap();

    assert_eq!(
        store.get_atoms_by_id(id_from_atom(&a)).await.unwrap(),
        vec![a]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn markup_variants_share_id_and_both_return() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();

    // Same text/placeholder structure, different placeholder markup: identical
    // atom_id, distinct atoms that must both persist and both come back.
    let ph = atom("en-US", vec![ContentNode::placeholder("<ph/>")]);
    let bpt = atom("en-US", vec![ContentNode::placeholder("<bpt/>")]);
    assert_eq!(id_from_atom(&ph), id_from_atom(&bpt));

    store.put_atoms(&[ph.clone(), bpt.clone()]).await.unwrap();

    let got = store.get_atoms_by_id(id_from_atom(&ph)).await.unwrap();
    assert_eq!(sorted(got), sorted(vec![ph, bpt]));
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_put_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    let a = atom("en-US", vec![ContentNode::text("hello")]);

    store.put_atoms(&[]).await.unwrap();
    store.put_atoms(std::slice::from_ref(&a)).await.unwrap();
    store.put_atoms(&[]).await.unwrap();

    assert_eq!(
        store.get_atoms_by_id(id_from_atom(&a)).await.unwrap(),
        vec![a]
    );
}

// --- indexes & maintenance ---

#[tokio::test(flavor = "multi_thread")]
async fn ensure_indexes_succeeds_after_puts() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    store
        .put_atoms(std::slice::from_ref(&atom(
            "en-US",
            vec![ContentNode::text("hello")],
        )))
        .await
        .unwrap();
    store.ensure_indexes().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn ensure_indexes_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    store
        .put_atoms(std::slice::from_ref(&atom(
            "en-US",
            vec![ContentNode::text("hello")],
        )))
        .await
        .unwrap();
    store.ensure_indexes().await.unwrap();
    store.ensure_indexes().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn get_atoms_by_id_returns_same_results_after_indexing() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    let a = atom("en-US", vec![ContentNode::text("hello")]);
    let b = atom("en-US", vec![ContentNode::placeholder("<b>")]);
    store.put_atoms(&[a.clone(), b.clone()]).await.unwrap();

    let before = sorted(store.get_atoms_by_id(id_from_atom(&a)).await.unwrap());
    store.ensure_indexes().await.unwrap();
    let after = sorted(store.get_atoms_by_id(id_from_atom(&a)).await.unwrap());
    assert_eq!(before, after);
}

#[tokio::test(flavor = "multi_thread")]
async fn optimize_indexes_covers_new_writes() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    let a = atom("en-US", vec![ContentNode::text("first")]);
    store.put_atoms(std::slice::from_ref(&a)).await.unwrap();
    store.ensure_indexes().await.unwrap();

    let b = atom("en-US", vec![ContentNode::text("second")]);
    store.put_atoms(std::slice::from_ref(&b)).await.unwrap();
    store
        .optimize_indexes(&OptimizeOptions::default())
        .await
        .unwrap();

    assert_eq!(
        store.get_atoms_by_id(id_from_atom(&b)).await.unwrap(),
        vec![b]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn compact_preserves_all_data() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    let atoms: Vec<Atom> = (0..4)
        .map(|i| atom("en-US", vec![ContentNode::text(format!("atom_{i}"))]))
        .collect();
    for a in &atoms {
        store.put_atoms(std::slice::from_ref(a)).await.unwrap();
    }

    store.compact(&CompactionOptions::default()).await.unwrap();

    for a in &atoms {
        assert_eq!(
            store.get_atoms_by_id(id_from_atom(a)).await.unwrap(),
            vec![a.clone()]
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cleanup_versions_preserves_latest_data() {
    let dir = tempfile::tempdir().unwrap();
    let uri = uri_in(&dir);
    let mut store = AtomStore::create(&uri).await.unwrap();
    let a = atom("en-US", vec![ContentNode::text("first")]);
    let b = atom("en-US", vec![ContentNode::text("second")]);
    let c = atom("en-US", vec![ContentNode::text("third")]);
    store.put_atoms(std::slice::from_ref(&a)).await.unwrap();
    store.put_atoms(std::slice::from_ref(&b)).await.unwrap();
    store.put_atoms(std::slice::from_ref(&c)).await.unwrap();

    let stats = store.cleanup_versions(1).await.unwrap();
    assert!(stats.old_versions >= 2, "should have removed old versions");

    assert_eq!(
        store.get_atoms_by_id(id_from_atom(&c)).await.unwrap(),
        vec![c]
    );
}

fn content_node_strategy() -> impl Strategy<Value = ContentNode> {
    prop_oneof![
        any::<String>().prop_map(ContentNode::Text),
        any::<String>().prop_map(ContentNode::Placeholder),
    ]
}

fn atom_strategy() -> impl Strategy<Value = Atom> {
    let languages = prop::sample::select(vec!["en-US", "fr-FR", "de-DE", "ja-JP", "zh-CN"]);
    (
        languages,
        prop::collection::vec(content_node_strategy(), 0..5),
    )
        .prop_map(|(language, nodes)| Atom::new(LanguageTag::from_string(language).unwrap(), nodes))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// After storing an arbitrary batch, every atom that went in is retrievable by
    /// its id (among possibly several markup variants sharing that id).
    #[test]
    fn every_put_atom_is_retrievable(atoms in prop::collection::vec(atom_strategy(), 0..6)) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let all_found = runtime.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let uri = uri_in(&dir);
            let mut store = AtomStore::create(&uri).await.unwrap();
            store.put_atoms(&atoms).await.unwrap();

            for wanted in &atoms {
                let matches = store.get_atoms_by_id(id_from_atom(wanted)).await.unwrap();
                if !matches.contains(wanted) {
                    return false;
                }
            }
            true
        });

        prop_assert!(all_found, "a stored atom was not retrievable by its id");
    }
}
