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

use observatory_core::ir::Atom;

use crate::encode::encode_atoms;
use crate::error::Result;
use crate::schema::{ROW_DIGEST_COLUMN, atoms_schema};

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
}
