use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::PathBuf,
};

use benchmarks::{
    Error, Result,
    benchmark::ArtifactPolicy,
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

fn main() -> Result<()> {
    let mut output = OutputLayout {
        output_dir: PathBuf::from("output"),
        experiment: "mcm".into(),
    };
    let mut artifacts = ArtifactPolicy::default();
    let mut jobs = None;
    let mut options = RunOptions::default();
    let mut commits = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--help" | "-h" => {
                println!(
                    "Usage: run_mcm [--output-dir PATH] [--name NAME] [--experiment ID] [--commit REV] [--workload INDEX] [--artifacts all|none|logs,scripts,netlists] [--keep-artifacts] [--jobs N]\nCompatible experiments are reused. Repeat --commit or --workload to select multiple items.\nDefaults: output directory 'output', experiment name 'mcm', logs/scripts retained, build jobs equal to available CPUs."
                );
                return Ok(());
            }
            "--output-dir" => {
                output.output_dir = args
                    .next()
                    .ok_or_else(|| Error::Output("--output-dir requires a path".into()))?
                    .into()
            }
            "--name" => {
                output.experiment = args
                    .next()
                    .ok_or_else(|| Error::Output("--name requires an experiment name".into()))?
            }
            "--keep-artifacts" => {
                artifacts = ArtifactPolicy {
                    logs: true,
                    scripts: true,
                    netlists: true,
                }
            }
            "--artifacts" => {
                let value = argument(&mut args, "--artifacts")?;
                artifacts = artifact_policy(&value)?;
            }
            "--experiment" => {
                let value = argument(&mut args, "--experiment")?;
                options.experiment = Some(
                    value
                        .parse::<i64>()
                        .ok()
                        .filter(|id| *id > 0)
                        .ok_or_else(|| {
                            Error::Benchmark("--experiment requires a positive integer".into())
                        })?,
                );
            }
            "--commit" => commits.push(CommitId(argument(&mut args, "--commit")?)),
            "--workload" => {
                options
                    .workloads
                    .push(argument(&mut args, "--workload")?.parse().map_err(|_| {
                        Error::Benchmark("--workload requires a nonnegative integer".into())
                    })?);
            }
            "--jobs" => {
                jobs = Some(
                    args.next()
                        .and_then(|value| value.parse::<NonZeroUsize>().ok())
                        .ok_or_else(|| {
                            Error::Benchmark("--jobs requires a positive integer".into())
                        })?,
                );
            }
            value => {
                return Err(Error::Output(format!(
                    "unknown argument {value:?}; use --help"
                )));
            }
        }
    }
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
        cache_dir: experiment.output.output_dir.join("builds/yosys"),
        jobs,
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

fn argument(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| Error::Benchmark(format!("{flag} requires a value")))
}

fn artifact_policy(value: &str) -> Result<ArtifactPolicy> {
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
                return Err(Error::Benchmark(format!(
                    "unknown artifact kind {item:?}; use all, none, or logs,scripts,netlists"
                )));
            }
        }
    }
    Ok(policy)
}
