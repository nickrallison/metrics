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
    experiment::{self, Experiment},
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
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!(
                    "Usage: run_mcm [--output-dir PATH] [--name NAME] [--keep-artifacts] [--jobs N]\nDefaults: output directory 'output', experiment name 'mcm', raw artifacts off, build jobs equal to available CPUs."
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
    let experiment = Experiment {
        output,
        history: BranchHistory {
            repository: "../yosys".into(),
            branch: "main".into(),
        },
        // Replace this anchor with a commit on the selected Yosys branch.
        revisions: RevisionSelection::Since(CommitId(
            "8802950c22d4dd32abd482f409c83396d04bd496".into(),
        )),
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

    let mut builder = YosysBuilder {
        repository: experiment.history.repository.clone(),
        cache_dir: experiment.output.output_dir.join("builds/yosys"),
        jobs,
    };
    let mut store = SqliteStore::open(experiment.output.database_path()?)?;

    println!(
        "Configured {} MCM workloads",
        experiment.suite.coefficient_sets.len()
    );
    let id = experiment::run(&experiment, &mut builder, &mut store)?;
    let definition = store.experiment(id)?;
    if let Some(path) = definition.configuration["run_directory"].as_str() {
        println!("Run directory: {path}");
    }
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
