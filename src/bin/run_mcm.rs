use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::PathBuf,
};

use clap::Parser;

use benchmarks::{
    Error, Result,
    benchmark::{ArtifactPolicy, PerfProfiler},
    benchmarks::mcm::{
        self, CoefficientGenerator, CoefficientSets, McmOptions, McmOutput, McmSuite, McmView,
        PaperTool,
    },
    build::YosysBuilder,
    design::CellLibrary,
    experiment::{self, Experiment, RunOptions},
    history::{BranchHistory, CommitId, RevisionSelection},
    output::OutputLayout,
    store::{HistoryQuery, SqliteStore, Store},
};

#[derive(Debug, Parser)]
#[command(
    about = "Run MCM benchmarks across Yosys commits",
    version,
    args_override_self = true
)]
struct Args {
    /// Root directory for results, artifacts, and build caches.
    #[arg(long, value_name = "PATH", default_value = "output")]
    output_dir: PathBuf,
    /// Named experiment directory/database.
    #[arg(long, default_value = "mcm")]
    name: String,
    /// Reuse an existing experiment's saved suite and repository settings.
    #[arg(long, value_name = "ID", value_parser = clap::value_parser!(i64).range(1..))]
    experiment: Option<i64>,
    /// Yosys revision to run; repeat to select multiple commits.
    #[arg(long = "commit", value_name = "REV")]
    commits: Vec<String>,
    /// Zero-based workload index; repeat to select multiple workloads.
    #[arg(long = "workload", value_name = "INDEX")]
    workloads: Vec<usize>,
    /// Retain all logs, scripts, and netlists.
    #[arg(long, overrides_with = "artifacts")]
    keep_artifacts: bool,
    /// Retention: all, none, or a comma-separated subset of logs,scripts,netlists.
    /// Defaults to logs and scripts. The last retention flag wins.
    #[arg(long, value_name = "POLICY", value_parser = artifact_policy, overrides_with = "keep_artifacts")]
    artifacts: Option<ArtifactPolicy>,
    /// Parallel compiler jobs, capped at available CPUs; defaults to that cap.
    #[arg(long, value_name = "N")]
    jobs: Option<NonZeroUsize>,
    /// Record extra Yosys invocations with perf and retain source flamegraphs.
    #[arg(long)]
    perf: bool,
    /// Address resolver for perf script; requires --perf.
    #[arg(
        long,
        value_name = "PATH",
        default_value = "/usr/bin/llvm-addr2line-16",
        requires = "perf"
    )]
    perf_addr2line: PathBuf,
}

impl Args {
    fn artifact_policy(&self) -> ArtifactPolicy {
        if self.keep_artifacts {
            ArtifactPolicy {
                logs: true,
                scripts: true,
                netlists: true,
            }
        } else {
            self.artifacts.unwrap_or_default()
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let artifacts = args.artifact_policy();
    let output = OutputLayout {
        output_dir: args.output_dir,
        experiment: args.name,
    };
    let options = RunOptions {
        experiment: args.experiment,
        workloads: args.workloads,
        perf: args
            .perf
            .then(|| PerfProfiler::new(&args.perf_addr2line))
            .transpose()?,
    };
    let commits: Vec<_> = args.commits.into_iter().map(CommitId).collect();
    let mut experiment = Experiment {
        output,
        history: BranchHistory {
            repository: "../yosys".into(),
            branch: "main".into(),
        },
        // Two adjacent mainline commits, oldest first, to check compiler reuse.
        // Use a fresh --output-dir to exercise both builds instead of installed hits.
        revisions: RevisionSelection::Explicit(vec![
            CommitId("8802950c22d4dd32abd482f409c83396d04bd496".into()),
            CommitId("a31d95ea005b35bb894876e27627030bf8ef2c8a".into()),
        ]),
        suite: McmSuite {
            coefficient_sets: CoefficientSets::Generated(CoefficientGenerator::default()),
            input_width: 32,
            library: CellLibrary {
                name: "Nangate45nm".into(),
                liberty: "../yosys/tests/techmap/mcm/NangateOpenCellLibrary_typical.lib".into(),
            },
            paper: PaperTool {
                executable: "../yosys/tests/techmap/mcm/synth/acm".into(),
            },
            options: McmOptions {
                max_depth: 16,
                search_budget: 200_000_000,
                paper_seed: 1,
                paper_bit_width: 30,
            },
            repetitions: NonZeroU32::new(1).unwrap(),
        },
        artifacts,
    };

    let database = experiment.output.database_path()?;
    if options.experiment.is_some() && !database.is_file() {
        return Err(Error::Storage(format!(
            "database {} does not exist",
            database.display()
        )));
    }
    let mut store = SqliteStore::open(database)?;
    if let Some(id) = options.experiment {
        // Use the saved suite rather than today's hard-coded benchmark defaults.
        let saved = store.experiment(id)?;
        experiment.suite = serde_json::from_value(saved.configuration["suite"].clone())?;
        experiment.history = serde_json::from_value(saved.configuration["history"].clone())?;
        experiment.revisions = RevisionSelection::Explicit(
            saved.commits.into_iter().map(|commit| commit.id).collect(),
        );
    }
    if !commits.is_empty() {
        experiment.revisions = RevisionSelection::Explicit(commits);
    }

    let mut builder = YosysBuilder {
        repository: experiment.history.repository.clone(),
        cache_dir: experiment
            .output
            .output_dir
            .join("builds/yosys-relwithdebinfo"),
        jobs: args.jobs,
    };
    println!(
        "Configured {} MCM workloads",
        experiment.suite.coefficient_sets.len()
    );
    let summary = experiment::run_with_options(&experiment, &mut builder, &mut store, &options)?;
    let id = summary.experiment;
    println!("Run directory: {}", summary.run_directory.display());
    let history = store.history::<McmOutput>(&HistoryQuery {
        experiment: id,
        benchmark_kind: mcm::KIND.into(),
        benchmark_version: mcm::VERSION,
        benchmark_parameters: None,
    })?;
    println!("Experiment {id}\n");
    print!("{}", McmView.report(&history)?);
    Ok(())
}

fn artifact_policy(value: &str) -> std::result::Result<ArtifactPolicy, String> {
    let mut policy = ArtifactPolicy {
        logs: false,
        scripts: false,
        netlists: false,
    };
    match value {
        "all" => {
            return Ok(ArtifactPolicy {
                logs: true,
                scripts: true,
                netlists: true,
            });
        }
        "none" => return Ok(policy),
        _ => {}
    }
    for item in value.split(',') {
        match item {
            "logs" => policy.logs = true,
            "scripts" => policy.scripts = true,
            "netlists" => policy.netlists = true,
            _ => {
                return Err(format!(
                    "unknown artifact kind {item:?}; use all, none, or logs,scripts,netlists"
                ));
            }
        }
    }
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_selectors_append_and_scalar_options_use_the_last_value() {
        let args = Args::try_parse_from([
            "run_mcm",
            "--commit",
            "first",
            "--commit",
            "second",
            "--workload",
            "0",
            "--workload",
            "2",
            "--jobs",
            "1",
            "--jobs",
            "3",
        ])
        .unwrap();
        assert_eq!(args.commits, ["first", "second"]);
        assert_eq!(args.workloads, [0, 2]);
        assert_eq!(args.jobs.unwrap().get(), 3);
    }

    #[test]
    fn the_last_artifact_policy_flag_wins() {
        let args =
            Args::try_parse_from(["run_mcm", "--keep-artifacts", "--artifacts", "none"]).unwrap();
        let policy = args.artifact_policy();
        assert!(!policy.logs && !policy.scripts && !policy.netlists);

        let args =
            Args::try_parse_from(["run_mcm", "--artifacts", "none", "--keep-artifacts"]).unwrap();
        let policy = args.artifact_policy();
        assert!(policy.logs && policy.scripts && policy.netlists);
    }
}
