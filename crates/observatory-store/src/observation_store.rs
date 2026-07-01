//! The Lance-backed store for observations.
//!
//! `ObservationStore` is a thin, unopinionated wrapper over exactly one Lance
//! dataset (`observations.lance`). It faithfully stores and retrieves observations
//! and holds no policy: it does not decide which `Kind`s are valid, interpret
//! subject order, or implement a query language — those are the calling
//! application's concerns. It offers primitives, not workflows.
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

use observatory_observations::Observation;
use observatory_observations::identity::ObservationId;
use observatory_observations::Kind;

use crate::decode::decode_observations;
use crate::encode::encode_observations;
use crate::error::Result;
use crate::schema::{KIND_COLUMN, OBSERVATION_ID_COLUMN, observations_schema};

/// A handle to the observations dataset at one version. Cloning the inner `Arc`
/// is cheap; the whole point is that many readers can share one open dataset.
pub struct ObservationStore {
    dataset: Arc<Dataset>,
}

impl ObservationStore {
    /// Opens the existing observations dataset at `uri` (a Lance URI — a local
    /// path, `file://`, `s3://`, …). Errors if no dataset exists there; use
    /// [`create`](Self::create) to make a new one.
    pub async fn open(uri: &str) -> Result<Self> {
        let dataset = Dataset::open(uri).await?;
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
        let dataset = Dataset::write(batches, uri, Some(params)).await?;
        Ok(Self {
            dataset: Arc::new(dataset),
        })
    }

    /// Stores `observations`, deriving each id itself via
    /// [`id_from_observation`](observatory_observations::id_from_observation) and
    /// upserting by content-addressed identity: an observation already present
    /// byte-for-byte is left untouched, so the call is idempotent, while a
    /// genuinely distinct observation is inserted. One call is one Lance commit.
    /// Passing no observations is a no-op that writes no new version.
    pub async fn put_observations(&mut self, observations: &[Observation]) -> Result<()> {
        if observations.is_empty() {
            return Ok(());
        }

        let batch = encode_observations(observations);
        let schema = batch.schema();
        let reader = RecordBatchIterator::new([Ok(batch)], schema);

        let mut builder = MergeInsertBuilder::try_new(
            Arc::clone(&self.dataset),
            vec![OBSERVATION_ID_COLUMN.to_string()],
        )?;
        builder
            .when_matched(WhenMatched::DoNothing)
            .when_not_matched(WhenNotMatched::InsertAll);
        let job = builder.try_build()?;

        let (dataset, _stats) = job.execute_reader(reader).await?;
        self.dataset = dataset;
        Ok(())
    }

    /// Returns the observation whose content-addressed id is `id`, or `None` if
    /// no such observation is stored. The id is unique (it is the exact
    /// identity), so at most one match exists.
    pub async fn get_observation_by_id(&self, id: ObservationId) -> Result<Option<Observation>> {
        let hex: String = id.digest().map(|byte| format!("{byte:02x}")).concat();
        let predicate = format!("{OBSERVATION_ID_COLUMN} = X'{hex}'");

        let mut scanner = self.dataset.scan();
        scanner.filter(&predicate)?;
        let batch = scanner.try_into_batch().await?;

        let mut observations = decode_observations(&batch)?;
        Ok(observations.pop())
    }

    /// Returns every observation whose `kind` matches. The result is unranked;
    /// choosing among them is the caller's concern. An unknown kind yields an
    /// empty vector, not an error.
    pub async fn get_observations_of_kind(&self, kind: &Kind) -> Result<Vec<Observation>> {
        let escaped = kind.as_str().replace('\'', "''");
        let predicate = format!("{KIND_COLUMN} = '{escaped}'");

        let mut scanner = self.dataset.scan();
        scanner.filter(&predicate)?;
        let batch = scanner.try_into_batch().await?;

        decode_observations(&batch)
    }
}
