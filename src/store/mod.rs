//! Typed history storage and independent reference computations.
//!
//! Keep common experiment/commit/attempt metadata in relational records and
//! benchmark-specific configuration/output in versioned serialized payloads.
//! Authors provide typed data rather than SQL migrations per benchmark. Generic
//! metric indexing is deferred; views can derive values from typed history.
//!
//! Register the frozen benchmark collection once and assign storage-owned IDs.
//! Attempt start/finish and commit-completion operations still need adding to the
//! trait. Finalization should consistently persist its outcome and any artifact
//! references. Interrupted attempts must remain distinguishable from terminal
//! failures and must not advance completion.
//!
//! Queries select measurement attempts of one declared kind/version. The caller
//! chooses the matching output type; reject inconsistent payload metadata rather
//! than guessing its schema. Preserve exact-workload filtering and deterministic
//! revision order. Retry selection must choose attempts explicitly, never pool
//! retries as extra samples. The initial selection policy is still undecided.
//!
//! Inspection attempts link to their source record and are excluded from normal
//! history summaries. They may request logs/scripts/netlists, whose generation
//! changes timing. Verify original inputs, tools, and benchmark version before
//! claiming reproduction; discarded artifacts cannot be recovered from old runs.

mod reference;
mod sqlite;

pub use reference::{Reference, ReferenceCache, ReferenceId};
pub use sqlite::SqliteStore;

use std::time::SystemTime;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Result,
    history::{Commit, CommitId},
};

pub type ExperimentId = i64;
pub type RecordId = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkDefinition {
    pub kind: String,
    pub version: u32,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunPurpose {
    Measurement,
    Inspection { source: RecordId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Outcome<O> {
    Succeeded(O),
    Failed(String),
    Unsupported(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord<O> {
    pub experiment: ExperimentId,
    pub commit: Commit,
    pub benchmark: BenchmarkDefinition,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub purpose: RunPurpose,
    pub outcome: Outcome<O>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry<O> {
    pub id: RecordId,
    pub record: RunRecord<O>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History<O> {
    pub records: Vec<HistoryEntry<O>>,
}

/// Measurement attempts only. Domain filtering and summaries belong to views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub experiment: ExperimentId,
    pub benchmark_kind: String,
    pub benchmark_version: u32,
    pub benchmark_parameters: Option<serde_json::Value>,
}

pub trait Store: ReferenceCache {
    fn append<O: Serialize>(&mut self, record: &RunRecord<O>) -> Result<RecordId>;

    /// Validate the stored kind/version before deserializing the selected output.
    fn history<O: DeserializeOwned>(&self, query: &HistoryQuery) -> Result<History<O>>;

    /// Last commit in the processed prefix of this experiment's selected history.
    /// Terminal failures count as processed; unfinished earlier commits block
    /// advancement. This is not the maximum commit timestamp or newest result.
    fn last_complete(&self, experiment: ExperimentId) -> Result<Option<CommitId>>;
}
