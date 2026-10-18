use std::{borrow::Cow, cell::RefCell, fmt, fs, path::Path, time::SystemTime};

use diesel::{
    connection::{InstrumentationEvent, SimpleConnection},
    dsl::{exists, max},
    prelude::*,
    sql_types::BigInt,
};
use serde::{Serialize, de::DeserializeOwned};

use super::{
    ExperimentDefinition, ExperimentId, History, HistoryQuery, Outcome, RecordId, Reference,
    ReferenceCache, ReferenceId, RunPurpose, RunStart, Store,
};
use crate::{
    Error, Result,
    history::{Commit, CommitId},
};

mod queries;
mod rows;
mod schema;

use queries::{RunSelection, select_runs};
use rows::{BenchmarkRow, Definitions};
use schema::{
    benchmarks, build_failures, commits, experiment_commits, experiments, reference_results, runs,
};

/// Persistent typed history. Opening creates the parent directory and schema.
/// Relationships and state transitions are validated here, not by SQL constraints.
pub struct SqliteStore {
    connection: RefCell<SqliteConnection>,
    database_path: std::path::PathBuf,
}

impl fmt::Debug for SqliteStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteStore")
            .finish_non_exhaustive()
    }
}

#[derive(QueryableByName)]
struct SchemaVersion {
    #[diesel(sql_type = BigInt)]
    user_version: i64,
}

impl SqliteStore {
    /// `:memory:` opens a temporary database. Unsupported schema versions fail.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let mut connection = SqliteConnection::establish(&database_url(path)?)?;
        if std::env::var_os("BENCHMARKS_SQL").is_some_and(|value| value == "1") {
            connection.set_instrumentation(|event: InstrumentationEvent<'_>| {
                if let InstrumentationEvent::StartQuery { query, .. } = event {
                    eprintln!("sqlite: {query}");
                }
            });
        }
        // SQLite connection settings and schema initialization are the only raw
        // SQL here. Each PRAGMA runs separately, before opening a transaction.
        connection.batch_execute("PRAGMA busy_timeout = 5000;")?;
        connection.batch_execute("PRAGMA foreign_keys = ON;")?;
        connection.immediate_transaction(|connection| -> Result<()> {
            let version = diesel::sql_query("PRAGMA user_version")
                .get_result::<SchemaVersion>(connection)?
                .user_version;
            match version {
                0 => {
                    connection.batch_execute(include_str!("sqlite/schema.sql"))?;
                    connection.batch_execute(include_str!("sqlite/setup.sql"))?;
                }
                1 => {}
                version => {
                    return Err(Error::Storage(format!(
                        "unsupported schema version {version}"
                    )));
                }
            }
            Ok(())
        })?;
        Ok(Self {
            connection: RefCell::new(connection),
            database_path: path.to_path_buf(),
        })
    }

    // Preserve &self read APIs while Diesel borrows its connection mutably.
    // Helpers receive the borrowed connection, never reenter this store. A read
    // transaction keeps definitions and selected attempts in the same snapshot.
    fn read<T>(&self, operation: impl FnOnce(&mut SqliteConnection) -> Result<T>) -> Result<T> {
        let mut connection = self
            .connection
            .try_borrow_mut()
            .map_err(|_| Error::Storage("SQLite connection is already in use".into()))?;
        connection.transaction(operation)
    }

    /// Most recently registered experiment, or `None` for an empty database.
    pub fn latest_experiment(&self) -> Result<Option<ExperimentId>> {
        self.read(|connection| {
            Ok(experiments::table
                .select(max(experiments::id))
                .first(connection)?)
        })
    }

    /// All experiment IDs registered in this database, in stable order.
    pub fn experiment_ids(&self) -> Result<Vec<ExperimentId>> {
        self.read(|connection| {
            Ok(experiments::table
                .select(experiments::id)
                .order(experiments::id.asc())
                .load(connection)?)
        })
    }

    /// Load original configuration, frozen workloads, and registered revisions.
    pub fn experiment(&self, id: ExperimentId) -> Result<ExperimentDefinition> {
        self.read(|connection| load_experiment(connection, id))
    }

    /// All finished measurements, including superseded results, excluding inspections.
    pub fn attempts<O: DeserializeOwned>(&self, query: &HistoryQuery) -> Result<History<O>> {
        self.history_selection(query, RunSelection::AllFinished)
    }

    fn history_selection<O: DeserializeOwned>(
        &self,
        query: &HistoryQuery,
        selection: RunSelection,
    ) -> Result<History<O>> {
        self.read(|connection| {
            let definition = load_experiment(connection, query.experiment)?;
            if !definition.benchmarks.iter().any(|benchmark| {
                benchmark.kind == query.benchmark_kind
                    && benchmark.version == query.benchmark_version
            }) {
                return Err(Error::Storage(
                    "benchmark kind/version is not registered in this experiment".into(),
                ));
            }
            // Compare JSON values, not serialized strings whose key order may differ.
            let selected: std::collections::BTreeMap<_, _> = definition
                .benchmarks
                .iter()
                .enumerate()
                .filter(|(_, benchmark)| {
                    benchmark.kind == query.benchmark_kind
                        && benchmark.version == query.benchmark_version
                        && query
                            .benchmark_parameters
                            .as_ref()
                            .is_none_or(|filter| *filter == benchmark.parameters)
                })
                .map(|(index, benchmark)| (index, benchmark.clone()))
                .collect();
            let positions = selected
                .keys()
                .copied()
                .map(sql_index)
                .collect::<Result<Vec<_>>>()?;
            let definitions = Definitions::new(&definition);
            let records = select_runs(
                connection,
                query.experiment,
                Some(&positions),
                None,
                selection,
            )?
            .iter()
            .map(|row| row.finished(query.experiment, &definitions))
            .collect::<Result<_>>()?;
            Ok(History {
                commits: definition.commits,
                benchmarks: selected,
                records,
            })
        })
    }

    /// In-progress or interrupted attempts, including inspections.
    pub fn unfinished_runs(&self, experiment: ExperimentId) -> Result<Vec<(RecordId, RunStart)>> {
        self.read(|connection| {
            let definition = load_experiment(connection, experiment)?;
            let definitions = Definitions::new(&definition);
            select_runs(connection, experiment, None, None, RunSelection::Unfinished)?
                .iter()
                .map(|row| Ok((row.id, row.start(experiment, &definitions)?)))
                .collect()
        })
    }
}

impl ReferenceCache for SqliteStore {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Reference<T>>> {
        self.read(|connection| {
            let row = reference_results::table
                .filter(reference_results::cache_key.eq(key))
                .order(reference_results::id.desc())
                .select((
                    reference_results::id,
                    reference_results::measured_at,
                    reference_results::payload,
                ))
                .first::<(i64, String, String)>(connection)
                .optional()?;
            row.map(|(id, measured_at, payload)| {
                Ok(Reference {
                    id,
                    measured_at: serde_json::from_str(&measured_at)?,
                    value: serde_json::from_str(&payload)?,
                })
            })
            .transpose()
        })
    }

    fn insert<T: Serialize>(&mut self, key: &str, value: &T) -> Result<ReferenceId> {
        Ok(diesel::insert_into(reference_results::table)
            .values((
                reference_results::cache_key.eq(key),
                reference_results::measured_at.eq(serde_json::to_string(&SystemTime::now())?),
                reference_results::payload.eq(serde_json::to_string(value)?),
            ))
            .returning(reference_results::id)
            .get_result(self.connection.get_mut())?)
    }
}

impl Store for SqliteStore {
    fn database_path(&self) -> Option<&Path> {
        (self.database_path != Path::new(":memory:")).then_some(&self.database_path)
    }

    fn prepare_experiment(
        &mut self,
        definition: &ExperimentDefinition,
        existing: Option<ExperimentId>,
    ) -> Result<ExperimentId> {
        validate_commits(&definition.commits)?;
        self.connection
            .get_mut()
            .immediate_transaction(|connection| {
                let candidates = match existing {
                    Some(id) => vec![id],
                    None => experiments::table
                        .select(experiments::id)
                        .order(experiments::id.desc())
                        .load(connection)?,
                };
                for id in candidates {
                    let saved = load_experiment(connection, id)?;
                    if saved.benchmarks != definition.benchmarks
                        || experiment_identity(&saved.configuration)
                            != experiment_identity(&definition.configuration)
                    {
                        if existing.is_some() {
                            return Err(Error::Storage(format!(
                                "experiment {id} has different benchmark or suite settings"
                            )));
                        }
                        continue;
                    }
                    let mut position = saved.commits.len();
                    for commit in &definition.commits {
                        insert_commit(connection, commit)?;
                        if saved.commits.iter().any(|saved| saved.id == commit.id) {
                            continue;
                        }
                        register_commit(connection, id, commit, position)?;
                        position += 1;
                    }
                    return Ok(id);
                }
                insert_experiment(connection, definition)
            })
    }

    fn register_experiment(&mut self, definition: &ExperimentDefinition) -> Result<ExperimentId> {
        self.connection
            .get_mut()
            .immediate_transaction(|connection| insert_experiment(connection, definition))
    }

    fn start_run(&mut self, start: &RunStart) -> Result<RecordId> {
        let position = sql_index(start.benchmark_index)?;
        self.connection
            .get_mut()
            .immediate_transaction(|connection| {
                let (_, commit) =
                    registered_commit(connection, start.experiment, &start.commit.id)?;
                if commit != start.commit {
                    return Err(Error::CommitMetadataMismatch(start.commit.id.0.clone()));
                }
                let expected = benchmarks::table
                    .find((start.experiment, position))
                    .select(BenchmarkRow::as_select())
                    .first::<BenchmarkRow>(connection)
                    .optional()?
                    .ok_or(Error::UnregisteredBenchmark {
                        experiment: start.experiment,
                        index: start.benchmark_index,
                    })?
                    .definition()?;
                if start.benchmark != expected {
                    return Err(Error::BenchmarkMismatch {
                        experiment: start.experiment,
                        index: start.benchmark_index,
                    });
                }
                let source = match start.purpose {
                    RunPurpose::Measurement => None,
                    RunPurpose::Inspection { source } => {
                        let valid: bool = diesel::select(exists(
                            runs::table
                                .find(source)
                                .filter(runs::experiment_id.eq(start.experiment))
                                .filter(runs::commit_id.eq(&start.commit.id.0))
                                .filter(runs::benchmark_index.eq(position))
                                .filter(runs::inspection_source.is_null())
                                .filter(runs::finished_at.is_not_null()),
                        ))
                        .get_result(connection)?;
                        if !valid {
                            return Err(Error::InvalidInspectionSource(source));
                        }
                        Some(source)
                    }
                };
                let id = diesel::insert_into(runs::table)
                    .values((
                        runs::experiment_id.eq(start.experiment),
                        runs::commit_id.eq(&start.commit.id.0),
                        runs::benchmark_index.eq(position),
                        runs::started_at.eq(serde_json::to_string(&start.started_at)?),
                        runs::inspection_source.eq(source),
                        runs::artifacts.eq(start
                            .artifacts
                            .as_ref()
                            .map(serde_json::to_string)
                            .transpose()?),
                    ))
                    .returning(runs::id)
                    .get_result(connection)?;
                if source.is_none() {
                    set_completed(connection, start.experiment, &start.commit.id, false)?;
                }
                Ok(id)
            })
    }

    fn finish_run<O: Serialize>(
        &mut self,
        record: RecordId,
        finished_at: SystemTime,
        outcome: &Outcome<O>,
    ) -> Result<()> {
        self.connection
            .get_mut()
            .immediate_transaction(|connection| {
                let changed =
                    diesel::update(runs::table.find(record).filter(runs::finished_at.is_null()))
                        .set((
                            runs::finished_at.eq(serde_json::to_string(&finished_at)?),
                            runs::outcome.eq(serde_json::to_string(outcome)?),
                        ))
                        .execute(connection)?;
                if changed != 1 {
                    return Err(
                        if diesel::select(exists(runs::table.find(record)))
                            .get_result::<bool>(connection)?
                        {
                            Error::RunAlreadyFinished(record)
                        } else {
                            Error::UnknownRun(record)
                        },
                    );
                }
                Ok(())
            })
    }

    fn record_build_failure(
        &mut self,
        experiment: ExperimentId,
        commit: &Commit,
        reason: &str,
    ) -> Result<()> {
        self.connection
            .get_mut()
            .immediate_transaction(|connection| {
                let (_, expected) = registered_commit(connection, experiment, &commit.id)?;
                if expected != *commit {
                    return Err(Error::CommitMetadataMismatch(commit.id.0.clone()));
                }
                diesel::insert_into(build_failures::table)
                    .values((
                        build_failures::experiment_id.eq(experiment),
                        build_failures::commit_id.eq(&commit.id.0),
                        build_failures::recorded_at.eq(serde_json::to_string(&SystemTime::now())?),
                        build_failures::reason.eq(reason),
                    ))
                    .execute(connection)?;
                set_completed(connection, experiment, &commit.id, false)
            })
    }

    fn complete_commit(&mut self, experiment: ExperimentId, commit: &CommitId) -> Result<()> {
        if self.try_complete_commit(experiment, commit)? {
            Ok(())
        } else {
            Err(Error::Storage(
                "the latest measurement of every workload must be finished".into(),
            ))
        }
    }

    fn try_complete_commit(&mut self, experiment: ExperimentId, commit: &CommitId) -> Result<bool> {
        self.connection
            .get_mut()
            .immediate_transaction(|connection| {
                registered_commit(connection, experiment, commit)?;
                let expected: i64 = benchmarks::table
                    .filter(benchmarks::experiment_id.eq(experiment))
                    .count()
                    .get_result(connection)?;
                let latest = select_runs(
                    connection,
                    experiment,
                    None,
                    Some(&commit.0),
                    RunSelection::LatestStarted,
                )?;
                if i64::try_from(latest.len()).ok() != Some(expected)
                    || latest.iter().any(|row| row.finished_at.is_none())
                {
                    return Ok(false);
                }
                set_completed(connection, experiment, commit, true)?;
                Ok(true)
            })
    }

    fn history<O: DeserializeOwned>(&self, query: &HistoryQuery) -> Result<History<O>> {
        self.history_selection(query, RunSelection::LatestFinished)
    }

    fn all_finished<O: DeserializeOwned>(&self, query: &HistoryQuery) -> Result<History<O>> {
        SqliteStore::attempts(self, query)
    }

    fn last_complete(&self, experiment: ExperimentId) -> Result<Option<CommitId>> {
        self.read(|connection| {
            ensure_experiment(connection, experiment)?;
            let commits = experiment_commits::table
                .filter(experiment_commits::experiment_id.eq(experiment))
                .order(experiment_commits::position.asc())
                .select((experiment_commits::commit_id, experiment_commits::completed))
                .load::<(String, i64)>(connection)?;
            Ok(commits
                .into_iter()
                .take_while(|(_, completed)| *completed == 1)
                .last()
                .map(|(id, _)| CommitId(id)))
        })
    }
}

fn database_url(path: &Path) -> Result<Cow<'_, str>> {
    if let Some(url) = path.to_str() {
        return Ok(Cow::Borrowed(url));
    }
    // SQLite URI escaping preserves non-UTF-8 Unix filenames through Diesel's
    // string-based connection API, without changing the public Path interface.
    #[cfg(unix)]
    {
        let encoded: String = path
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .map(|byte| format!("%{byte:02X}"))
            .collect();
        Ok(Cow::Owned(format!("file:{encoded}")))
    }
    #[cfg(not(unix))]
    Err(Error::Storage("SQLite database path must be UTF-8".into()))
}

fn load_experiment(
    connection: &mut SqliteConnection,
    id: ExperimentId,
) -> Result<ExperimentDefinition> {
    ensure_experiment(connection, id)?;
    let configuration: String = experiments::table
        .find(id)
        .select(experiments::configuration)
        .first(connection)?;
    let benchmarks = benchmarks::table
        .filter(benchmarks::experiment_id.eq(id))
        .order(benchmarks::position.asc())
        .select(BenchmarkRow::as_select())
        .load::<BenchmarkRow>(connection)?
        .into_iter()
        .map(BenchmarkRow::definition)
        .collect::<Result<_>>()?;
    let commits = experiment_commits::table
        .inner_join(commits::table.on(commits::id.eq(experiment_commits::commit_id)))
        .filter(experiment_commits::experiment_id.eq(id))
        .order(experiment_commits::position.asc())
        .select(commits::metadata)
        .load::<String>(connection)?
        .into_iter()
        .map(|metadata| Ok(serde_json::from_str(&metadata)?))
        .collect::<Result<_>>()?;
    Ok(ExperimentDefinition {
        configuration: serde_json::from_str(&configuration)?,
        benchmarks,
        commits,
    })
}

fn experiment_identity(configuration: &serde_json::Value) -> serde_json::Value {
    let mut identity = configuration.clone();
    if let Some(object) = identity.as_object_mut() {
        for key in ["output", "artifacts", "revisions", "run_directory"] {
            object.remove(key);
        }
    }
    identity
}

fn insert_experiment(
    connection: &mut SqliteConnection,
    definition: &ExperimentDefinition,
) -> Result<ExperimentId> {
    validate_commits(&definition.commits)?;
    let id = diesel::insert_into(experiments::table)
        .values(experiments::configuration.eq(serde_json::to_string(&definition.configuration)?))
        .returning(experiments::id)
        .get_result(connection)?;
    for (position, benchmark) in definition.benchmarks.iter().enumerate() {
        diesel::insert_into(benchmarks::table)
            .values((
                benchmarks::experiment_id.eq(id),
                benchmarks::position.eq(sql_index(position)?),
                benchmarks::kind.eq(&benchmark.kind),
                benchmarks::version.eq(i64::from(benchmark.version)),
                benchmarks::parameters.eq(serde_json::to_string(&benchmark.parameters)?),
            ))
            .execute(connection)?;
    }
    for (position, commit) in definition.commits.iter().enumerate() {
        insert_commit(connection, commit)?;
        register_commit(connection, id, commit, position)?;
    }
    Ok(id)
}

fn insert_commit(connection: &mut SqliteConnection, commit: &Commit) -> Result<()> {
    diesel::insert_into(commits::table)
        .values((
            commits::id.eq(&commit.id.0),
            commits::metadata.eq(serde_json::to_string(commit)?),
        ))
        .on_conflict(commits::id)
        .do_nothing()
        .execute(connection)?;
    let metadata: String = commits::table
        .find(&commit.id.0)
        .select(commits::metadata)
        .first(connection)?;
    if serde_json::from_str::<Commit>(&metadata)? != *commit {
        return Err(Error::CommitMetadataMismatch(commit.id.0.clone()));
    }
    Ok(())
}

fn register_commit(
    connection: &mut SqliteConnection,
    experiment: ExperimentId,
    commit: &Commit,
    position: usize,
) -> Result<()> {
    let index = sql_index(position)?;
    if diesel::select(exists(
        experiment_commits::table.find((experiment, &commit.id.0)),
    ))
    .get_result::<bool>(connection)?
    {
        return Err(Error::DuplicateCommit(commit.id.0.clone()));
    }
    if diesel::select(exists(
        experiment_commits::table
            .filter(experiment_commits::experiment_id.eq(experiment))
            .filter(experiment_commits::position.eq(index)),
    ))
    .get_result::<bool>(connection)?
    {
        return Err(Error::DuplicateCommitPosition {
            experiment,
            position,
        });
    }
    diesel::insert_into(experiment_commits::table)
        .values((
            experiment_commits::experiment_id.eq(experiment),
            experiment_commits::commit_id.eq(&commit.id.0),
            experiment_commits::position.eq(index),
            experiment_commits::completed.eq(0_i64),
        ))
        .execute(connection)?;
    Ok(())
}

fn set_completed(
    connection: &mut SqliteConnection,
    experiment: ExperimentId,
    commit: &CommitId,
    completed: bool,
) -> Result<()> {
    diesel::update(experiment_commits::table.find((experiment, &commit.0)))
        .set(experiment_commits::completed.eq(i64::from(completed)))
        .execute(connection)?;
    Ok(())
}

fn ensure_experiment(connection: &mut SqliteConnection, experiment: ExperimentId) -> Result<()> {
    if !diesel::select(exists(experiments::table.find(experiment)))
        .get_result::<bool>(connection)?
    {
        return Err(Error::UnknownExperiment(experiment));
    }
    Ok(())
}

fn sql_index(index: usize) -> Result<i64> {
    i64::try_from(index).map_err(|_| Error::PositionOutOfRange(index))
}

fn validate_commits(commits: &[Commit]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for commit in commits {
        if !seen.insert(&commit.id.0) {
            return Err(Error::DuplicateCommit(commit.id.0.clone()));
        }
    }
    Ok(())
}

fn registered_commit(
    connection: &mut SqliteConnection,
    experiment: ExperimentId,
    commit: &CommitId,
) -> Result<(i64, Commit)> {
    ensure_experiment(connection, experiment)?;
    let (position, metadata) = experiment_commits::table
        .inner_join(commits::table.on(commits::id.eq(experiment_commits::commit_id)))
        .filter(experiment_commits::experiment_id.eq(experiment))
        .filter(experiment_commits::commit_id.eq(&commit.0))
        .select((experiment_commits::position, commits::metadata))
        .first::<(i64, String)>(connection)
        .optional()?
        .ok_or_else(|| Error::UnregisteredCommit {
            experiment,
            commit: commit.0.clone(),
        })?;
    Ok((position, serde_json::from_str(&metadata)?))
}

#[cfg(test)]
mod tests;
