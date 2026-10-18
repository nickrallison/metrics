use benchmarks::{
    benchmarks::mcm::{self, McmMetric, McmOutput, McmSelection, McmView},
    store::{Analysis, ExperimentId, Frame, History, HistoryQuery, Query, SqliteStore, Store},
    view::View,
};

use crate::{
    analysis::AnalysisProvider,
    presentation::{
        AnalysisDescriptor, AnalysisRequest, AnalysisResult, ChartPoint, ChartSeries, ChartSpec,
        DetailField, ParameterDescriptor, ParameterKind, ParameterOption, ReferenceLine,
    },
};

pub struct McmPerformance;

impl AnalysisProvider<String> for McmPerformance {
    fn descriptor(&self) -> AnalysisDescriptor {
        AnalysisDescriptor {
            id: "mcm.performance".into(),
            label: "Performance history".into(),
            description: "Compare measurements across the experiment's frozen commit order.".into(),
            parameters: vec![
                ParameterDescriptor {
                    id: "metric".into(),
                    label: "Metric".into(),
                    kind: ParameterKind::Select,
                    options: vec![
                        ParameterOption {
                            value: "runtime".into(),
                            label: "Runtime ratio".into(),
                        },
                        ParameterOption {
                            value: "area".into(),
                            label: "Area savings".into(),
                        },
                    ],
                    default: Some("runtime".into()),
                },
                ParameterDescriptor {
                    id: "series".into(),
                    label: "Series".into(),
                    kind: ParameterKind::Select,
                    options: vec![
                        ParameterOption {
                            value: "aggregate".into(),
                            label: "Aggregate inputs".into(),
                        },
                        ParameterOption {
                            value: "workloads".into(),
                            label: "Each input".into(),
                        },
                    ],
                    default: Some("aggregate".into()),
                },
            ],
        }
    }

    fn supports(&self, benchmark_types: &[(String, u32)]) -> bool {
        benchmark_types
            .iter()
            .any(|(kind, version)| kind == mcm::KIND && [1, mcm::VERSION].contains(version))
    }

    fn run(&self, request: &AnalysisRequest<String>) -> Result<AnalysisResult, String> {
        let experiment = request
            .experiment
            .parse::<ExperimentId>()
            .map_err(|_| "Invalid experiment identifier".to_string())?;
        let metric = match request.parameters.get("metric").and_then(|v| v.as_str()) {
            Some("area") => McmMetric::AreaSavings,
            Some("runtime") | None => McmMetric::RuntimeRatio,
            Some(value) => return Err(format!("Unknown metric {value:?}")),
        };
        let projection = match request.parameters.get("series").and_then(|v| v.as_str()) {
            Some("workloads") => Projection::EachWorkload,
            Some("aggregate") | None => Projection::Aggregate,
            Some(value) => return Err(format!("Unknown series selection {value:?}")),
        };

        let database = super::database_path();
        let store = SqliteStore::open(database).map_err(|error| error.to_string())?;
        let definition = store
            .experiment(experiment)
            .map_err(|error| error.to_string())?;
        let versions: std::collections::BTreeSet<_> = definition
            .benchmarks
            .iter()
            .filter(|benchmark| benchmark.kind == mcm::KIND)
            .map(|benchmark| benchmark.version)
            .collect();
        if versions.len() > 1 {
            return Err("Experiment contains multiple MCM procedure versions.".into());
        }
        if versions.is_empty() {
            return Err("The selected experiment has no MCM measurements.".into());
        }
        let version = *versions.first().unwrap_or(&mcm::VERSION);
        let query = Query::new(HistoryQuery {
            experiment,
            benchmark_kind: mcm::KIND.into(),
            benchmark_version: version,
            benchmark_parameters: None,
        });
        let frame = store
            .query::<McmOutput>(&query)
            .map_err(|error| error.to_string())?;

        Analysis::new(move |frame: Frame<McmOutput>| project(frame, metric, projection)).run(frame)
    }
}

#[derive(Clone, Copy)]
enum Projection {
    Aggregate,
    EachWorkload,
}

fn project(
    frame: Frame<McmOutput>,
    metric: McmMetric,
    projection: Projection,
) -> Result<AnalysisResult, String> {
    let history = History {
        commits: frame.commits,
        benchmarks: frame.benchmarks,
        records: frame.runs,
    };
    let selection = McmSelection { metric };
    let view = McmView;
    let (title, series_label, unit, reference, reference_label, description) = match metric {
        McmMetric::RuntimeRatio => (
            "Runtime ratio",
            "MCM runtime ratio (Yosys / paper)",
            "ratio",
            1.0,
            "Equal runtime",
            "Yosys runtime divided by the paper reference. Lower is better.",
        ),
        McmMetric::AreaSavings => (
            "Area savings",
            "MCM area savings",
            "%",
            0.0,
            "Equal area",
            "Area saved against synthesis without MCM. Higher is better.",
        ),
    };

    let series = match projection {
        Projection::Aggregate => view
            .project(&history, &selection)
            .map_err(|error| error.to_string())?
            .series
            .into_iter()
            .map(|series| {
                to_chart_series("aggregate".into(), series_label.into(), series.points)
            })
            .collect(),
        Projection::EachWorkload => {
            let mut series = Vec::new();
            for (index, definition) in benchmarks_for(&history) {
                let workload_history = History {
                    commits: history.commits.clone(),
                    benchmarks: [(index, definition.clone())].into(),
                    records: history
                        .records
                        .iter()
                        .filter(|entry| entry.record.benchmark_index == index)
                        .cloned()
                        .collect(),
                };
                let projected = view
                    .project(&workload_history, &selection)
                    .map_err(|error| error.to_string())?;
                let projected = projected
                    .series
                    .into_iter()
                    .next()
                    .ok_or_else(|| "MCM projection returned no series".to_string())?;
                let label = definition
                    .parameters
                    .get("coefficients")
                    .map(|coefficients| format!("Input {index}: {coefficients}"))
                    .unwrap_or_else(|| format!("Input {index}"));
                series.push(to_chart_series(
                    format!("input-{index}"),
                    label,
                    projected.points,
                ));
            }
            series
        }
    };

    Ok(AnalysisResult {
        title: format!("{title} history"),
        description: format!("{description} Unit: {unit}."),
        chart: ChartSpec {
            x_label: "Commit order".into(),
            y_label: format!("{title} ({unit})"),
            reference_lines: vec![ReferenceLine {
                value: reference,
                label: reference_label.into(),
            }],
            series,
        },
        tables: Vec::new(),
    })
}

fn benchmarks_for(
    history: &History<McmOutput>,
) -> Vec<(usize, benchmarks::store::BenchmarkDefinition)> {
    history
        .benchmarks
        .iter()
        .map(|(&index, definition)| (index, definition.clone()))
        .collect()
}

fn to_chart_series(id: String, label: String, points: Vec<benchmarks::view::Point>) -> ChartSeries {
    ChartSeries {
        id,
        label,
        points: points
            .into_iter()
            .map(|point| {
                let mut details = vec![
                    DetailField {
                        label: "Commit".into(),
                        value: point.commit.0.clone(),
                    },
                    DetailField {
                        label: "Coverage".into(),
                        value: format!(
                            "{} / {} inputs",
                            point.coverage.included, point.coverage.total
                        ),
                    },
                ];
                details.extend(point.coverage.exclusions.into_iter().map(|exclusion| {
                    DetailField {
                        label: format!("Input {} excluded", exclusion.benchmark_index),
                        value: exclusion.reason,
                    }
                }));
                ChartPoint {
                    x: point.commit.0,
                    y: point.value,
                    details,
                }
            })
            .collect(),
    }
}
