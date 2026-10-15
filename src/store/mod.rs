//! Typed history storage and independent reference computations.

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

    fn last_complete(&self, experiment: ExperimentId) -> Result<Option<CommitId>>;
}
