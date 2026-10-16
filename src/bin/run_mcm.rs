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
    /// Record extra Yosys invocations and retain flamegraphs; use --perf=false to disable.
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        require_equals = true
    )]
    perf: bool,
    /// Address resolver for perf script.
    #[arg(
        long,
        value_name = "PATH",
        default_value = "/usr/bin/llvm-addr2line-16"
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
            branch: "nick/mcm".into(),
        },
        // Every first-parent commit mentioning MCM or changing MCM files,
        // oldest first. Include tests, comments, profiling, and build fixes.
        revisions: RevisionSelection::Explicit(vec![
            // Original MCM pass merge.
            CommitId("fb1a2fdae7a84e327d239aec6e637fca4f037ae7".into()),
            // Release merge changing MCM source.
            CommitId("70bf41c29bef52fe513cf81552114b09ff325caa".into()),
            // Source-location refactor merge changing MCM source.
            CommitId("ec3ddf1adf8d66f15078356454a3c7e80f3d5c1c".into()),
            // Refactor using the HCUB paper.
            CommitId("7d3b4d260376d72e2f731040a6e5c1fc50cfedc2".into()),
            // Add test scripts.
            CommitId("6029ae358528532585509b747941c7a9b16eeb4d".into()),
            // Start profiling script.
            CommitId("f444f7d942409fa4c673856b6e85f5a66af7344c".into()),
            // Improve profiling.
            CommitId("293f668020c7024ad26863d31f09233695337b1d".into()),
            // Add profiling test files.
            CommitId("f41e296144b3a741d63409b7fb884eef56e6881c".into()),
            // Add profiling comment.
            CommitId("84d5e8a897b5468f7f4b985aee300255c4fc2c5b".into()),
            // Add cell-count statistics.
            CommitId("aa0e05f93a2a27a8e3a88c7acf012198a1376fc9".into()),
            // Add size comparison.
            CommitId("f836a636e3d838828e0f721320b6f14d50d851cc".into()),
            // Re-enable original pass options.
            CommitId("b04205e564da25c0246fa518b7b3f5bdd5435ed8".into()),
            // Remove test files.
            CommitId("edf018257b2fda232979b6051c46152ad74b0ab5".into()),
            // Start generating Python on the fly.
            CommitId("921ba6541c557fb4d07dcebc5b3b6a94832ad804".into()),
            // Get on-the-fly generation working.
            CommitId("49710f7834acb9c643d101f0b24bb4b1f42b4b68".into()),
            // Update profiling sampling distribution.
            CommitId("a1780f80e3d5e3bb0a7c701eb475fb7a24ccb1cb".into()),
            // Add comments and fix test printing.
            CommitId("ac5d1cc04f14f371cbff32503d394e4ce257ba20".into()),
            // Update settings from test results.
            CommitId("421b0ed545191ccec5a14bd30be998561cf01957".into()),
            // Add big-integer support.
            CommitId("b4452ec0f95fe901513d303d39240129bae76e57".into()),
            // Update big-integer tests.
            CommitId("102e29d3d1ed70a89395329d90b77d7ef6aa6a46".into()),
            // Fix negative coefficients.
            CommitId("5797a7c4c2833a5de64816ebf4f33347c670dc29".into()),
            // Add containers and uint64_t fast path.
            CommitId("c579a824c1a3cc96f07602cc63576431357f5a48".into()),
            // Fix overflow behavior.
            CommitId("d5fbbe2d74c556aca021fe4d85ee94218b015249".into()),
            // Add <bit> and rearrange search results.
            CommitId("07195e5af7c9399e1be49be3384b51308c86b671".into()),
            // Fix macOS build.
            CommitId("dd2741eb18147cc4266fcad1e92821fcc4e7008c".into()),
            // Limit inverse-set bit width.
            CommitId("14dba6a6a5ba30c0706cad2428b733955b4bd55b".into()),
            // Delete test.ys.
            CommitId("66281e952aec6c28bfe5f23ae8d4eb08fd740fe8".into()),
            // Add a_op_valid fast paths.
            CommitId("5355fa19fb0f7fb78f6d17c0223b56da71f0c8b2".into()),
            // Reuse previous distance-test results.
            CommitId("ea4b19e8e44019ccbaaf681bd795b77a0efe73e9".into()),
            // Remove forward search from exact tests and save intermediate work.
            CommitId("8d8437f6cffe723f1b6e3c6d90d7734a513a98fb".into()),
            // Remove unused comments.
            CommitId("481c9af00b5cbcc2d99898e3b0e3a6f5b29e337b".into()),
            // Profile against ABC without MCM.
            CommitId("962674201bc1aa17812276df86c43168583e4ed0".into()),
            // Fix unused parameters.
            CommitId("c55eb05cb151378b50a26a67ef3dd98e29e81765".into()),
            // Add average area savings.
            CommitId("d77d3ecf341fd806466296d8ad402cf7f95fa9f8".into()),
            // Fix Visual Studio build; final branch state.
            CommitId("f47cc3bfa0f4e412e5c0bc6dcd3761891f98a6f4".into()),
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
