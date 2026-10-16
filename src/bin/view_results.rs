use std::{io, path::PathBuf, process::ExitCode};

use benchmarks::{
    Error, Result,
    benchmarks::mcm::{self, McmMetric, McmOutput, McmSelection, McmView},
    output::OutputLayout,
    store::{HistoryQuery, Outcome, SqliteStore, Store},
    view::{View, ViewData},
};

const HELP: &str =
    "Usage: view_results [--output-dir PATH] [--name NAME] [--database PATH] [--experiment ID] [--metric runtime|area|all] [--details | --attempts] [--json | --html PATH]

Defaults: output/mcm/database.sqlite, latest registered experiment, both metrics, text output.
Workload indices refer to positions in the frozen benchmark collection.
Runtime ratios below 1 and area savings above 0 favor Yosys MCM.
--html exports a portable offline graph; open the file in a browser.
Regenerate and reload the HTML to see newly recorded measurements.";

struct Options {
    output: OutputLayout,
    database: Option<PathBuf>,
    experiment: Option<i64>,
    metrics: Vec<McmMetric>,
    json: bool,
    details: bool,
    attempts: bool,
    html: Option<PathBuf>,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Self>> {
        let mut options = Self {
            output: OutputLayout {
                output_dir: "output".into(),
                experiment: "mcm".into(),
            },
            database: None,
            experiment: None,
            metrics: vec![McmMetric::RuntimeRatio, McmMetric::AreaSavings],
            json: false,
            details: false,
            attempts: false,
            html: None,
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!("{HELP}");
                    return Ok(None);
                }
                "--json" => options.json = true,
                "--details" => options.details = true,
                "--attempts" => options.attempts = true,
                "--database" => options.database = Some(argument(&mut args, &arg)?.into()),
                "--html" => options.html = Some(argument(&mut args, &arg)?.into()),
                "--output-dir" => options.output.output_dir = argument(&mut args, &arg)?.into(),
                "--name" => options.output.experiment = argument(&mut args, &arg)?,
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
        if options.details && options.attempts {
            return Err(invalid_argument("use either --details or --attempts"));
        }
        if options.html.is_some() && (options.json || options.details || options.attempts) {
            return Err(invalid_argument(
                "--html cannot be combined with --json, --details, or --attempts",
            ));
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
    let database = match options.database {
        Some(path) => path,
        None => options.output.database_path()?,
    };
    if !database.is_file() {
        return Err(Error::Storage(format!(
            "database {} does not exist; run an experiment first",
            database.display()
        )));
    }
    if let Some(path) = &options.html {
        if path.extension().is_none_or(|ext| ext != "html") {
            return Err(invalid_argument(
                "--html output must have an .html extension",
            ));
        }
        if path.exists() && path.canonicalize()? == database.canonicalize()? {
            return Err(invalid_argument(
                "HTML output cannot overwrite the database",
            ));
        }
    }
    let store = SqliteStore::open(database)?;
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
    if options.attempts {
        let attempts = store.attempts::<McmOutput>(&query)?;
        let unfinished = store.unfinished_runs(experiment)?;
        if options.json {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"finished": attempts, "unfinished": unfinished})
                )?
            );
        } else {
            println!("Experiment {experiment}\n");
            println!("commit        workload  attempt  record  outcome");
            let mut counts = std::collections::BTreeMap::new();
            for entry in &attempts.records {
                let record = &entry.record;
                let number = counts
                    .entry((&record.commit.id.0, record.benchmark_index))
                    .or_insert(0);
                *number += 1;
                let outcome = match &record.outcome {
                    Outcome::Succeeded(_) => "succeeded".to_owned(),
                    Outcome::Failed(reason) => format!("failed: {reason}"),
                    Outcome::Unsupported(reason) => format!("unsupported: {reason}"),
                };
                println!(
                    "{:.12}  {:>8}  {:>7}  {:>6}  {outcome}",
                    record.commit.id.0, record.benchmark_index, number, entry.id
                );
                if let Some(path) = &record.artifacts {
                    println!("  artifacts: {}", path.display());
                }
            }
            for (id, start) in unfinished {
                println!(
                    "{:.12}  {:>8}        -  {:>6}  unfinished",
                    start.commit.id.0, start.benchmark_index, id
                );
            }
        }
        return Ok(());
    }
    let history = store.history::<McmOutput>(&query)?;
    if let Some(path) = options.html {
        let html = McmView
            .snapshot(experiment, &history, &options.metrics)?
            .render()?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, html)?;
        println!(
            "Experiment {experiment}: graph written to {}",
            path.display()
        );
        return Ok(());
    }
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
