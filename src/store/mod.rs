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

mod catalog;
mod reference;
mod sqlite;

pub use catalog::{CatalogExperiment, ExperimentCatalog};
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

/// Original configuration and frozen workloads. New commits append to the
/// registered revision order when reusing an experiment.
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

/// Select a typed slice of one experiment for analysis.
///
/// `measurement` identifies the output schema. Optional commit and benchmark
/// selections further restrict the frozen dimensions while preserving their
/// recorded order. Retry selection remains explicit and is not pooled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub measurement: HistoryQuery,
    pub commits: Option<Vec<CommitId>>,
    pub benchmarks: Option<Vec<usize>>,
    pub runs: RunSelection,
}

impl Query {
    pub fn new(measurement: HistoryQuery) -> Self {
        Self {
            measurement,
            commits: None,
            benchmarks: None,
            runs: RunSelection::LatestFinished,
        }
    }

    pub fn commits(mut self, commits: Vec<CommitId>) -> Self {
        self.commits = Some(commits);
        self
    }

    pub fn benchmarks(mut self, benchmarks: Vec<usize>) -> Self {
        self.benchmarks = Some(benchmarks);
        self
    }

    pub fn runs(mut self, runs: RunSelection) -> Self {
        self.runs = runs;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunSelection {
    LatestFinished,
    AllFinished,
}

/// The selected experiment data, retaining its commit, workload, and run axes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame<O> {
    pub experiment: ExperimentId,
    pub commits: Vec<Commit>,
    pub benchmarks: BTreeMap<usize, BenchmarkDefinition>,
    pub runs: Vec<HistoryEntry<O>>,
}

impl<O> Frame<O> {
    fn from_history(experiment: ExperimentId, history: History<O>) -> Self {
        Self {
            experiment,
            commits: history.commits,
            benchmarks: history.benchmarks,
            runs: history.records,
        }
    }
}

impl<O> Frame<O> {
    /// Keep runs accepted by a caller-defined predicate.
    pub fn filter<F>(mut self, mut predicate: F) -> Self
    where
        F: FnMut(&HistoryEntry<O>) -> bool,
    {
        self.runs.retain(|run| predicate(run));
        self
    }

    /// Map each run to any caller-defined value or row type.
    pub fn map_runs<R, F>(&self, mut map: F) -> Vec<R>
    where
        F: FnMut(&HistoryEntry<O>) -> R,
    {
        self.runs.iter().map(&mut map).collect()
    }

    /// Group caller-defined keys and reduce caller-defined values per group.
    ///
    /// The store does not prescribe which dimension forms a group or how its
    /// values are combined. A reducer can compute a mean, a custom statistic,
    /// or a richer result carrying its own coverage information.
    ///
    /// e.g. Can be used for finding mean, median, min & max
    pub fn group_reduce<K, V, R, Group, Value, Reduce>(
        &self,
        mut group: Group,
        mut value: Value,
        mut reduce: Reduce,
    ) -> BTreeMap<K, R>
    where
        K: Ord,
        Group: FnMut(&HistoryEntry<O>) -> K,
        Value: FnMut(&HistoryEntry<O>) -> V,
        Reduce: FnMut(Vec<V>) -> R,
    {
        let mut groups = BTreeMap::<K, Vec<V>>::new();
        for run in &self.runs {
            groups.entry(group(run)).or_default().push(value(run));
        }
        groups
            .into_iter()
            .map(|(key, values)| (key, reduce(values)))
            .collect()
    }
}

/// A caller-defined analysis over a loaded frame.
///
/// Closures keep filtering, projection, aggregation, and derived metrics open
/// to benchmark-specific types. Analyses are executable code, so they are not
/// serialized with experiment data.
pub struct Analysis<F> {
    operation: F,
}

impl<F> Analysis<F> {
    pub fn new(operation: F) -> Self {
        Self { operation }
    }

    pub fn run<O, R>(self, frame: Frame<O>) -> R
    where
        F: FnOnce(Frame<O>) -> R,
    {
        (self.operation)(frame)
    }
}

pub trait Store: ReferenceCache {
    /// Persistent database location, when this store has one.
    fn database_path(&self) -> Option<&std::path::Path> {
        None
    }

    /// Create an experiment and persist its frozen definitions and revision order.
    /// This does not implicitly resume or merge a previous experiment.
    fn register_experiment(&mut self, definition: &ExperimentDefinition) -> Result<ExperimentId>;

    /// Reuse a compatible experiment, appending newly selected commits. An
    /// explicit ID must match; stores without reuse support can register anew.
    fn prepare_experiment(
        &mut self,
        definition: &ExperimentDefinition,
        existing: Option<ExperimentId>,
    ) -> Result<ExperimentId> {
        if existing.is_some() {
            return Err(crate::Error::Unsupported(
                "experiment reuse in this store".into(),
            ));
        }
        self.register_experiment(definition)
    }

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

    /// Verify terminal outcomes for every frozen workload and mark this commit
    /// complete. `last_complete` derives the processed prefix independently.
    fn complete_commit(&mut self, experiment: ExperimentId, commit: &CommitId) -> Result<()>;

    /// Mark a commit complete if all workloads have finished latest attempts.
    /// Partial workload reruns can leave a commit incomplete without failing.
    fn try_complete_commit(&mut self, experiment: ExperimentId, commit: &CommitId) -> Result<bool> {
        self.complete_commit(experiment, commit)?;
        Ok(true)
    }

    /// Validate the stored kind/version before deserializing the selected output.
    fn history<O: DeserializeOwned>(&self, query: &HistoryQuery) -> Result<History<O>>;

    /// Load every finished measurement attempt for a query. Stores may opt in
    /// independently; the default keeps the initial query API usable meanwhile.
    fn all_finished<O: DeserializeOwned>(&self, _query: &HistoryQuery) -> Result<History<O>> {
        Err(crate::Error::Unsupported(
            "loading all finished measurement attempts in this store".into(),
        ))
    }

    /// Load a dimensioned frame using the store's normal latest-finished policy.
    /// Run filtering is applied in Rust so this remains available to every store.
    fn query<O: DeserializeOwned>(&self, query: &Query) -> Result<Frame<O>> {
        let history = match query.runs {
            RunSelection::LatestFinished => self.history(&query.measurement)?,
            RunSelection::AllFinished => self.all_finished(&query.measurement)?,
        };
        let mut frame = Frame::from_history(query.measurement.experiment, history);

        if let Some(selected) = &query.commits {
            if selected
                .iter()
                .enumerate()
                .any(|(index, id)| selected[..index].contains(id))
            {
                return Err(crate::Error::Storage(
                    "query contains duplicate commits".into(),
                ));
            }
            if selected
                .iter()
                .any(|id| !frame.commits.iter().any(|commit| &commit.id == id))
            {
                return Err(crate::Error::Storage(
                    "query contains a commit outside the selected experiment history".into(),
                ));
            }
            frame.commits.retain(|commit| selected.contains(&commit.id));
        }

        if let Some(selected) = &query.benchmarks {
            let selected: std::collections::HashSet<_> = selected.iter().copied().collect();
            if selected.len() != query.benchmarks.as_ref().map_or(0, Vec::len) {
                return Err(crate::Error::Storage(
                    "query contains duplicate benchmark indices".into(),
                ));
            }
            if selected
                .iter()
                .any(|index| !frame.benchmarks.contains_key(index))
            {
                return Err(crate::Error::Storage(
                    "query contains a benchmark outside the selected experiment history".into(),
                ));
            }
            frame.benchmarks.retain(|index, _| selected.contains(index));
        }

        frame.runs.retain(|entry| {
            query
                .commits
                .as_ref()
                .is_none_or(|commits| commits.contains(&entry.record.commit.id))
                && query
                    .benchmarks
                    .as_ref()
                    .is_none_or(|benchmarks| benchmarks.contains(&entry.record.benchmark_index))
        });
        Ok(frame)
    }

    /// Last commit in the processed prefix of this experiment's selected history.
    /// Terminal failures count as processed; unfinished earlier commits block
    /// advancement. This is not the maximum commit timestamp or newest result.
    fn last_complete(&self, experiment: ExperimentId) -> Result<Option<CommitId>>;
}
