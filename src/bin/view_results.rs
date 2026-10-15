use benchmarks::{
    Result,
    benchmarks::mcm::{self, McmMetric, McmOutput, McmSelection, McmView},
    store::{HistoryQuery, SqliteStore, Store},
    view::View,
};

fn main() -> Result<()> {
    let store = SqliteStore::new("output/mcm.sqlite");
    let query = HistoryQuery {
        experiment: 1,
        benchmark_kind: mcm::KIND.into(),
        benchmark_version: mcm::VERSION,
        benchmark_parameters: None,
    };
    let selection = McmSelection {
        metric: McmMetric::RuntimeRatio,
    };

    let history = store.history::<McmOutput>(&query)?;
    let data = McmView.project(&history, &selection)?;
    // A graphical renderer can consume this same projection later.
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}
