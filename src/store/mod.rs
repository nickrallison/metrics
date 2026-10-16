//! Typed history storage and independent reference computations.
//!
//! Keep common experiment/commit/attempt metadata in relational records and
//! benchmark-specific configuration/output in versioned serialized payloads.
//! Authors provide typed data rather than SQL migrations per benchmark. Generic
//! metric indexing is deferred; views can derive values from typed history.
//!
//! Register the frozen benchmark collection once and assign storage-owned IDs.
//! Attempt start/finish and commit-completion operations must be durable.
//! Finalization should consistently persist its outcome and any artifact
//! references. Interrupted attempts must remain distinguishable from terminal
//! failures and must not advance completion.
//!
//! Queries select measurement attempts of one declared kind/version. The caller
//! chooses the matching output type; reject inconsistent payload metadata rather
//! than guessing its schema. Preserve exact-workload filtering and deterministic
//! revision order. Retry selection must choose attempts explicitly, never pool
//! retries as extra samples. SQLite selects the latest finished measurement per
//! frozen workload and commit, excluding inspection and unfinished attempts.
//!
//! Inspection attempts link to their source record and are excluded from normal
//! history summaries. They may request logs/scripts/netlists, whose generation
//! changes timing. Verify original inputs, tools, and benchmark version before
//! claiming reproduction; discarded artifacts cannot be recovered from old runs.

mod reference;
mod sqlite;

pub use reference::{Reference, ReferenceCache, ReferenceId};
pub use sqlite::SqliteStore;

use std::{collections::BTreeMap, path::PathBuf, time::SystemTime};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Result,
    history::{Commit, CommitId},
};

pub type ExperimentId = i64;
pub type RecordId = i64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkDefinition {
    pub kind: String,
    pub version: u32,
    pub parameters: serde_json::Value,
}

/// Frozen configuration, workload order, and resolved revision order for a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentDefinition {
    pub configuration: serde_json::Value,
    pub benchmarks: Vec<BenchmarkDefinition>,
    pub commits: Vec<Commit>,
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

/// Persist before execution. An unfinished entry is in progress or interrupted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStart {
    pub experiment: ExperimentId,
    pub commit: Commit,
    /// Position in the frozen collection, including duplicate definitions.
    pub benchmark_index: usize,
    pub benchmark: BenchmarkDefinition,
    pub started_at: SystemTime,
    pub purpose: RunPurpose,
    pub artifacts: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord<O> {
    pub experiment: ExperimentId,
    pub commit: Commit,
    pub benchmark_index: usize,
    pub benchmark: BenchmarkDefinition,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub purpose: RunPurpose,
    pub artifacts: Option<PathBuf>,
    pub outcome: Outcome<O>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry<O> {
    pub id: RecordId,
    pub record: RunRecord<O>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History<O> {
    /// Frozen revision order, including commits without finished measurements.
    pub commits: Vec<Commit>,
    /// Matching frozen workloads, keyed by their original collection positions.
    pub benchmarks: BTreeMap<usize, BenchmarkDefinition>,
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
    /// Create an experiment and persist its frozen definitions and revision order.
    /// This does not implicitly resume or merge a previous experiment.
    fn register_experiment(&mut self, definition: &ExperimentDefinition) -> Result<ExperimentId>;

    fn start_run(&mut self, start: &RunStart) -> Result<RecordId>;

    /// Finalize the same attempt; failure must leave it visibly unfinished.
    fn finish_run<O: Serialize>(
        &mut self,
        record: RecordId,
        finished_at: SystemTime,
        outcome: &Outcome<O>,
    ) -> Result<()>;

    fn append<O: Serialize>(&mut self, record: &RunRecord<O>) -> Result<RecordId> {
        let id = self.start_run(&RunStart {
            experiment: record.experiment,
            commit: record.commit.clone(),
            benchmark_index: record.benchmark_index,
            benchmark: record.benchmark.clone(),
            started_at: record.started_at,
            purpose: record.purpose.clone(),
            artifacts: record.artifacts.clone(),
        })?;
        self.finish_run(id, record.finished_at, &record.outcome)?;
        Ok(id)
    }

    fn record_build_failure(
        &mut self,
        experiment: ExperimentId,
        commit: &Commit,
        reason: &str,
    ) -> Result<()>;

    /// Verify terminal outcomes for every frozen workload before advancing the
    /// processed prefix. Failures/unsupported count; unfinished attempts do not.
    fn complete_commit(&mut self, experiment: ExperimentId, commit: &CommitId) -> Result<()>;

    /// Validate the stored kind/version before deserializing the selected output.
    fn history<O: DeserializeOwned>(&self, query: &HistoryQuery) -> Result<History<O>>;

    /// Last commit in the processed prefix of this experiment's selected history.
    /// Terminal failures count as processed; unfinished earlier commits block
    /// advancement. This is not the maximum commit timestamp or newest result.
    fn last_complete(&self, experiment: ExperimentId) -> Result<Option<CommitId>>;
}
