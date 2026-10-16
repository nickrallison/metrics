use std::{io, path::PathBuf, process::ExitCode};

use benchmarks::{
    Error, Result,
    benchmarks::mcm::{self, McmMetric, McmOutput, McmSelection, McmView},
    store::{HistoryQuery, SqliteStore, Store},
    view::{View, ViewData},
};

const HELP: &str =
    "Usage: view_results [--database PATH] [--experiment ID] [--metric runtime|area|all] [--details] [--json]

Defaults: output/mcm.sqlite, latest registered experiment, both metrics, text output.
Workload indices refer to positions in the frozen benchmark collection.
Runtime ratios below 1 and area savings above 0 favor Yosys MCM.";

struct Options {
    database: PathBuf,
    experiment: Option<i64>,
    metrics: Vec<McmMetric>,
    json: bool,
    details: bool,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Self>> {
        let mut options = Self {
            database: "output/mcm.sqlite".into(),
            experiment: None,
            metrics: vec![McmMetric::RuntimeRatio, McmMetric::AreaSavings],
            json: false,
            details: false,
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!("{HELP}");
                    return Ok(None);
                }
                "--json" => options.json = true,
                "--details" => options.details = true,
                "--database" => options.database = argument(&mut args, &arg)?.into(),
                "--experiment" => {
                    let id = argument(&mut args, &arg)?.parse::<i64>().map_err(|_| {
                        invalid_argument("experiment ID must be a positive integer")
                    })?;
                    if id <= 0 {
                        return Err(invalid_argument("experiment ID must be a positive integer"));
                    }
                    options.experiment = Some(id);
                }
                "--metric" => {
                    options.metrics = match argument(&mut args, &arg)?.as_str() {
                        "runtime" => vec![McmMetric::RuntimeRatio],
                        "area" => vec![McmMetric::AreaSavings],
                        "all" => vec![McmMetric::RuntimeRatio, McmMetric::AreaSavings],
                        value => {
                            return Err(invalid_argument(format!(
                                "unknown metric {value:?}; use runtime, area, or all"
                            )));
                        }
                    };
                }
                _ => {
                    return Err(invalid_argument(format!(
                        "unknown argument {arg:?}; use --help"
                    )));
                }
            }
        }
        Ok(Some(options))
    }
}

fn argument(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| invalid_argument(format!("{flag} requires a value")))
}

fn invalid_argument(message: impl Into<String>) -> Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into()).into()
}

fn run() -> Result<()> {
    let Some(options) = Options::parse(std::env::args().skip(1))? else {
        return Ok(());
    };
    if !options.database.is_file() {
        return Err(Error::Storage(format!(
            "database {} does not exist; run an experiment first",
            options.database.display()
        )));
    }
    let store = SqliteStore::open(&options.database)?;
    let experiment = match options.experiment {
        Some(id) => id,
        None => store.latest_experiment()?.ok_or_else(|| {
            Error::Storage("no experiments recorded; run an experiment first".into())
        })?,
    };
    // Versions 1 and 2 share the output schema; their generation procedures
    // remain separate experiments and are never combined in one projection.
    let definition = store.experiment(experiment)?;
    let versions: std::collections::BTreeSet<_> = definition
        .benchmarks
        .iter()
        .filter(|benchmark| benchmark.kind == mcm::KIND)
        .map(|benchmark| benchmark.version)
        .collect();
    if versions.len() > 1 {
        return Err(Error::InvalidView(
            "experiment contains multiple MCM procedure versions".into(),
        ));
    }
    let version = versions.first().copied().unwrap_or(mcm::VERSION);
    let query = HistoryQuery {
        experiment,
        benchmark_kind: mcm::KIND.into(),
        benchmark_version: version,
        benchmark_parameters: None,
    };
    let history = store.history::<McmOutput>(&query)?;
    if options.details {
        let report = McmView.report(&history)?;
        if options.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("Experiment {experiment}\n");
            print!("{report}");
        }
        return Ok(());
    }
    let mut data = ViewData { series: Vec::new() };
    for metric in options.metrics {
        data.series
            .extend(McmView.project(&history, &McmSelection { metric })?.series);
    }
    if options.json {
        println!("{}", serde_json::to_string_pretty(&data)?);
    } else {
        println!("Experiment {experiment}\n");
        print!("{data}");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}
