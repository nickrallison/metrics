use std::num::NonZeroU32;

use benchmarks::{
    Result,
    benchmark::ArtifactPolicy,
    benchmarks::mcm::{
        self, CoefficientGenerator, CoefficientSets, McmOptions, McmOutput, McmSuite, McmView,
        PaperTool,
    },
    build::YosysBuilder,
    design::CellLibrary,
    experiment::{self, Experiment},
    history::{BranchHistory, CommitId, RevisionSelection},
    store::{HistoryQuery, SqliteStore, Store},
};

fn main() -> Result<()> {
    let experiment = Experiment {
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
        artifacts: ArtifactPolicy::default(),
    };

    let mut builder = YosysBuilder {
        repository: experiment.history.repository.clone(),
        cache_dir: "output/builds/yosys".into(),
    };
    let mut store = SqliteStore::open("output/mcm.sqlite")?;

    println!(
        "Configured {} MCM workloads",
        experiment.suite.coefficient_sets.len()
    );
    let id = experiment::run(&experiment, &mut builder, &mut store)?;
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
