//! Sequential orchestration of one suite over one branch.
//!
//! Freeze the suite, resolve commits, and register the experiment, then build and
//! run each commit in order. Do not overlap builds, reference computation, or other
//! benchmark processes with a timed process. Host activity outside this program remains
//! possible; record the execution environment and actual measurement order.
//!
//! Persist attempt start before execution and finalize its outcome afterward.
//! Interrupted attempts remain identifiable. Resume only against the same frozen
//! definitions and verified input contents. A commit is processed when every
//! expected benchmark has a terminal outcome, including failure or unsupported;
//! an interrupted attempt or build failure leaves it incomplete.
//!
//! Advance completion only through the processed prefix of the selected history.
//! A recent individual result is not a completion marker. Failed measurements can
//! be explicitly rerun; append attempts rather than overwriting history. Reruns
//! must not become extra independent repetition samples. Automatic retries and a
//! job graph are deferred. Compatible SQLite experiments are reused, retaining
//! previous attempts. Input digest verification is not implemented yet.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::Serialize;
use tempfile::TempDir;

use crate::{
    Error, Result,
    benchmark::{ArtifactPolicy, Benchmark, PerfProfiler, RunContext},
    build::Builder,
    history::{BranchHistory, Commit, RevisionSelection},
    output::OutputLayout,
    store::{
        BenchmarkDefinition, ExperimentCatalog, ExperimentDefinition, ExperimentId, Outcome,
        RunPurpose, RunStart, Store,
    },
    suite::Suite,
};

#[derive(Debug, Serialize)]
pub struct Experiment<S: Suite> {
    pub output: OutputLayout,
    pub history: BranchHistory,
    pub revisions: RevisionSelection,
    pub suite: S,
    pub artifacts: ArtifactPolicy,
}

/// Execution choices do not change the frozen benchmark collection.
#[derive(Debug, Default)]
pub struct RunOptions {
    /// Pin an existing experiment; otherwise reuse a compatible one if available.
    pub experiment: Option<ExperimentId>,
    /// Original zero-based workload positions. Empty means every workload.
    pub workloads: Vec<usize>,
    /// Collect diagnostic perf recordings after measured Yosys phases.
    pub perf: Option<PerfProfiler>,
}

#[derive(Debug)]
pub struct RunSummary {
    pub experiment: ExperimentId,
    pub run_directory: PathBuf,
}

/// Freeze the suite, resolve commits, then build, run, and record sequentially.
///
/// Benchmark failures are recorded and execution continues. Build, input setup,
/// workspace, and storage errors stop execution without completing that commit.
/// Returns the reused or newly registered experiment ID. Individual
/// comparisons may have failed; their outcomes remain available under this ID.
pub fn run<S, B, D>(
    experiment: &Experiment<S>,
    builder: &mut B,
    store: &mut D,
) -> Result<ExperimentId>
where
    S: Suite,
    B: Builder<Tool = S::Tool>,
    D: Store,
{
    Ok(run_with_options(experiment, builder, store, &RunOptions::default())?.experiment)
}

/// Execute selected workloads as new measurement attempts in a stable experiment.
pub fn run_with_options<S, B, D>(
    experiment: &Experiment<S>,
    builder: &mut B,
    store: &mut D,
    options: &RunOptions,
) -> Result<RunSummary>
where
    S: Suite,
    B: Builder<Tool = S::Tool>,
    D: Store,
{
    let benchmarks = experiment.suite.benchmarks()?;
    let commits = experiment.history.resolve(&experiment.revisions)?;
    execute(experiment, benchmarks, commits, builder, store, options)
}

#[cfg(test)]
fn run_resolved<S, B, D>(
    experiment: &Experiment<S>,
    benchmarks: Vec<S::Benchmark>,
    commits: Vec<Commit>,
    builder: &mut B,
    store: &mut D,
) -> Result<ExperimentId>
where
    S: Suite,
    B: Builder<Tool = S::Tool>,
    D: Store,
{
    Ok(execute(
        experiment,
        benchmarks,
        commits,
        builder,
        store,
        &RunOptions::default(),
    )?
    .experiment)
}

fn execute<S, B, D>(
    experiment: &Experiment<S>,
    benchmarks: Vec<S::Benchmark>,
    commits: Vec<Commit>,
    builder: &mut B,
    store: &mut D,
    options: &RunOptions,
) -> Result<RunSummary>
where
    S: Suite,
    B: Builder<Tool = S::Tool>,
    D: Store,
{
    let mut selected = std::collections::HashSet::new();
    for &index in &options.workloads {
        if index >= benchmarks.len() || !selected.insert(index) {
            return Err(Error::Benchmark(format!(
                "invalid or duplicate workload index {index}"
            )));
        }
    }
    let definitions = benchmarks
        .iter()
        .map(|benchmark| {
            Ok(BenchmarkDefinition {
                kind: benchmark.kind().into(),
                version: benchmark.version(),
                parameters: serde_json::to_value(benchmark)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let run_directory = experiment.output.create_run_directory()?;
    let mut configuration = serde_json::to_value(experiment)?;
    configuration["run_directory"] = serde_json::to_value(&run_directory)?;
    let experiment_id = store.prepare_experiment(
        &ExperimentDefinition {
            configuration: configuration.clone(),
            benchmarks: definitions.clone(),
            commits: commits.clone(),
        },
        options.experiment,
    )?;
    if let Some(database) = store.database_path() {
        let mut catalog = ExperimentCatalog::open(experiment.output.catalog_database_path())?;
        catalog.register(database, experiment_id, &experiment.output.experiment)?;
    }
    // Keep invocation settings separately from the experiment's original
    // configuration, including the requested policy when no raw files are kept.
    configuration["experiment_id"] = serde_json::to_value(experiment_id)?;
    configuration["workloads"] = serde_json::to_value(&options.workloads)?;
    configuration["perf"] = serde_json::to_value(&options.perf)?;
    fs::write(
        run_directory.join("run.json"),
        serde_json::to_vec_pretty(&configuration)?,
    )?;
    let reference_scope = format!("experiment:{experiment_id}:run:{}", run_directory.display());

    for commit in commits {
        let tool = match builder.build(&commit) {
            Ok(tool) => tool,
            Err(error) => {
                store.record_build_failure(experiment_id, &commit, &error.to_string())?;
                return Err(error);
            }
        };
        let input = experiment.suite.input(&tool)?;
        for (benchmark_index, (benchmark, definition)) in
            benchmarks.iter().zip(&definitions).enumerate()
        {
            if !selected.is_empty() && !selected.contains(&benchmark_index) {
                continue;
            }
            let retain = experiment.artifacts.logs
                || experiment.artifacts.scripts
                || experiment.artifacts.netlists
                || options.perf.is_some();
            let workspace =
                workspace(&run_directory, &commit, benchmark_index, definition, retain)?;
            let path = fs::canonicalize(workspace.path())?;
            let record = store.start_run(&RunStart {
                experiment: experiment_id,
                commit: commit.clone(),
                benchmark_index,
                benchmark: definition.clone(),
                started_at: SystemTime::now(),
                purpose: RunPurpose::Measurement,
                artifacts: retain.then(|| path.clone()),
            })?;
            // Temporary workspaces clean up on return/unwind. Retained
            // workspaces keep their files if execution or finalization fails.
            let outcome = {
                let mut context = RunContext::new(path, experiment.artifacts, store);
                context.reference_scope = Some(reference_scope.clone());
                context.perf = options.perf.as_ref();
                match benchmark.run(&input, &mut context) {
                    Ok(output) => Outcome::Succeeded(output),
                    Err(Error::Unsupported(reason)) => Outcome::Unsupported(reason),
                    Err(error) => Outcome::Failed(error.to_string()),
                }
            };
            match &outcome {
                Outcome::Failed(reason) => eprintln!(
                    "{} workload {benchmark_index} failed: {reason}",
                    commit.id.0
                ),
                Outcome::Unsupported(reason) => eprintln!(
                    "{} workload {benchmark_index} unsupported: {reason}",
                    commit.id.0
                ),
                Outcome::Succeeded(_) => {}
            }
            store.finish_run(record, SystemTime::now(), &outcome)?;
        }
        store.try_complete_commit(experiment_id, &commit.id)?;
    }
    Ok(RunSummary {
        experiment: experiment_id,
        run_directory,
    })
}

enum Workspace {
    Temporary(TempDir),
    Retained(PathBuf),
}

impl Workspace {
    fn path(&self) -> &Path {
        match self {
            Self::Temporary(directory) => directory.path(),
            Self::Retained(path) => path,
        }
    }
}

fn workspace(
    run_directory: &Path,
    commit: &Commit,
    index: usize,
    definition: &BenchmarkDefinition,
    retain: bool,
) -> Result<Workspace> {
    if retain {
        // Collection position distinguishes duplicate workloads; the full commit
        // distinguishes measurements of the same workload across the history.
        let kind: String = definition
            .kind
            .chars()
            .map(|value| {
                if value.is_ascii_alphanumeric() || matches!(value, '-' | '_') {
                    value
                } else {
                    '_'
                }
            })
            .collect();
        let commit: String = commit
            .id
            .0
            .chars()
            .map(|value| {
                if value.is_ascii_alphanumeric() {
                    value
                } else {
                    '_'
                }
            })
            .collect();
        let root = run_directory
            .join(format!("{index:04}-{kind}-{commit}"))
            .join("build_artifacts");
        fs::create_dir_all(
            root.parent()
                .expect("artifact directory has a benchmark parent"),
        )?;
        fs::create_dir(&root)?;
        Ok(Workspace::Retained(root))
    } else {
        Ok(Workspace::Temporary(
            tempfile::Builder::new()
                .prefix(".work-")
                .tempdir_in(run_directory)?,
        ))
    }
}

#[cfg(test)]
mod tests;
