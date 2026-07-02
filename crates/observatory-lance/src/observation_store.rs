//! The Lance-backed store for observations.
//!
//! [`LanceObservationStore`] is a thin, unopinionated wrapper over exactly
//! one Lance dataset (`observations.lance`). It implements the
//! [`ObservationStore`] trait from [`observatory-store`] for the domain-level
//! write/read surface, and additionally exposes Lance-specific lifecycle
//! (`open` / `create`) and maintenance (`ensure_indexes`, `optimize_indexes`,
//! `compact`, `cleanup_versions`) methods as inherent methods. Code that
//! needs those holds the concrete type; code that is backend-agnostic holds
//! the trait.
//!
//! The handle is an `Arc<Dataset>`: a Lance dataset is an immutable, versioned
//! snapshot, and a write yields a *new* version's handle rather than mutating
//! in place. Readers borrow `&self` and share the current snapshot; a writer
//! borrows `&mut self`, runs the write, and swaps the returned handle into
//! the field.

use std::sync::Arc;

use arrow::array::{RecordBatch, RecordBatchIterator};
use arrow::error::ArrowError;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use lance::Dataset;
use lance::dataset::{MergeInsertBuilder, WhenMatched, WhenNotMatched, WriteMode, WriteParams};
use lance::dataset::cleanup::{CleanupPolicyBuilder, RemovalStats};
use lance::dataset::optimize::{CompactionOptions, compact_files};
use lance::index::DatasetIndexExt;
use lance_index::optimize::OptimizeOptions;
use lance_index::{IndexType, scalar::ScalarIndexParams};

use observatory_core::identity::AtomId;
use observatory_observations::Observation;
use observatory_observations::identity::ObservationId;
use observatory_observations::Kind;
use observatory_store::ObservationStore;

use crate::decode::decode_observations;
use crate::encode::encode_observations;
use crate::error::{Result, map_lance};
use crate::schema::{KIND_COLUMN, OBSERVATION_ID_COLUMN, SUBJECTS_COLUMN, observations_schema};

/// Name of the BTREE index over [`OBSERVATION_ID_COLUMN`]. Stable across runs;
/// reused by `ensure_indexes` to make the operation idempotent.
const OBSERVATION_ID_INDEX_NAME: &str = "observation_id_idx";
/// Name of the BITMAP index over [`KIND_COLUMN`].
const KIND_INDEX_NAME: &str = "kind_idx";
/// Name of the LABEL_LIST index over [`SUBJECTS_COLUMN`].
const SUBJECTS_INDEX_NAME: &str = "subjects_idx";

/// Maximum number of observations buffered into one `RecordBatch` before it is
/// handed to Lance. A `put_observations` call drains its input stream in chunks
/// of this size: one logical commit at the end, with memory bounded by
/// `CHUNK_SIZE` rather than by the stream's total length. See the matching
/// constant on [`LanceAtomStore`](crate::LanceAtomStore) for the rationale.
const CHUNK_SIZE: usize = 1024;

/// A handle to the observations dataset at one version. Cloning the inner
/// `Arc` is cheap; the whole point is that many readers can share one open
/// dataset.
pub struct LanceObservationStore {
    dataset: Arc<Dataset>,
}

impl LanceObservationStore {
    /// Opens the existing observations dataset at `uri` (a Lance URI — a local
    /// path, `file://`, `s3://`, …). Errors if no dataset exists there; use
    /// [`create`](Self::create) to make a new one.
    pub async fn open(uri: &str) -> Result<Self> {
        let dataset = Dataset::open(uri).await.map_err(map_lance)?;
        Ok(Self {
            dataset: Arc::new(dataset),
        })
    }

    /// Creates a new, empty observations dataset at `uri`, establishing the
    /// schema. Errors if a dataset already exists there; use
    /// [`open`](Self::open) for that.
    pub async fn create(uri: &str) -> Result<Self> {
        let empty = std::iter::empty::<std::result::Result<RecordBatch, ArrowError>>();
        let batches = RecordBatchIterator::new(empty, observations_schema());
        let params = WriteParams {
            mode: WriteMode::Create,
            ..Default::default()
        };
        let dataset = Dataset::write(batches, uri, Some(params))
            .await
            .map_err(map_lance)?;
        Ok(Self {
            dataset: Arc::new(dataset),
        })
    }

    /// Builds the scalar indexes over `observation_id`, `kind`, and `subjects`
    /// if they are not already present. After this returns,
    /// [`get_observation_by_id`](ObservationStore::get_observation_by_id) and
    /// [`get_observations_of_kind`](ObservationStore::get_observations_of_kind)
    /// consult the relevant index instead of scanning every row; the
    /// `subjects` index backs the
    /// [`get_observations_by_subject`](ObservationStore::get_observations_by_subject)
    /// query. Idempotent: a no-op (no new dataset version) when every index
    /// already exists.
    ///
    /// Rows written by [`put_observations`](ObservationStore::put_observations)
    /// *after* this call are not covered by the indexes until
    /// `optimize_indices` is run — Lance serves queries correctly in the
    /// meantime, but scans the unindexed fragments rather than using the
    /// indexes. When the store should call `optimize_indices` is the calling
    /// application's concern, not the store's.
    pub async fn ensure_indexes(&mut self) -> Result<()> {
        let existing = self.dataset.load_indices().await.map_err(map_lance)?;
        let has_observation_id = existing
            .iter()
            .any(|idx| idx.name == OBSERVATION_ID_INDEX_NAME);
        let has_kind = existing.iter().any(|idx| idx.name == KIND_INDEX_NAME);
        let has_subjects = existing.iter().any(|idx| idx.name == SUBJECTS_INDEX_NAME);

        if has_observation_id && has_kind && has_subjects {
            return Ok(());
        }

        let mut dataset = (*self.dataset).clone();
        if !has_observation_id {
            dataset
                .create_index(
                    &[OBSERVATION_ID_COLUMN],
                    IndexType::BTree,
                    Some(OBSERVATION_ID_INDEX_NAME.to_string()),
                    &ScalarIndexParams::default(),
                    false,
                )
                .await
                .map_err(map_lance)?;
        }
        if !has_kind {
            dataset
                .create_index(
                    &[KIND_COLUMN],
                    IndexType::Bitmap,
                    Some(KIND_INDEX_NAME.to_string()),
                    &ScalarIndexParams::default(),
                    false,
                )
                .await
                .map_err(map_lance)?;
        }
        if !has_subjects {
            dataset
                .create_index(
                    &[SUBJECTS_COLUMN],
                    IndexType::LabelList,
                    Some(SUBJECTS_INDEX_NAME.to_string()),
                    &ScalarIndexParams::default(),
                    false,
                )
                .await
                .map_err(map_lance)?;
        }
        self.dataset = Arc::new(dataset);
        Ok(())
    }

    /// Refreshes the `observation_id`, `kind`, and `subjects` indexes to
    /// cover rows written by
    /// [`put_observations`](ObservationStore::put_observations) since the
    /// indexes were last built or refreshed. Delta-merges new fragments into
    /// the existing indexes rather than rebuilding from scratch — cheap when
    /// only a small fraction of the dataset has changed. A no-op (no new
    /// dataset version) when there is nothing to merge.
    ///
    /// Lance serves queries correctly without this call — it scans the
    /// unindexed fragments instead — so this is purely a latency optimization.
    /// When to call it (after every write, nightly, by fragment count, …) is
    /// the calling application's concern, not the store's.
    pub async fn optimize_indexes(&mut self, options: &OptimizeOptions) -> Result<()> {
        let mut dataset = (*self.dataset).clone();
        dataset
            .optimize_indices(options)
            .await
            .map_err(map_lance)?;
        self.dataset = Arc::new(dataset);
        Ok(())
    }

    /// Rewrites the dataset's fragments to coalesce many small writes into
    /// fewer, larger ones. Each
    /// [`put_observations`](ObservationStore::put_observations) call creates
    /// new fragments; over time, fragment accumulation slows every later
    /// scan. `compact` reads the small fragments and rewrites them into a
    /// smaller set of large ones.
    ///
    /// The store never deletes rows, so this is purely defragmentation — no
    /// tombstone reclamation is involved. Existing scalar indexes are
    /// remapped automatically as part of the compaction commit.
    ///
    /// A no-op (no new dataset version) when there is nothing to compact.
    /// When to call it is the calling application's concern, not the store's.
    pub async fn compact(&mut self, options: &CompactionOptions) -> Result<()> {
        let mut dataset = (*self.dataset).clone();
        compact_files(&mut dataset, options.clone(), None)
            .await
            .map_err(map_lance)?;
        self.dataset = Arc::new(dataset);
        Ok(())
    }

    /// Deletes old dataset versions beyond the most recent `retain`, freeing
    /// the disk space they held. Lance keeps every committed version by
    /// default (each
    /// [`put_observations`](ObservationStore::put_observations) call creates
    /// one); without cleanup, the dataset grows without bound. After cleanup,
    /// time-travel to a dropped version is impossible — the latest `retain`
    /// versions are always kept.
    ///
    /// The store exposes the primitive; the retention policy (how many
    /// versions to keep, how often to clean) is the calling application's
    /// concern, not the store's.
    pub async fn cleanup_versions(&self, retain: usize) -> Result<RemovalStats> {
        let policy = CleanupPolicyBuilder::default()
            .retain_n_versions(&self.dataset, retain)
            .await
            .map_err(map_lance)?
            .build();
        let stats = self
            .dataset
            .cleanup_with_policy(policy)
            .await
            .map_err(map_lance)?;
        Ok(stats)
    }
}

#[async_trait]
impl ObservationStore for LanceObservationStore {
    /// Stores `observations`, deriving each id itself via
    /// [`id_from_observation`](observatory_observations::id_from_observation)
    /// and upserting by content-addressed identity: an observation already
    /// present byte-for-byte is left untouched, so the call is idempotent,
    /// while a genuinely distinct observation is inserted. The stream is
    /// drained in chunks of [`CHUNK_SIZE`] items, each chunk encoded into one
    /// `RecordBatch`; one call hands all batches to a single Lance
    /// `merge_insert` — one logical commit. An empty stream is a no-op that
    /// writes no new version.
    async fn put_observations(
        &mut self,
        observations: impl Stream<Item = Observation> + Unpin + Send + 'static,
    ) -> Result<()> {
        let mut observations = observations;
        let mut batches: Vec<RecordBatch> = Vec::new();
        let mut buffer: Vec<Observation> = Vec::with_capacity(CHUNK_SIZE);

        while let Some(observation) = observations.next().await {
            buffer.push(observation);
            if buffer.len() >= CHUNK_SIZE {
                batches.push(encode_observations(&buffer));
                buffer.clear();
            }
        }
        if !buffer.is_empty() {
            batches.push(encode_observations(&buffer));
        }
        if batches.is_empty() {
            return Ok(());
        }

        let schema = batches[0].schema();
        let reader = RecordBatchIterator::new(batches.into_iter().map(Ok), schema);

        let mut builder = MergeInsertBuilder::try_new(
            Arc::clone(&self.dataset),
            vec![OBSERVATION_ID_COLUMN.to_string()],
        )
        .map_err(map_lance)?;
        builder
            .when_matched(WhenMatched::DoNothing)
            .when_not_matched(WhenNotMatched::InsertAll);
        let job = builder.try_build().map_err(map_lance)?;

        let (dataset, _stats) = job.execute_reader(reader).await.map_err(map_lance)?;
        self.dataset = dataset;
        Ok(())
    }

    /// Returns the observation whose content-addressed id is `id`, or `None`
    /// if no such observation is stored. The id is unique (it is the exact
    /// identity), so at most one match exists.
    async fn get_observation_by_id(
        &self,
        id: ObservationId,
    ) -> Result<Option<Observation>> {
        let hex: String = id.digest().map(|byte| format!("{byte:02x}")).concat();
        let predicate = format!("{OBSERVATION_ID_COLUMN} = X'{hex}'");

        let mut scanner = self.dataset.scan();
        scanner.filter(&predicate).map_err(map_lance)?;
        let batch = scanner.try_into_batch().await.map_err(map_lance)?;

        let mut observations = decode_observations(&batch)?;
        Ok(observations.pop())
    }

    /// Returns every observation whose `kind` matches. The result is
    /// unranked; choosing among them is the caller's concern. An unknown kind
    /// yields an empty vector, not an error.
    async fn get_observations_of_kind(&self, kind: &Kind) -> Result<Vec<Observation>> {
        let escaped = kind.as_str().replace('\'', "''");
        let predicate = format!("{KIND_COLUMN} = '{escaped}'");

        let mut scanner = self.dataset.scan();
        scanner.filter(&predicate).map_err(map_lance)?;
        let batch = scanner.try_into_batch().await.map_err(map_lance)?;

        decode_observations(&batch)
    }

    /// Returns every observation whose `subjects` list contains `atom`. The
    /// result is unranked; choosing among them is the caller's concern. An
    /// atom observed by nothing yields an empty vector, not an error.
    ///
    /// Uses the `LABEL_LIST` index on `subjects` when it has been built via
    /// [`ensure_indexes`](LanceObservationStore::ensure_indexes); without the
    /// index the query scans every row, so callers should ensure indexes
    /// before relying on this for latency-sensitive paths.
    async fn get_observations_by_subject(&self, atom: AtomId) -> Result<Vec<Observation>> {
        let hex: String = atom.digest().map(|byte| format!("{byte:02x}")).concat();
        let predicate = format!(
            "array_has({SUBJECTS_COLUMN}, arrow_cast(X'{hex}', 'FixedSizeBinary(32)'))"
        );

        let mut scanner = self.dataset.scan();
        scanner.filter(&predicate).map_err(map_lance)?;
        let batch = scanner.try_into_batch().await.map_err(map_lance)?;

        decode_observations(&batch)
    }
}
