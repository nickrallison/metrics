use std::{
    path::Path,
    process::{Command, Output},
    time::{Duration, UNIX_EPOCH},
};

use benchmarks::{
    benchmark::Samples,
    benchmarks::{
        mcm::{self, McmOutput, PaperStats, YosysStats},
        synth::SynthStats,
    },
    history::{Commit, CommitId},
    output::OutputLayout,
    store::{
        BenchmarkDefinition, ExperimentDefinition, Outcome, Reference, RunPurpose, RunRecord,
        RunStart, SqliteStore, Store,
    },
};
use serde_json::{Value, json};

fn view(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_view_results"))
        .arg("--database")
        .arg(path)
        .args(args)
        .output()
        .unwrap()
}

fn text(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn fixture() -> (ExperimentDefinition, McmOutput) {
    let definition = ExperimentDefinition {
        configuration: json!({}),
        commits: vec![Commit {
            id: CommitId("0123456789abcdef".into()),
            committed_at: UNIX_EPOCH,
            author: "Test".into(),
            commit_message: "test".into(),
        }],
        benchmarks: vec![BenchmarkDefinition {
            kind: mcm::KIND.into(),
            version: mcm::VERSION,
            parameters: json!({"coefficients": [3, 5]}),
        }],
    };
    let synth = |areas: &[f64]| Samples {
        values: areas
            .iter()
            .map(|&area| SynthStats {
                area: Some(area),
                cell_count: 10,
                wall_clock_time: Duration::ZERO,
            })
            .collect(),
    };
    let output = McmOutput {
        yosys_mcm: Samples {
            values: [2, 4]
                .into_iter()
                .map(|ms| YosysStats {
                    wall_clock_time: Duration::from_millis(ms),
                    adds_subs: 1,
                    negations: 0,
                    multipliers: 0,
                    depth: Some(1),
                })
                .collect(),
        },
        paper: Reference {
            id: 1,
            measured_at: UNIX_EPOCH,
            value: Samples {
                values: [1, 3]
                    .into_iter()
                    .map(|ms| PaperStats {
                        wall_clock_time: Duration::from_millis(ms),
                        adds_subs: 1,
                        shifts: 1,
                        negations: 0,
                        multipliers: 0,
                        depth: 1,
                    })
                    .collect(),
            },
        },
        synth_with_mcm: synth(&[10.0, 30.0]),
        synth_without_mcm: synth(&[20.0, 60.0]),
    };
    (definition, output)
}

#[test]
fn cli_projects_sqlite_history_selects_latest_experiment_and_reports_failures() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("results.sqlite");
    let mut store = SqliteStore::open(&path).unwrap();
    let (definition, output) = fixture();
    let first = store.register_experiment(&definition).unwrap();
    let record = RunRecord {
        experiment: first,
        commit: definition.commits[0].clone(),
        benchmark_index: 0,
        benchmark: definition.benchmarks[0].clone(),
        started_at: UNIX_EPOCH,
        finished_at: UNIX_EPOCH,
        purpose: RunPurpose::Measurement,
        artifacts: None,
        outcome: Outcome::Succeeded(output.clone()),
    };
    let source = store.append(&record).unwrap();
    let mut inspection = record.clone();
    inspection.purpose = RunPurpose::Inspection { source };
    if let Outcome::Succeeded(output) = &mut inspection.outcome {
        output.yosys_mcm.values[0].wall_clock_time = Duration::from_secs(100);
    }
    store.append(&inspection).unwrap();
    store
        .start_run(&RunStart {
            experiment: first,
            commit: record.commit.clone(),
            benchmark_index: 0,
            benchmark: record.benchmark.clone(),
            started_at: UNIX_EPOCH,
            purpose: RunPurpose::Measurement,
            artifacts: None,
        })
        .unwrap();
    let latest = store.register_experiment(&definition).unwrap();
    let mut failed = record;
    failed.experiment = latest;
    failed.outcome = Outcome::<McmOutput>::Failed("comparison failed".into());
    store.append(&failed).unwrap();

    let first_id = first.to_string();
    let data: Value =
        serde_json::from_str(&text(&view(&path, &["--experiment", &first_id, "--json"]))).unwrap();
    assert_eq!(data["series"].as_array().unwrap().len(), 2);
    assert_eq!(data["series"][0]["points"][0]["value"], 1.5);
    assert_eq!(data["series"][1]["points"][0]["value"], 50.0);
    assert_eq!(data["series"][0]["points"][0]["coverage"]["included"], 1);
    let area: Value = serde_json::from_str(&text(&view(
        &path,
        &["--experiment", &first_id, "--metric", "area", "--json"],
    )))
    .unwrap();
    assert_eq!(area["series"].as_array().unwrap().len(), 1);
    assert_eq!(area["series"][0]["unit"], "%");
    let report: Value = serde_json::from_str(&text(&view(
        &path,
        &["--experiment", &first_id, "--details", "--json"],
    )))
    .unwrap();
    let summary = &report["commits"][0]["summary"];
    assert_eq!(summary["runtime_ratio"], 1.5);
    assert_eq!(summary["average_area_savings_percent"], 50.0);
    assert_eq!(summary["quality"]["same"], 1);
    assert!(summary["yosys_across_workloads"]["sample_stdev_ms"].is_null());
    let rendered = text(&view(&path, &[]));
    assert!(rendered.contains(&format!("Experiment {latest}")));
    assert!(rendered.contains("n/a"));
    assert!(rendered.contains("0/1"));
    assert!(rendered.contains("failed: comparison failed"));
    assert!(!rendered.contains("NaN"));

    // A commit stopped before any measurement still has an explicit missing point.
    let unmeasured = store.register_experiment(&definition).unwrap();
    store
        .record_build_failure(unmeasured, &definition.commits[0], "build failed")
        .unwrap();
    let data: Value = serde_json::from_str(&text(&view(&path, &["--json"]))).unwrap();
    let point = &data["series"][0]["points"][0];
    assert!(point["value"].is_null());
    assert_eq!(point["commit"], "0123456789abcdef");
    assert_eq!(point["coverage"]["total"], 1);
    assert_eq!(
        point["coverage"]["exclusions"][0]["reason"],
        "no finished measurement"
    );
}

#[test]
fn cli_reports_empty_missing_and_unknown_experiments_without_creating_databases() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("results.sqlite");
    let missing = view(&path, &[]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("does not exist"));
    assert!(!path.exists());
    let store = SqliteStore::open(&path).unwrap();
    let empty = view(&path, &[]);
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("no experiments recorded"));
    let unknown = view(&path, &["--experiment", "99"]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("does not exist"));
    drop(store);
}

#[test]
fn cli_resolves_named_experiment_under_configured_output_directory() {
    let root = tempfile::tempdir().unwrap();
    let output = OutputLayout {
        output_dir: root.path().into(),
        experiment: "manual-mcm".into(),
    };
    let mut store = SqliteStore::open(output.database_path().unwrap()).unwrap();
    let (definition, _) = fixture();
    store.register_experiment(&definition).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_view_results"))
        .arg("--output-dir")
        .arg(root.path())
        .args(["--name", "manual-mcm", "--json"])
        .output()
        .unwrap();
    let data: Value = serde_json::from_str(&text(&result)).unwrap();
    assert!(data["series"][0]["points"][0]["value"].is_null());
    assert_eq!(data["series"][0]["points"][0]["coverage"]["total"], 1);
    let missing = Command::new(env!("CARGO_BIN_EXE_view_results"))
        .arg("--output-dir")
        .arg(root.path())
        .args(["--name", "unknown"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(!root.path().join("unknown").exists());
}
