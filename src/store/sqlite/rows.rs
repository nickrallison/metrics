use std::collections::HashMap;

use diesel::prelude::*;
use serde::de::DeserializeOwned;

use super::schema::{benchmarks, runs};
use crate::{
    Error, Result,
    history::Commit,
    store::{
        BenchmarkDefinition, ExperimentDefinition, ExperimentId, HistoryEntry, RunPurpose,
        RunRecord, RunStart,
    },
};

#[derive(Queryable, Selectable)]
#[diesel(table_name = benchmarks, check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct BenchmarkRow {
    kind: String,
    version: i64,
    parameters: String,
}

impl BenchmarkRow {
    pub fn definition(self) -> Result<BenchmarkDefinition> {
        Ok(BenchmarkDefinition {
            kind: self.kind,
            version: u32::try_from(self.version)
                .map_err(|_| Error::Storage("invalid stored benchmark version".into()))?,
            parameters: serde_json::from_str(&self.parameters)?,
        })
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = runs, check_for_backend(diesel::sqlite::Sqlite))]
pub(super) struct RunRow {
    pub id: i64,
    pub commit_id: String,
    pub benchmark_index: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub inspection_source: Option<i64>,
    pub artifacts: Option<String>,
    pub outcome: Option<String>,
}

/// Decode rows using the same registered definitions within one read snapshot.
pub(super) struct Definitions<'a> {
    commits: HashMap<&'a str, &'a Commit>,
    benchmarks: &'a [BenchmarkDefinition],
}

impl<'a> Definitions<'a> {
    pub fn new(definition: &'a ExperimentDefinition) -> Self {
        Self {
            commits: definition
                .commits
                .iter()
                .map(|commit| (commit.id.0.as_str(), commit))
                .collect(),
            benchmarks: &definition.benchmarks,
        }
    }
}

impl RunRow {
    pub fn start(
        &self,
        experiment: ExperimentId,
        definitions: &Definitions<'_>,
    ) -> Result<RunStart> {
        let benchmark_index = usize::try_from(self.benchmark_index)
            .map_err(|_| Error::Storage("invalid stored workload position".into()))?;
        let benchmark = definitions
            .benchmarks
            .get(benchmark_index)
            .ok_or_else(|| Error::Storage("unregistered stored workload".into()))?;
        let commit = definitions
            .commits
            .get(self.commit_id.as_str())
            .ok_or_else(|| Error::Storage("unregistered stored commit".into()))?;
        Ok(RunStart {
            experiment,
            commit: (*commit).clone(),
            benchmark_index,
            benchmark: benchmark.clone(),
            started_at: serde_json::from_str(&self.started_at)?,
            purpose: self
                .inspection_source
                .map_or(RunPurpose::Measurement, |source| RunPurpose::Inspection {
                    source,
                }),
            artifacts: self
                .artifacts
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
        })
    }

    pub fn finished<O: DeserializeOwned>(
        &self,
        experiment: ExperimentId,
        definitions: &Definitions<'_>,
    ) -> Result<HistoryEntry<O>> {
        let start = self.start(experiment, definitions)?;
        Ok(HistoryEntry {
            id: self.id,
            record: RunRecord {
                experiment,
                commit: start.commit,
                benchmark_index: start.benchmark_index,
                benchmark: start.benchmark,
                started_at: start.started_at,
                finished_at: serde_json::from_str(
                    self.finished_at
                        .as_deref()
                        .ok_or_else(|| Error::Storage("missing finish time".into()))?,
                )?,
                purpose: start.purpose,
                artifacts: start.artifacts,
                outcome: serde_json::from_str(
                    self.outcome
                        .as_deref()
                        .ok_or_else(|| Error::Storage("missing outcome".into()))?,
                )?,
            },
        })
    }
}
