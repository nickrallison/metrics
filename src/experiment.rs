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

use std::{fs, path::PathBuf, time::SystemTime};

use serde::Serialize;
use tempfile::TempDir;

use crate::{
    Error, Result,
    benchmark::{ArtifactPolicy, Benchmark, RunContext},
    build::Builder,
    history::{BranchHistory, Commit, RevisionSelection},
    store::{
        BenchmarkDefinition, ExperimentDefinition, ExperimentId, Outcome, RunPurpose, RunStart,
        Store,
    },
    suite::Suite,
};

#[derive(Debug, Serialize)]
pub struct Experiment<S: Suite> {
    pub history: BranchHistory,
    pub revisions: RevisionSelection,
    pub suite: S,
    pub artifacts: ArtifactPolicy,
}

/// Freeze the suite, resolve commits, then build, run, and record sequentially.
///
/// Benchmark failures are recorded and execution continues. Build, input setup,
/// workspace, and storage errors stop execution without completing that commit.
/// `Ok(())` means all requested comparisons were processed, not all succeeded.
pub fn run<S, B, D>(experiment: &Experiment<S>, builder: &mut B, store: &mut D) -> Result<()>
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
) -> Result<()>
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
    let experiment_id = store.register_experiment(&ExperimentDefinition {
        configuration: serde_json::to_value(experiment)?,
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
            let workspace = workspace(experiment_id, retain)?;
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
            // Keep requested artifacts even if the benchmark panics or finalization fails.
            let _cleanup = if retain {
                let _ = workspace.keep();
                None
            } else {
                Some(workspace)
            };
            let outcome = {
                let mut context = RunContext::new(path, experiment.artifacts, store);
                match benchmark.run(&input, &mut context) {
                    Ok(output) => Outcome::Succeeded(output),
                    Err(Error::Unsupported(reason)) => Outcome::Unsupported(reason),
                    Err(error) => Outcome::Failed(error.to_string()),
                }
            };
            store.finish_run(record, SystemTime::now(), &outcome)?;
        }
        store.complete_commit(experiment_id, &commit.id)?;
    }
    Ok(())
}

fn workspace(experiment: ExperimentId, retain: bool) -> Result<TempDir> {
    if retain {
        let root = PathBuf::from("output/artifacts").join(experiment.to_string());
        fs::create_dir_all(&root)?;
        Ok(tempfile::Builder::new()
            .prefix("attempt-")
            .tempdir_in(root)?)
    } else {
        Ok(tempfile::tempdir()?)
    }
}

#[cfg(test)]
mod tests;
