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
//! job graph are deferred. Registration currently starts a new experiment;
//! automatic resume and input digest verification are not implemented yet.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::Serialize;
use tempfile::TempDir;

use crate::{
    Error, Result,
    benchmark::{ArtifactPolicy, Benchmark, RunContext},
    build::Builder,
    history::{BranchHistory, Commit, RevisionSelection},
    output::OutputLayout,
    store::{
        BenchmarkDefinition, ExperimentDefinition, ExperimentId, Outcome, RunPurpose, RunStart,
        Store,
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

/// Freeze the suite, resolve commits, then build, run, and record sequentially.
///
/// Benchmark failures are recorded and execution continues. Build, input setup,
/// workspace, and storage errors stop execution without completing that commit.
/// Returns the experiment ID when all comparisons were processed. Individual
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
    let benchmarks = experiment.suite.benchmarks()?;
    let commits = experiment.history.resolve(&experiment.revisions)?;
    run_resolved(experiment, benchmarks, commits, builder, store)
}

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
    let experiment_id = store.register_experiment(&ExperimentDefinition {
        configuration,
        benchmarks: definitions.clone(),
        commits: commits.clone(),
    })?;

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
            let retain = experiment.artifacts.logs
                || experiment.artifacts.scripts
                || experiment.artifacts.netlists;
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
                context.reference_scope = Some(format!("experiment:{experiment_id}"));
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
        store.complete_commit(experiment_id, &commit.id)?;
    }
    Ok(experiment_id)
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
