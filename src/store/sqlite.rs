use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};

use super::{
    BenchmarkDefinition, ExperimentDefinition, ExperimentId, History, HistoryEntry, HistoryQuery,
    Outcome, RecordId, Reference, ReferenceCache, ReferenceId, RunPurpose, RunRecord, RunStart,
    Store,
};
use crate::{
    Error, Result,
    history::{Commit, CommitId},
};

/// Persistent typed history. Opening creates the parent directory and schema.
#[derive(Debug)]
pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    /// Most recently registered experiment, or `None` for an empty database.
    pub fn latest_experiment(&self) -> Result<Option<ExperimentId>> {
        Ok(self
            .connection
            .query_row("SELECT MAX(id) FROM experiments", [], |row| row.get(0))?)
    }

    /// `:memory:` opens a temporary database. Unsupported schema versions fail.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: u32 =
            transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => transaction.execute_batch(include_str!("sqlite/schema.sql"))?,
            1 => {}
            version => {
                return Err(Error::Storage(format!(
                    "unsupported schema version {version}"
                )));
            }
        }
        transaction.commit()?;
        Ok(Self { connection })
    }

    /// Load the frozen definitions in their original workload/revision order.
    pub fn experiment(&self, id: ExperimentId) -> Result<ExperimentDefinition> {
        ensure_experiment(&self.connection, id)?;
        let configuration: String = self.connection.query_row(
            "SELECT configuration FROM experiments WHERE id = ?1",
            [id],
            |row| row.get(0),
        )?;
        let mut statement = self.connection.prepare(
            "SELECT kind, version, parameters FROM benchmarks WHERE experiment_id = ?1 ORDER BY position",
        )?;
        let rows = statement.query_map([id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut benchmarks = Vec::new();
        for row in rows {
            let (kind, version, parameters) = row?;
            benchmarks.push(BenchmarkDefinition {
                kind,
                version,
                parameters: serde_json::from_str(&parameters)?,
            });
        }
        let mut statement = self.connection.prepare(
            "SELECT c.metadata FROM experiment_commits ec JOIN commits c ON c.id = ec.commit_id
             WHERE ec.experiment_id = ?1 ORDER BY ec.position",
        )?;
        let rows = statement.query_map([id], |row| row.get::<_, String>(0))?;
        let mut commits = Vec::new();
        for row in rows {
            commits.push(serde_json::from_str(&row?)?);
        }
        Ok(ExperimentDefinition {
            configuration: serde_json::from_str(&configuration)?,
            benchmarks,
            commits,
        })
    }

    /// In-progress or interrupted attempts, including inspections. They are not
    /// returned by `history`; a later finished retry can supersede them.
    pub fn unfinished_runs(&self, experiment: ExperimentId) -> Result<Vec<(RecordId, RunStart)>> {
        ensure_experiment(&self.connection, experiment)?;
        let mut statement = self.connection.prepare(&format!(
            "{RUN_SELECT} WHERE r.experiment_id = ?1 AND r.finished_at IS NULL
             ORDER BY ec.position, r.benchmark_index, r.id",
        ))?;
        let rows = statement.query_map([experiment], RawRun::from_row)?;
        let mut runs = Vec::new();
        for row in rows {
            let row = row?;
            runs.push((row.id, row.start(experiment)?));
        }
        Ok(runs)
    }
}

impl ReferenceCache for SqliteStore {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Reference<T>>> {
        let row = self.connection.query_row(
            "SELECT id, measured_at, payload FROM reference_results WHERE cache_key = ?1 ORDER BY id DESC LIMIT 1",
            [key], |row| Ok((row.get(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        ).optional()?;
        row.map(|(id, measured_at, payload)| {
            Ok(Reference {
                id,
                measured_at: serde_json::from_str(&measured_at)?,
                value: serde_json::from_str(&payload)?,
            })
        })
        .transpose()
    }

    fn insert<T: Serialize>(&mut self, key: &str, value: &T) -> Result<ReferenceId> {
        self.connection.execute(
            "INSERT INTO reference_results(cache_key, measured_at, payload) VALUES (?1, ?2, ?3)",
            params![
                key,
                serde_json::to_string(&SystemTime::now())?,
                serde_json::to_string(value)?
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }
}

impl Store for SqliteStore {
    fn register_experiment(&mut self, definition: &ExperimentDefinition) -> Result<ExperimentId> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO experiments(configuration) VALUES (?1)",
            [serde_json::to_string(&definition.configuration)?],
        )?;
        let id = transaction.last_insert_rowid();
        for (position, benchmark) in definition.benchmarks.iter().enumerate() {
            let position = sql_index(position)?;
            transaction.execute(
                "INSERT INTO benchmarks(experiment_id, position, kind, version, parameters) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, position, benchmark.kind, benchmark.version, serde_json::to_string(&benchmark.parameters)?],
            )?;
        }
        for (position, commit) in definition.commits.iter().enumerate() {
            let position = sql_index(position)?;
            transaction.execute(
                "INSERT INTO commits(id, metadata) VALUES (?1, ?2) ON CONFLICT(id) DO NOTHING",
                params![commit.id.0, serde_json::to_string(commit)?],
            )?;
            let metadata: String = transaction.query_row(
                "SELECT metadata FROM commits WHERE id = ?1",
                [&commit.id.0],
                |row| row.get(0),
            )?;
            if serde_json::from_str::<Commit>(&metadata)? != *commit {
                return Err(Error::Storage(format!(
                    "conflicting metadata for commit {}",
                    commit.id.0
                )));
            }
            transaction.execute(
                "INSERT INTO experiment_commits(experiment_id, commit_id, position) VALUES (?1, ?2, ?3)",
                params![id, commit.id.0, position],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    fn start_run(&mut self, start: &RunStart) -> Result<RecordId> {
        let benchmark_index = sql_index(start.benchmark_index)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (_, commit) = registered_commit(&transaction, start.experiment, &start.commit.id)?;
        if commit != start.commit {
            return Err(Error::Storage(
                "attempt commit metadata differs from the frozen definition".into(),
            ));
        }
        let expected = transaction.query_row(
            "SELECT kind, version, parameters FROM benchmarks WHERE experiment_id = ?1 AND position = ?2",
            params![start.experiment, benchmark_index],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?, row.get::<_, String>(2)?)),
        ).optional()?.ok_or_else(|| Error::Storage("workload is not registered in this experiment".into()))?;
        if start.benchmark.kind != expected.0
            || start.benchmark.version != expected.1
            || start.benchmark.parameters != serde_json::from_str::<serde_json::Value>(&expected.2)?
        {
            return Err(Error::Storage(
                "attempt benchmark differs from the frozen definition".into(),
            ));
        }
        let source = match start.purpose {
            RunPurpose::Measurement => None,
            RunPurpose::Inspection { source } => {
                let valid: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM runs WHERE id = ?1 AND experiment_id = ?2 AND commit_id = ?3
                     AND benchmark_index = ?4 AND inspection_source IS NULL AND finished_at IS NOT NULL)",
                    params![source, start.experiment, start.commit.id.0, benchmark_index], |row| row.get(0),
                )?;
                if !valid {
                    return Err(Error::Storage("inspection source must be a finished measurement of the same workload and commit".into()));
                }
                Some(source)
            }
        };
        transaction.execute(
            "INSERT INTO runs(experiment_id, commit_id, benchmark_index, started_at, inspection_source, artifacts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![start.experiment, start.commit.id.0, benchmark_index,
                serde_json::to_string(&start.started_at)?, source, start.artifacts.as_ref().map(serde_json::to_string).transpose()?],
        )?;
        let id = transaction.last_insert_rowid();
        if source.is_none() {
            transaction.execute("UPDATE experiment_commits SET completed = 0 WHERE experiment_id = ?1 AND commit_id = ?2",
                params![start.experiment, start.commit.id.0])?;
        }
        transaction.commit()?;
        Ok(id)
    }

    fn finish_run<O: Serialize>(
        &mut self,
        record: RecordId,
        finished_at: SystemTime,
        outcome: &Outcome<O>,
    ) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE runs SET finished_at = ?1, outcome = ?2 WHERE id = ?3 AND finished_at IS NULL",
            params![
                serde_json::to_string(&finished_at)?,
                serde_json::to_string(outcome)?,
                record
            ],
        )?;
        if changed != 1 {
            return Err(Error::Storage(format!(
                "attempt {record} does not exist or is already finished"
            )));
        }
        Ok(())
    }

    fn record_build_failure(
        &mut self,
        experiment: ExperimentId,
        commit: &Commit,
        reason: &str,
    ) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (_, expected) = registered_commit(&transaction, experiment, &commit.id)?;
        if expected != *commit {
            return Err(Error::Storage(
                "build commit differs from the frozen definition".into(),
            ));
        }
        transaction.execute(
            "INSERT INTO build_failures(experiment_id, commit_id, recorded_at, reason) VALUES (?1, ?2, ?3, ?4)",
            params![experiment, commit.id.0, serde_json::to_string(&SystemTime::now())?, reason],
        )?;
        transaction.execute("UPDATE experiment_commits SET completed = 0 WHERE experiment_id = ?1 AND commit_id = ?2",
            params![experiment, commit.id.0])?;
        transaction.commit()?;
        Ok(())
    }

    fn complete_commit(&mut self, experiment: ExperimentId, commit: &CommitId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (position, _) = registered_commit(&transaction, experiment, commit)?;
        let earlier_incomplete: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM experiment_commits WHERE experiment_id = ?1 AND position < ?2 AND completed = 0)",
            params![experiment, position], |row| row.get(0),
        )?;
        if earlier_incomplete {
            return Err(Error::Storage(
                "an earlier selected commit is incomplete".into(),
            ));
        }
        let incomplete: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM benchmarks b LEFT JOIN runs r ON r.id = (
                SELECT MAX(id) FROM runs WHERE experiment_id = b.experiment_id AND commit_id = ?2
                AND benchmark_index = b.position AND inspection_source IS NULL)
             WHERE b.experiment_id = ?1 AND r.finished_at IS NULL)",
            params![experiment, commit.0],
            |row| row.get(0),
        )?;
        if incomplete {
            return Err(Error::Storage(
                "the latest measurement of every workload must be finished".into(),
            ));
        }
        transaction.execute("UPDATE experiment_commits SET completed = 1 WHERE experiment_id = ?1 AND commit_id = ?2",
            params![experiment, commit.0])?;
        transaction.commit()?;
        Ok(())
    }

    fn history<O: DeserializeOwned>(&self, query: &HistoryQuery) -> Result<History<O>> {
        let definition = self.experiment(query.experiment)?;
        let valid_schema = definition.benchmarks.iter().any(|benchmark| {
            benchmark.kind == query.benchmark_kind && benchmark.version == query.benchmark_version
        });
        if !valid_schema {
            return Err(Error::Storage(
                "benchmark kind/version is not registered in this experiment".into(),
            ));
        }
        let mut statement = self.connection.prepare(&format!(
            "{RUN_SELECT} WHERE r.experiment_id = ?1 AND b.kind = ?2 AND b.version = ?3
             AND r.inspection_source IS NULL AND r.finished_at IS NOT NULL
             AND r.id = (SELECT MAX(id) FROM runs WHERE experiment_id = r.experiment_id
                AND commit_id = r.commit_id AND benchmark_index = r.benchmark_index
                AND inspection_source IS NULL AND finished_at IS NOT NULL)
             ORDER BY ec.position, r.benchmark_index",
        ))?;
        let rows = statement.query_map(
            params![
                query.experiment,
                query.benchmark_kind,
                query.benchmark_version
            ],
            RawRun::from_row,
        )?;
        let mut records = Vec::new();
        for row in rows {
            let row = row?;
            let start = row.start(query.experiment)?;
            if query
                .benchmark_parameters
                .as_ref()
                .is_some_and(|filter| *filter != start.benchmark.parameters)
            {
                continue;
            }
            let record = RunRecord {
                experiment: start.experiment,
                commit: start.commit,
                benchmark_index: start.benchmark_index,
                benchmark: start.benchmark,
                started_at: start.started_at,
                finished_at: serde_json::from_str(
                    row.finished_at
                        .as_deref()
                        .ok_or_else(|| Error::Storage("missing finish time".into()))?,
                )?,
                purpose: start.purpose,
                artifacts: start.artifacts,
                outcome: serde_json::from_str(
                    row.outcome
                        .as_deref()
                        .ok_or_else(|| Error::Storage("missing outcome".into()))?,
                )?,
            };
            records.push(HistoryEntry { id: row.id, record });
        }
        let benchmarks = definition
            .benchmarks
            .into_iter()
            .enumerate()
            .filter(|(_, benchmark)| {
                benchmark.kind == query.benchmark_kind
                    && benchmark.version == query.benchmark_version
                    && query
                        .benchmark_parameters
                        .as_ref()
                        .is_none_or(|filter| *filter == benchmark.parameters)
            })
            .collect();
        Ok(History {
            commits: definition.commits,
            benchmarks,
            records,
        })
    }

    fn last_complete(&self, experiment: ExperimentId) -> Result<Option<CommitId>> {
        ensure_experiment(&self.connection, experiment)?;
        let id = self.connection.query_row(
            "SELECT commit_id FROM experiment_commits WHERE experiment_id = ?1 AND completed = 1
             AND position < COALESCE((SELECT MIN(position) FROM experiment_commits
                WHERE experiment_id = ?1 AND completed = 0), 9223372036854775807)
             ORDER BY position DESC LIMIT 1", [experiment], |row| row.get(0),
        ).optional()?;
        Ok(id.map(CommitId))
    }
}

fn ensure_experiment(connection: &Connection, experiment: ExperimentId) -> Result<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM experiments WHERE id = ?1)",
        [experiment],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(Error::Storage(format!(
            "experiment {experiment} does not exist"
        )));
    }
    Ok(())
}

fn sql_index(index: usize) -> Result<i64> {
    i64::try_from(index)
        .map_err(|_| Error::Storage("workload position exceeds SQLite integer range".into()))
}

fn registered_commit(
    connection: &Connection,
    experiment: ExperimentId,
    commit: &CommitId,
) -> Result<(i64, Commit)> {
    let (position, metadata) = connection.query_row(
        "SELECT ec.position, c.metadata FROM experiment_commits ec JOIN commits c ON c.id = ec.commit_id
         WHERE ec.experiment_id = ?1 AND ec.commit_id = ?2", params![experiment, commit.0],
        |row| Ok((row.get(0)?, row.get::<_, String>(1)?)),
    ).optional()?.ok_or_else(|| Error::Storage(format!("commit {} is not registered in experiment {experiment}", commit.0)))?;
    Ok((position, serde_json::from_str(&metadata)?))
}

const RUN_SELECT: &str = "SELECT r.id, c.metadata, r.benchmark_index, b.kind, b.version, b.parameters,
    r.started_at, r.finished_at, r.inspection_source, r.artifacts, r.outcome
    FROM runs r JOIN commits c ON c.id = r.commit_id
    JOIN benchmarks b ON b.experiment_id = r.experiment_id AND b.position = r.benchmark_index
    JOIN experiment_commits ec ON ec.experiment_id = r.experiment_id AND ec.commit_id = r.commit_id";

struct RawRun {
    id: RecordId,
    commit: String,
    benchmark_index: i64,
    kind: String,
    version: u32,
    parameters: String,
    started_at: String,
    finished_at: Option<String>,
    source: Option<RecordId>,
    artifacts: Option<String>,
    outcome: Option<String>,
}

impl RawRun {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            commit: row.get(1)?,
            benchmark_index: row.get(2)?,
            kind: row.get(3)?,
            version: row.get(4)?,
            parameters: row.get(5)?,
            started_at: row.get(6)?,
            finished_at: row.get(7)?,
            source: row.get(8)?,
            artifacts: row.get(9)?,
            outcome: row.get(10)?,
        })
    }

    fn start(&self, experiment: ExperimentId) -> Result<RunStart> {
        Ok(RunStart {
            experiment,
            commit: serde_json::from_str(&self.commit)?,
            benchmark_index: usize::try_from(self.benchmark_index)
                .map_err(|_| Error::Storage("invalid stored workload position".into()))?,
            benchmark: BenchmarkDefinition {
                kind: self.kind.clone(),
                version: self.version,
                parameters: serde_json::from_str(&self.parameters)?,
            },
            started_at: serde_json::from_str(&self.started_at)?,
            purpose: self
                .source
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
}

#[cfg(test)]
mod tests;
