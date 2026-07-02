//! The Lance-backed store for atoms.
//!
//! `AtomStore` is a thin, unopinionated wrapper over exactly one Lance dataset
//! (`atoms.lance`). It faithfully stores and retrieves atoms and holds no policy:
//! it does not decide when to compact, how to split hot from cold data, or what
//! any observation means — those are the calling application's concerns. It offers
//! primitives, not workflows.
//!
//! The handle is an `Arc<Dataset>`: a Lance dataset is an immutable, versioned
//! snapshot, and a write yields a *new* version's handle rather than mutating in
//! place. Readers borrow `&self` and share the current snapshot; a writer borrows
//! `&mut self`, runs the write, and swaps the returned handle into the field.

use std::sync::Arc;

use arrow::array::{RecordBatch, RecordBatchIterator};
use arrow::error::ArrowError;
use lance::Dataset;
use lance::dataset::{MergeInsertBuilder, WhenMatched, WhenNotMatched, WriteMode, WriteParams};
use lance::dataset::cleanup::{CleanupPolicyBuilder, RemovalStats};
use lance::dataset::optimize::{CompactionOptions, compact_files};
use lance::index::DatasetIndexExt;
use lance_index::optimize::OptimizeOptions;
use lance_index::{IndexType, scalar::ScalarIndexParams};

use observatory_core::identity::AtomId;
use observatory_core::ir::Atom;

use crate::decode::decode_atoms;
use crate::encode::encode_atoms;
use crate::error::Result;
use crate::schema::{ATOM_ID_COLUMN, ROW_DIGEST_COLUMN, atoms_schema};

/// Name of the BTREE index over [`ATOM_ID_COLUMN`]. Stable across runs; reused
/// by `ensure_indexes` to make the operation idempotent.
const ATOM_ID_INDEX_NAME: &str = "atom_id_idx";

/// A handle to the atoms dataset at one version. Cloning the inner `Arc` is cheap;
/// the whole point is that many readers can share one open dataset.
pub struct AtomStore {
    dataset: Arc<Dataset>,
}

impl AtomStore {
    /// Opens the existing atoms dataset at `uri` (a Lance URI — a local path,
    /// `file://`, `s3://`, …). Errors if no dataset exists there; use
    /// [`create`](Self::create) to make a new one.
    pub async fn open(uri: &str) -> Result<Self> {
        let dataset = Dataset::open(uri).await?;
        Ok(Self {
            dataset: Arc::new(dataset),
        })
    }

    /// Creates a new, empty atoms dataset at `uri`, establishing the schema. Errors
    /// if a dataset already exists there; use [`open`](Self::open) for that.
    pub async fn create(uri: &str) -> Result<Self> {
        let empty = std::iter::empty::<std::result::Result<RecordBatch, ArrowError>>();
        let batches = RecordBatchIterator::new(empty, atoms_schema());
        let params = WriteParams {
            mode: WriteMode::Create,
            ..Default::default()
        };
        let dataset = Dataset::write(batches, uri, Some(params)).await?;
        Ok(Self {
            dataset: Arc::new(dataset),
        })
    }

    /// Stores `atoms`, deriving each key itself and upserting by exact identity
    /// (`row_digest`): an atom already present byte-for-byte is left untouched, so
    /// the call is idempotent, while a genuine variant is inserted. One call is one
    /// Lance commit. Passing no atoms is a no-op that writes no new version.
    pub async fn put_atoms(&mut self, atoms: &[Atom]) -> Result<()> {
        if atoms.is_empty() {
            return Ok(());
        }

        let batch = encode_atoms(atoms);
        let schema = batch.schema();
        let reader = RecordBatchIterator::new([Ok(batch)], schema);

        let mut builder = MergeInsertBuilder::try_new(
            Arc::clone(&self.dataset),
            vec![ROW_DIGEST_COLUMN.to_string()],
        )?;
        builder
            .when_matched(WhenMatched::DoNothing)
            .when_not_matched(WhenNotMatched::InsertAll);
        let job = builder.try_build()?;

        let (dataset, _stats) = job.execute_reader(reader).await?;
        self.dataset = dataset;
        Ok(())
    }

    /// Builds the scalar index over `atom_id` if it is not already present. After
    /// this returns, [`get_atoms_by_id`](Self::get_atoms_by_id) consults the
    /// index instead of scanning every row. Idempotent: a no-op (no new dataset
    /// version) when the index already exists.
    ///
    /// Rows written by [`put_atoms`](Self::put_atoms) *after* this call are not
    /// covered by the index until `optimize_indices` is run — Lance serves
    /// queries correctly in the meantime, but scans the unindexed fragments
    /// rather than using the index. When the store should call `optimize_indices`
    /// is the calling application's concern, not the store's.
    pub async fn ensure_indexes(&mut self) -> Result<()> {
        let existing = self
            .dataset
            .load_indices_by_name(ATOM_ID_INDEX_NAME)
            .await?;
        if existing.is_empty() {
            let mut dataset = (*self.dataset).clone();
            dataset
                .create_index(
                    &[ATOM_ID_COLUMN],
                    IndexType::BTree,
                    Some(ATOM_ID_INDEX_NAME.to_string()),
                    &ScalarIndexParams::default(),
                    false,
                )
                .await?;
            self.dataset = Arc::new(dataset);
        }
        Ok(())
    }

    /// Refreshes the `atom_id` index to cover rows written by
    /// [`put_atoms`](Self::put_atoms) since the index was last built or
    /// refreshed. Delta-merges new fragments into the existing index rather
    /// than rebuilding from scratch — cheap when only a small fraction of the
    /// dataset has changed. A no-op (no new dataset version) when there is
    /// nothing to merge.
    ///
    /// Lance serves queries correctly without this call — it scans the
    /// unindexed fragments instead — so this is purely a latency optimization.
    /// When to call it (after every write, nightly, by fragment count, …) is
    /// the calling application's concern, not the store's.
    pub async fn optimize_indexes(&mut self, options: &OptimizeOptions) -> Result<()> {
        let mut dataset = (*self.dataset).clone();
        dataset.optimize_indices(options).await?;
        self.dataset = Arc::new(dataset);
        Ok(())
    }

    /// Rewrites the dataset's fragments to coalesce many small writes into
    /// fewer, larger ones. Each [`put_atoms`](Self::put_atoms) call creates new
    /// fragments; over time, fragment accumulation slows every later scan.
    /// `compact` reads the small fragments and rewrites them into a smaller
    /// set of large ones.
    ///
    /// The store never deletes rows, so this is purely defragmentation — no
    /// tombstone reclamation is involved. Existing scalar indexes are
    /// remapped automatically as part of the compaction commit.
    ///
    /// A no-op (no new dataset version) when there is nothing to compact.
    /// When to call it is the calling application's concern, not the store's.
    pub async fn compact(&mut self, options: &CompactionOptions) -> Result<()> {
        let mut dataset = (*self.dataset).clone();
        compact_files(&mut dataset, options.clone(), None).await?;
        self.dataset = Arc::new(dataset);
        Ok(())
    }

    /// Deletes old dataset versions beyond the most recent `retain`, freeing
    /// the disk space they held. Lance keeps every committed version by
    /// default (each [`put_atoms`](Self::put_atoms) call creates one); without
    /// cleanup, the dataset grows without bound. After cleanup, time-travel
    /// to a dropped version is impossible — the latest `retain` versions are
    /// always kept.
    ///
    /// The store exposes the primitive; the retention policy (how many
    /// versions to keep, how often to clean) is the calling application's
    /// concern, not the store's.
    pub async fn cleanup_versions(&self, retain: usize) -> Result<RemovalStats> {
        let policy = CleanupPolicyBuilder::default()
            .retain_n_versions(&self.dataset, retain)
            .await?
            .build();
        let stats = self.dataset.cleanup_with_policy(policy).await?;
        Ok(stats)
    }

    /// Returns every atom stored under `id`. Because `atom_id` is the lossy matching
    /// key, this can be more than one atom — the markup variants that share an id —
    /// and the store returns them all, unranked; choosing among them is the caller's
    /// concern. An unknown id yields an empty vector, not an error.
    pub async fn get_atoms_by_id(&self, id: AtomId) -> Result<Vec<Atom>> {
        let hex: String = id.digest().map(|byte| format!("{byte:02x}")).concat();
        let predicate = format!("{ATOM_ID_COLUMN} = X'{hex}'");

        let mut scanner = self.dataset.scan();
        scanner.filter(&predicate)?;
        let batch = scanner.try_into_batch().await?;

        decode_atoms(&batch)
    }
}
