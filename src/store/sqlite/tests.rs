use std::{
    path::PathBuf,
    time::{Duration, UNIX_EPOCH},
};

use serde::{Deserialize, Serializer};
use serde_json::json;

use super::*;
use crate::store::{BenchmarkDefinition, RunRecord};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Metrics {
    cells: u64,
    runtime: Duration,
}

fn metrics(cells: u64) -> Metrics {
    Metrics {
        cells,
        runtime: Duration::from_millis(17),
    }
}

fn definition() -> ExperimentDefinition {
    ExperimentDefinition {
        configuration: json!({"suite": "test", "branch": "main"}),
        benchmarks: [3, 5]
            .into_iter()
            .map(|coefficient| BenchmarkDefinition {
                kind: "test".into(),
                version: 1,
                parameters: json!({"coefficient": coefficient}),
            })
            .collect(),
        // Lexical/timestamp order differs from the selected revision order.
        commits: [("b", 200), ("a", 100)]
            .into_iter()
            .map(|(id, seconds)| Commit {
                id: CommitId(id.into()),
                committed_at: UNIX_EPOCH + Duration::from_secs(seconds),
                author: "Author <author@example.test>".into(),
                commit_message: format!("{id}\n\nBody\n"),
            })
            .collect(),
    }
}

fn start(
    experiment: ExperimentId,
    definition: &ExperimentDefinition,
    commit: usize,
    benchmark: usize,
) -> RunStart {
    RunStart {
        experiment,
        commit: definition.commits[commit].clone(),
        benchmark_index: benchmark,
        benchmark: definition.benchmarks[benchmark].clone(),
        started_at: UNIX_EPOCH + Duration::from_secs(1_000),
        purpose: RunPurpose::Measurement,
        artifacts: Some(PathBuf::from("/tmp/artifacts")),
    }
}

fn finish(store: &mut SqliteStore, id: RecordId, cells: u64) {
    store
        .finish_run(id, SystemTime::now(), &Outcome::Succeeded(metrics(cells)))
        .unwrap();
}

fn query(experiment: ExperimentId) -> HistoryQuery {
    HistoryQuery {
        experiment,
        benchmark_kind: "test".into(),
        benchmark_version: 1,
        benchmark_parameters: None,
    }
}

fn database() -> (SqliteStore, ExperimentDefinition, ExperimentId) {
    let mut store = SqliteStore::open(":memory:").unwrap();
    let definition = definition();
    let id = store.register_experiment(&definition).unwrap();
    (store, definition, id)
}

#[test]
fn prepares_stable_experiments_across_policies_subsets_and_new_commits() {
    let mut store = SqliteStore::open(":memory:").unwrap();
    let mut original = definition();
    original.configuration["artifacts"] = json!({"logs": false});
    original.configuration["run_directory"] = json!("first");
    let experiment = store.prepare_experiment(&original, None).unwrap();
    let mut rerun = original.clone();
    rerun.configuration["artifacts"] = json!({"logs": true});
    rerun.configuration["run_directory"] = json!("second");
    rerun.configuration["revisions"] = json!({"Explicit": ["a"]});
    rerun.commits = vec![original.commits[1].clone()];
    assert_eq!(store.prepare_experiment(&rerun, None).unwrap(), experiment);
    assert_eq!(
        store.prepare_experiment(&rerun, Some(experiment)).unwrap(),
        experiment
    );
    assert_eq!(
        store.experiment(experiment).unwrap().configuration["run_directory"],
        "first"
    );

    let mut next = original.commits[1].clone();
    next.id.0 = "c".into();
    rerun.commits = vec![next.clone()];
    assert_eq!(store.prepare_experiment(&rerun, None).unwrap(), experiment);
    assert_eq!(
        store.experiment(experiment).unwrap().commits,
        [original.commits, vec![next]].concat()
    );

    rerun.benchmarks[0].version += 1;
    assert!(store.prepare_experiment(&rerun, Some(experiment)).is_err());
    assert_ne!(store.prepare_experiment(&rerun, None).unwrap(), experiment);
}

#[test]
fn reuse_validation_rolls_back_new_commits_and_all_attempts_remain_queryable() {
    let (mut store, definition, experiment) = database();
    let mut bad = definition.clone();
    let mut next = bad.commits[0].clone();
    next.id.0 = "c".into();
    bad.commits = vec![next.clone(), next];
    assert!(store.prepare_experiment(&bad, Some(experiment)).is_err());
    assert_eq!(
        store.experiment(experiment).unwrap().commits,
        definition.commits
    );
    for cells in [7, 8] {
        let record = store
            .start_run(&start(experiment, &definition, 0, 0))
            .unwrap();
        finish(&mut store, record, cells);
    }
    assert!(
        !store
            .try_complete_commit(experiment, &definition.commits[0].id)
            .unwrap()
    );
    assert_eq!(
        store
            .history::<Metrics>(&query(experiment))
            .unwrap()
            .records
            .len(),
        1
    );
    let attempts = store.attempts::<Metrics>(&query(experiment)).unwrap();
    assert_eq!(attempts.records.len(), 2);
    assert!(matches!(
        attempts.records[0].record.outcome,
        Outcome::Succeeded(Metrics { cells: 7, .. })
    ));
    assert!(matches!(
        attempts.records[1].record.outcome,
        Outcome::Succeeded(Metrics { cells: 8, .. })
    ));
}

#[test]
fn records_pending_attempts_and_references_survive_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested/history.sqlite");
    let definition = definition();
    let mut store = SqliteStore::open(&path).unwrap();
    let experiment = store.register_experiment(&definition).unwrap();
    let first = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    finish(&mut store, first, 7);
    let pending = store
        .start_run(&start(experiment, &definition, 0, 1))
        .unwrap();
    let reference = store.insert("paper", &metrics(42)).unwrap();
    store
        .record_build_failure(experiment, &definition.commits[1], "compiler failed")
        .unwrap();
    drop(store);

    let mut store = SqliteStore::open(&path).unwrap();
    let frozen = store.experiment(experiment).unwrap();
    assert_eq!(
        serde_json::to_value(frozen).unwrap(),
        serde_json::to_value(&definition).unwrap()
    );
    let unfinished = store.unfinished_runs(experiment).unwrap();
    assert_eq!(unfinished.len(), 1);
    assert_eq!(unfinished[0].0, pending);
    assert_eq!(unfinished[0].1.benchmark_index, 1);
    assert_eq!(unfinished[0].1.artifacts, Some("/tmp/artifacts".into()));
    let history = store.history::<Metrics>(&query(experiment)).unwrap();
    assert_eq!(history.records.len(), 1);
    assert_eq!(history.records[0].id, first);
    assert_eq!(history.commits, definition.commits);
    assert_eq!(
        history.benchmarks.keys().copied().collect::<Vec<_>>(),
        [0, 1]
    );
    match &history.records[0].record.outcome {
        Outcome::Succeeded(value) => assert_eq!(*value, metrics(7)),
        outcome => panic!("unexpected outcome: {outcome:?}"),
    }
    assert_eq!(
        store.get::<Metrics>("paper").unwrap().unwrap().id,
        reference
    );
    let reason: String = store
        .read(|connection| {
            Ok(build_failures::table
                .select(build_failures::reason)
                .first(connection)?)
        })
        .unwrap();
    assert_eq!(reason, "compiler failed");
    assert!(
        store
            .complete_commit(experiment, &definition.commits[0].id)
            .is_err()
    );
    store
        .finish_run::<Metrics>(
            pending,
            SystemTime::now(),
            &Outcome::Failed("parse failed".into()),
        )
        .unwrap();
    store
        .complete_commit(experiment, &definition.commits[0].id)
        .unwrap();
    assert_eq!(
        store.last_complete(experiment).unwrap(),
        Some(definition.commits[0].id.clone())
    );
}

#[test]
fn history_uses_frozen_order_and_latest_finished_measurement() {
    let (mut store, definition, experiment) = database();
    for commit in (0..2).rev() {
        for benchmark in (0..2).rev() {
            let id = store
                .start_run(&start(experiment, &definition, commit, benchmark))
                .unwrap();
            finish(&mut store, id, 1);
        }
    }
    let retry = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    finish(&mut store, retry, 9);
    let mut inspection = start(experiment, &definition, 0, 0);
    inspection.purpose = RunPurpose::Inspection { source: retry };
    let inspection = store.start_run(&inspection).unwrap();
    finish(&mut store, inspection, 999);
    store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();

    let history = store.history::<Metrics>(&query(experiment)).unwrap();
    assert_eq!(history.records.len(), 4);
    let order: Vec<_> = history
        .records
        .iter()
        .map(|entry| {
            (
                entry.record.commit.id.0.as_str(),
                entry.record.benchmark_index,
            )
        })
        .collect();
    assert_eq!(order, [("b", 0), ("b", 1), ("a", 0), ("a", 1)]);
    assert_eq!(history.records[0].id, retry);
    assert!(
        matches!(&history.records[0].record.outcome, Outcome::Succeeded(value) if value.cells == 9)
    );
    let mut filtered = query(experiment);
    filtered.benchmark_parameters = Some(json!({"coefficient": 5}));
    let selected = store.history::<Metrics>(&filtered).unwrap();
    assert_eq!(selected.records.len(), 2);
    assert_eq!(selected.commits, definition.commits);
    assert_eq!(selected.benchmarks.keys().copied().collect::<Vec<_>>(), [1]);
    filtered.benchmark_parameters = Some(json!({"coefficient": 99}));
    let selected = store.history::<Metrics>(&filtered).unwrap();
    assert!(selected.records.is_empty());
    assert!(selected.benchmarks.is_empty());
    assert_eq!(selected.commits, definition.commits);
}

#[test]
fn latest_failed_measurement_is_not_hidden_by_an_older_success() {
    let (mut store, definition, experiment) = database();
    let original = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    finish(&mut store, original, 1);
    let retry = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    store
        .finish_run::<Metrics>(retry, SystemTime::now(), &Outcome::Failed("failed".into()))
        .unwrap();
    let history = store.history::<Metrics>(&query(experiment)).unwrap();
    assert_eq!(history.records.len(), 1);
    assert_eq!(history.records[0].id, retry);
    assert!(
        matches!(&history.records[0].record.outcome, Outcome::Failed(reason) if reason == "failed")
    );
}

#[test]
fn completion_is_per_commit_and_the_processed_prefix_still_stops_at_gaps() {
    let (mut store, definition, experiment) = database();
    assert_eq!(store.last_complete(experiment).unwrap(), None);
    for benchmark in 0..2 {
        let id = store
            .start_run(&start(experiment, &definition, 1, benchmark))
            .unwrap();
        finish(&mut store, id, 1);
    }
    store
        .complete_commit(experiment, &definition.commits[1].id)
        .unwrap();
    assert_eq!(store.last_complete(experiment).unwrap(), None);
    let old = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    assert!(
        store
            .complete_commit(experiment, &definition.commits[0].id)
            .is_err()
    );
    // An interrupted attempt stays visible but can be superseded by a retry.
    let retry = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    store
        .finish_run::<Metrics>(retry, SystemTime::now(), &Outcome::Failed("failed".into()))
        .unwrap();
    let unsupported = store
        .start_run(&start(experiment, &definition, 0, 1))
        .unwrap();
    store
        .finish_run::<Metrics>(
            unsupported,
            SystemTime::now(),
            &Outcome::Unsupported("old tool".into()),
        )
        .unwrap();
    store
        .complete_commit(experiment, &definition.commits[0].id)
        .unwrap();
    store
        .complete_commit(experiment, &definition.commits[1].id)
        .unwrap();
    assert_eq!(
        store.last_complete(experiment).unwrap(),
        Some(definition.commits[1].id.clone())
    );
    assert_eq!(store.unfinished_runs(experiment).unwrap()[0].0, old);

    let pending = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    assert_eq!(store.last_complete(experiment).unwrap(), None);
    assert!(
        store
            .complete_commit(experiment, &definition.commits[0].id)
            .is_err()
    );
    finish(&mut store, pending, 7);
    store
        .complete_commit(experiment, &definition.commits[0].id)
        .unwrap();
    assert_eq!(
        store.last_complete(experiment).unwrap(),
        Some(definition.commits[1].id.clone())
    );
}

#[test]
fn duplicate_definitions_are_distinct_frozen_workloads() {
    let mut store = SqliteStore::open(":memory:").unwrap();
    let mut definition = definition();
    definition.benchmarks[1] = definition.benchmarks[0].clone();
    let experiment = store.register_experiment(&definition).unwrap();
    let first = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    finish(&mut store, first, 1);
    assert!(
        store
            .complete_commit(experiment, &definition.commits[0].id)
            .is_err()
    );
    let second = store
        .start_run(&start(experiment, &definition, 0, 1))
        .unwrap();
    finish(&mut store, second, 2);
    store
        .complete_commit(experiment, &definition.commits[0].id)
        .unwrap();
    assert_eq!(
        store
            .history::<Metrics>(&query(experiment))
            .unwrap()
            .records
            .len(),
        2
    );
}

#[test]
fn inspections_do_not_reopen_completed_commits() {
    let (mut store, definition, experiment) = database();
    let mut ids = Vec::new();
    for benchmark in 0..2 {
        let id = store
            .start_run(&start(experiment, &definition, 0, benchmark))
            .unwrap();
        finish(&mut store, id, 1);
        ids.push(id);
    }
    store
        .complete_commit(experiment, &definition.commits[0].id)
        .unwrap();
    let mut inspection = start(experiment, &definition, 0, 0);
    inspection.purpose = RunPurpose::Inspection { source: ids[0] };
    store.start_run(&inspection).unwrap();
    assert_eq!(
        store.last_complete(experiment).unwrap(),
        Some(definition.commits[0].id.clone())
    );
}

#[test]
fn invalid_inspection_links_are_rejected() {
    let (mut store, definition, experiment) = database();
    let source = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    let mut inspection = start(experiment, &definition, 0, 0);
    inspection.purpose = RunPurpose::Inspection { source };
    assert!(store.start_run(&inspection).is_err()); // source unfinished
    finish(&mut store, source, 1);
    for (commit, benchmark) in [(1, 0), (0, 1)] {
        let mut wrong = start(experiment, &definition, commit, benchmark);
        wrong.purpose = RunPurpose::Inspection { source };
        assert!(store.start_run(&wrong).is_err());
    }
    let id = store.start_run(&inspection).unwrap();
    finish(&mut store, id, 99);
    inspection.purpose = RunPurpose::Inspection { source: id };
    assert!(store.start_run(&inspection).is_err()); // source is another inspection
}

#[test]
fn attempt_metadata_must_match_frozen_definitions() {
    let (mut store, definition, experiment) = database();
    let original = start(experiment, &definition, 0, 0);
    let mut wrong = original.clone();
    wrong.commit.author = "changed".into();
    assert!(store.start_run(&wrong).is_err());
    wrong = original.clone();
    wrong.commit.id = CommitId("unknown".into());
    assert!(store.start_run(&wrong).is_err());
    wrong = original.clone();
    wrong.benchmark.parameters = json!({"coefficient": 8});
    assert!(store.start_run(&wrong).is_err());
    wrong = original.clone();
    wrong.benchmark.version = 2;
    assert!(store.start_run(&wrong).is_err());
    wrong = original.clone();
    wrong.benchmark_index = usize::MAX;
    assert!(store.start_run(&wrong).is_err());
    wrong = original;
    wrong.experiment = 999;
    assert!(store.start_run(&wrong).is_err());
    assert!(store.unfinished_runs(experiment).unwrap().is_empty());
}

struct Unserializable;

impl Serialize for Unserializable {
    fn serialize<S: Serializer>(&self, _: S) -> std::result::Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom(
            "intentional serialization failure",
        ))
    }
}

#[test]
fn finalization_is_once_only_and_serialization_failure_leaves_attempt_pending() {
    let (mut store, definition, experiment) = database();
    let id = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    assert!(matches!(
        store.finish_run(id, SystemTime::now(), &Outcome::Succeeded(Unserializable)),
        Err(Error::Serialization(_))
    ));
    assert_eq!(store.unfinished_runs(experiment).unwrap().len(), 1);
    finish(&mut store, id, 7);
    assert!(
        store
            .finish_run(id, SystemTime::now(), &Outcome::Succeeded(metrics(99)))
            .is_err()
    );
    assert!(
        store
            .finish_run(999, SystemTime::now(), &Outcome::Succeeded(metrics(99)))
            .is_err()
    );
    assert!(
        matches!(&store.history::<Metrics>(&query(experiment)).unwrap().records[0].record.outcome, Outcome::Succeeded(value) if value.cells == 7)
    );
}

#[test]
fn registration_rolls_back_invalid_definitions_and_keeps_existing_metadata_immutable() {
    let mut store = SqliteStore::open(":memory:").unwrap();
    let mut invalid = definition();
    invalid.commits.push(invalid.commits[0].clone());
    assert!(store.register_experiment(&invalid).is_err());
    let count: i64 = store
        .read(|connection| Ok(experiments::table.count().get_result(connection)?))
        .unwrap();
    assert_eq!(count, 0);
    let mut definition = definition();
    let experiment = store.register_experiment(&definition).unwrap();
    definition.commits[0].author = "wrong metadata".into();
    assert!(matches!(
        store.register_experiment(&definition),
        Err(Error::Storage(_))
    ));
    assert_ne!(
        store.experiment(experiment).unwrap().commits[0].author,
        "wrong metadata"
    );
    let count: i64 = store
        .read(|connection| Ok(experiments::table.count().get_result(connection)?))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn history_rejects_unknown_experiments_schemas_and_incompatible_payload_types() {
    let (mut store, definition, experiment) = database();
    let id = store
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    finish(&mut store, id, 1);
    let mut wrong = query(experiment);
    wrong.benchmark_version = 2;
    assert!(matches!(
        store.history::<Metrics>(&wrong),
        Err(Error::Storage(_))
    ));
    wrong = query(experiment);
    wrong.benchmark_kind = "other".into();
    assert!(matches!(
        store.history::<Metrics>(&wrong),
        Err(Error::Storage(_))
    ));
    assert!(matches!(
        store.history::<String>(&query(experiment)),
        Err(Error::Serialization(_))
    ));
    assert!(store.history::<Metrics>(&query(999)).is_err());
    assert!(store.last_complete(999).is_err());
    assert!(store.unfinished_runs(999).is_err());
}

#[test]
fn reference_refresh_appends_and_failed_serialization_does_not_replace_the_latest() {
    let mut store = SqliteStore::open(":memory:").unwrap();
    assert!(store.get::<Metrics>("paper").unwrap().is_none());
    let first = store.insert("paper", &metrics(1)).unwrap();
    let second = store.insert("paper", &metrics(2)).unwrap();
    assert_ne!(first, second);
    assert!(store.insert("paper", &Unserializable).is_err());
    let latest = store.get::<Metrics>("paper").unwrap().unwrap();
    assert_eq!(latest.id, second);
    assert_eq!(latest.value, metrics(2));
    assert!(matches!(
        store.get::<String>("paper"),
        Err(Error::Serialization(_))
    ));
    let old: String = store
        .read(|connection| {
            Ok(reference_results::table
                .find(first)
                .select(reference_results::payload)
                .first(connection)?)
        })
        .unwrap();
    assert_eq!(serde_json::from_str::<Metrics>(&old).unwrap(), metrics(1));
}

#[test]
fn attempt_start_and_finalization_are_visible_to_another_connection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.sqlite");
    let mut writer = SqliteStore::open(&path).unwrap();
    let definition = definition();
    let experiment = writer.register_experiment(&definition).unwrap();
    let mut reader = SqliteStore::open(&path).unwrap();
    let id = writer
        .start_run(&start(experiment, &definition, 0, 0))
        .unwrap();
    assert_eq!(reader.unfinished_runs(experiment).unwrap().len(), 1);
    finish(&mut reader, id, 8);
    assert!(writer.unfinished_runs(experiment).unwrap().is_empty());
    assert_eq!(
        writer
            .history::<Metrics>(&query(experiment))
            .unwrap()
            .records[0]
            .id,
        id
    );
}

#[test]
fn build_failure_reopens_completion_and_persists_the_reason() {
    let (mut store, definition, experiment) = database();
    for benchmark in 0..2 {
        let id = store
            .start_run(&start(experiment, &definition, 0, benchmark))
            .unwrap();
        finish(&mut store, id, 1);
    }
    store
        .complete_commit(experiment, &definition.commits[0].id)
        .unwrap();
    store
        .record_build_failure(experiment, &definition.commits[0], "build error")
        .unwrap();
    assert_eq!(store.last_complete(experiment).unwrap(), None);
    let reason: String = store
        .read(|connection| {
            Ok(build_failures::table
                .select(build_failures::reason)
                .first(connection)?)
        })
        .unwrap();
    assert_eq!(reason, "build error");
}

#[test]
fn append_uses_the_same_started_attempt_and_preserves_metadata() {
    let (mut store, definition, experiment) = database();
    let start = start(experiment, &definition, 0, 0);
    let record = RunRecord {
        experiment,
        commit: start.commit,
        benchmark_index: 0,
        benchmark: start.benchmark,
        started_at: start.started_at,
        finished_at: SystemTime::now(),
        purpose: start.purpose,
        artifacts: start.artifacts,
        outcome: Outcome::Succeeded(metrics(17)),
    };
    let id = store.append(&record).unwrap();
    let history = store.history::<Metrics>(&query(experiment)).unwrap();
    assert_eq!(history.records[0].id, id);
    assert_eq!(history.records[0].record.artifacts, record.artifacts);
    assert_eq!(history.records[0].record.started_at, record.started_at);
    assert!(store.unfinished_runs(experiment).unwrap().is_empty());
}

#[test]
fn unsupported_schema_versions_fail_without_altering_existing_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("future.sqlite");
    let mut connection = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
    connection.batch_execute("CREATE TABLE sentinel(value TEXT); INSERT INTO sentinel VALUES ('keep'); PRAGMA user_version = 2;").unwrap();
    drop(connection);
    assert!(matches!(SqliteStore::open(&path), Err(Error::Storage(_))));
    let mut connection = SqliteConnection::establish(path.to_str().unwrap()).unwrap();
    #[derive(QueryableByName)]
    struct Sentinel {
        #[diesel(sql_type = diesel::sql_types::Text)]
        value: String,
    }
    let row = diesel::sql_query("SELECT value FROM sentinel")
        .get_result::<Sentinel>(&mut connection)
        .unwrap();
    assert_eq!(row.value, "keep");
}

#[test]
fn ids_and_benchmark_versions_keep_their_full_integer_range() {
    const LARGE: i64 = i32::MAX as i64 + 100;
    let mut store = SqliteStore::open(":memory:").unwrap();
    diesel::insert_into(experiments::table)
        .values((
            experiments::id.eq(LARGE),
            experiments::configuration.eq("{}"),
        ))
        .execute(store.connection.get_mut())
        .unwrap();
    let mut definition = definition();
    definition.benchmarks[0].version = u32::MAX;
    let experiment = store.register_experiment(&definition).unwrap();
    assert!(experiment > LARGE);
    let attempt = start(experiment, &definition, 0, 0);
    diesel::insert_into(runs::table)
        .values((
            runs::id.eq(LARGE),
            runs::experiment_id.eq(experiment),
            runs::commit_id.eq(&attempt.commit.id.0),
            runs::benchmark_index.eq(0),
            runs::started_at.eq(serde_json::to_string(&attempt.started_at).unwrap()),
        ))
        .execute(store.connection.get_mut())
        .unwrap();
    let id = store.start_run(&attempt).unwrap();
    assert!(id > LARGE);
    finish(&mut store, id, 7);
    let mut inspection = attempt;
    inspection.purpose = RunPurpose::Inspection { source: id };
    let inspected = store.start_run(&inspection).unwrap();
    finish(&mut store, inspected, 99);
    let mut selected = query(experiment);
    selected.benchmark_version = u32::MAX;
    let history = store.history::<Metrics>(&selected).unwrap();
    assert_eq!(history.records[0].id, id);
    assert_eq!(history.records[0].record.benchmark.version, u32::MAX);

    diesel::insert_into(reference_results::table)
        .values((
            reference_results::id.eq(LARGE),
            reference_results::cache_key.eq("paper"),
            reference_results::measured_at.eq(serde_json::to_string(&UNIX_EPOCH).unwrap()),
            reference_results::payload.eq(serde_json::to_string(&metrics(1)).unwrap()),
        ))
        .execute(store.connection.get_mut())
        .unwrap();
    let reference = store.insert("paper", &metrics(2)).unwrap();
    assert!(reference > LARGE);
    assert_eq!(
        store.get::<Metrics>("paper").unwrap().unwrap().id,
        reference
    );
}

#[test]
fn nested_connection_borrows_return_an_error_without_poisoning_the_store() {
    let store = SqliteStore::open(":memory:").unwrap();
    let borrowed = store.connection.borrow_mut();
    assert!(matches!(store.latest_experiment(), Err(Error::Storage(_))));
    drop(borrowed);
    assert_eq!(store.latest_experiment().unwrap(), None);
}

#[cfg(unix)]
#[test]
fn non_utf8_database_paths_still_open_and_reopen() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join(OsString::from_vec(b"history-\xff.sqlite".to_vec()));
    let mut store = SqliteStore::open(&path).unwrap();
    let experiment = store.register_experiment(&definition()).unwrap();
    drop(store);
    assert!(path.is_file());
    assert_eq!(
        SqliteStore::open(&path)
            .unwrap()
            .latest_experiment()
            .unwrap(),
        Some(experiment)
    );
}
