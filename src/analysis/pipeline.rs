use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::*;
use crate::{history::CommitId, store::{Analysis, Frame, HistoryQuery, Outcome, Query, Store}};

/// Samples from a typed output. Compound metrics reduce each phase separately
/// before composing a ratio or percentage; phase repetitions are never paired.
#[derive(Debug, Clone)]
pub struct MetricSamples {
    pub values: Vec<f64>,
    pub reference: Option<Vec<f64>>,
}

impl MetricSamples {
    pub fn values(values: Vec<f64>) -> Self { Self { values, reference: None } }
    pub fn compared(values: Vec<f64>, reference: Vec<f64>) -> Self { Self { values, reference: Some(reference) } }
}

/// Push dimension selections into Query. Parameter filters select workloads;
/// outcome and measurement filters are evaluated by the analysis stage.
pub fn load_frame<O: DeserializeOwned>(store: &SqliteStore, experiment: ExperimentId,
    kind: &str, versions: &[u32], plan: &AnalysisPlan) -> Result<Frame<O>, String>
{
    let definition = store.experiment(experiment).map_err(|e| e.to_string())?;
    let allowed: Vec<_> = definition.benchmarks.iter().enumerate()
        .filter(|(_, b)| b.kind == kind && versions.contains(&b.version)).collect();
    let selected = plan.selection.benchmarks.clone().unwrap_or_else(|| allowed.iter().map(|(i, _)| *i).collect());
    if selected.iter().enumerate().any(|(i, id)| selected[..i].contains(id)) {
        return Err("Duplicate benchmark selection".into());
    }
    if selected.iter().any(|id| !allowed.iter().any(|(index, _)| index == id)) {
        return Err("A selected benchmark is not supported by this analysis".into());
    }
    let selected: Vec<_> = selected.into_iter().filter(|id| {
        plan.filters.iter().filter(|f| f.field.starts_with("parameter:")).all(|f| {
            matches_filter(parameter(&definition.benchmarks[*id], &f.field), f)
        })
    }).collect();
    let selected_versions: std::collections::BTreeSet<_> = selected.iter().map(|i| definition.benchmarks[*i].version).collect();
    if selected_versions.len() > 1 { return Err("Select benchmarks from one procedure version at a time".into()); }
    let version = selected_versions.first().copied().or_else(|| allowed.first().map(|(_, b)| b.version))
        .ok_or("No supported benchmarks are registered in this experiment")?;
    let mut query = Query::new(HistoryQuery { experiment, benchmark_kind: kind.into(), benchmark_version: version, benchmark_parameters: None })
        .benchmarks(selected);
    match &plan.selection.commits {
        CommitSelection::All => (),
        CommitSelection::Only { ids } => { query = query.commits(ids.iter().cloned().map(CommitId).collect()); },
        CommitSelection::Range { start, end } => {
            let position = |id: &str| definition.commits.iter().position(|c| c.id.0 == id)
                .ok_or_else(|| format!("Unknown commit {id}"));
            let start = start.as_deref().map(position).transpose()?.unwrap_or(0);
            let end = end.as_deref().map(position).transpose()?.unwrap_or_else(|| definition.commits.len().saturating_sub(1));
            if start > end { return Err("The commit range must follow recorded history order".into()); }
            query = query.commits(definition.commits.iter().skip(start).take(end - start + 1).map(|c| c.id.clone()).collect());
        }
    }
    store.query(&query).map_err(|e| e.to_string())
}

/// Reusable stages over an arbitrary typed Frame. Benchmark modules only extract
/// samples and declare capabilities; they need no frontend projection code.
pub fn analyze_frame<O>(frame: Frame<O>, metric: &MetricDescriptor, plan: &AnalysisPlan,
    extract: impl Fn(&O, &str) -> Result<MetricSamples, String>) -> Result<AnalysisResult, String>
{
    Analysis::new(move |frame| project(frame, metric, plan, extract)).run(frame)
}

#[derive(Clone)]
struct Measurement {
    primary: f64,
    reference: Option<f64>,
    value: f64,
}

#[derive(Clone)]
struct Evaluated {
    measurement: Result<Measurement, String>,
    filtered: Option<String>,
}

fn project<O>(frame: Frame<O>, metric: &MetricDescriptor, plan: &AnalysisPlan,
    extract: impl Fn(&O, &str) -> Result<MetricSamples, String>) -> Result<AnalysisResult, String>
{
    if !metric.repetition_reductions.contains(&plan.repetitions) || !metric.benchmark_reductions.contains(&plan.benchmarks) {
        return Err("Unsupported reduction for this metric".into());
    }
    let rows = frame.map_runs(|entry| {
        let (outcome, measurement) = match &entry.record.outcome {
            Outcome::Succeeded(output) => ("succeeded", extract(output, &metric.id).and_then(|samples| summarize(samples, metric.formula, plan.repetitions))),
            Outcome::Failed(reason) => ("failed", Err(format!("Failed: {reason}"))),
            Outcome::Unsupported(reason) => ("unsupported", Err(format!("Unsupported: {reason}"))),
        };
        let value = measurement.as_ref().ok().map(|m| Value::from(m.value));
        let status = Value::String(outcome.into());
        let filtered = plan.filters.iter().find(|filter| match filter.field.as_str() {
            "value" => !matches_filter(value.as_ref(), filter),
            "outcome" => !matches_filter(Some(&status), filter),
            _ => false,
        }).map(|f| format!("Excluded by {} filter", f.field));
        ((entry.record.commit.id.0.clone(), entry.record.benchmark_index), Evaluated { measurement, filtered })
    });
    let count = rows.len();
    let rows: BTreeMap<_, _> = rows.into_iter().collect();
    if rows.len() != count { return Err("The analysis requires one selected attempt per benchmark and commit".into()); }
    // Retain filter explanations separately so filtering never turns exclusions
    // into unexplained missing values.
    let frame = frame.filter(|entry| rows[&(entry.record.commit.id.0.clone(), entry.record.benchmark_index)].filtered.is_none());
    let grouped = frame.group_reduce(
        |entry| entry.record.commit.id.0.clone(),
        |entry| (entry.record.benchmark_index, rows[&(entry.record.commit.id.0.clone(), entry.record.benchmark_index)].clone()),
        |values| values.into_iter().collect::<BTreeMap<_, _>>(),
    );
    let mut groups = BTreeMap::<String, (String, Vec<usize>)>::new();
    for (&index, benchmark) in &frame.benchmarks {
        let (key, label) = match &plan.series {
            SeriesGrouping::Combined => ("combined".into(), metric.label.clone()),
            SeriesGrouping::Benchmark => (format!("benchmark:{index}"), format!("Benchmark {index}")),
            SeriesGrouping::Parameter(field) => {
                let value = parameter(benchmark, field).map(display_value).unwrap_or_else(|| "Missing".into());
                (format!("{field}:{value}"), format!("{}: {value}", field.trim_start_matches("parameter:").trim_start_matches('/').replace('/', ".")))
            }
        };
        groups.entry(key).or_insert_with(|| (label, Vec::new())).1.push(index);
    }
    let mut coverage = CoverageSummary::default();
    let series = groups.into_iter().map(|(id, (label, indices))| {
        let points = frame.commits.iter().map(|commit| {
            let mut values = Vec::new();
            let mut point_coverage = PointCoverage { selected: indices.len(), ..Default::default() };
            for &index in &indices {
                let key = (commit.id.0.clone(), index);
                let result = if let Some(reason) = rows.get(&key).and_then(|row| row.filtered.as_ref()) {
                    Err(reason.clone())
                } else if let Some(row) = grouped.get(&commit.id.0).and_then(|rows| rows.get(&index)) {
                    row.measurement.clone()
                } else {
                    Err("No finished measurement".into())
                };
                match result {
                    Ok(value) => values.push(value),
                    Err(reason) => point_coverage.exclusions.push(Exclusion { benchmark: index, reason }),
                }
            }
            point_coverage.included = values.len();
            coverage.selected += indices.len();
            coverage.included += values.len();
            let value = if matches!(plan.series, SeriesGrouping::Benchmark) {
                values.first().map(|m| m.value)
            } else if plan.benchmarks == Reduction::RatioOfMeans {
                reduce(Reduction::Mean, &values.iter().map(|m| m.primary).collect::<Vec<_>>())
                    .zip(reduce(Reduction::Mean, &values.iter().filter_map(|m| m.reference).collect::<Vec<_>>()))
                    .and_then(|(a, b)| (b != 0.0).then_some(a / b)).filter(|v| v.is_finite())
            } else {
                reduce(plan.benchmarks, &values.iter().map(|m| m.value).collect::<Vec<_>>())
            };
            let mut details = vec![DetailField { label: "Commit".into(), value: commit.id.0.clone() },
                DetailField { label: "Message".into(), value: commit.commit_message.lines().next().unwrap_or_default().into() }];
            if value.is_none() && !values.is_empty() {
                details.push(DetailField { label: "Reduction".into(), value: "The reduction produced no finite value".into() });
            }
            ChartPoint { x: commit.id.0.clone(), y: value, coverage: point_coverage, details }
        }).collect();
        ChartSeries { id, label, points }
    }).collect();
    let across_benchmarks = !matches!(plan.series, SeriesGrouping::Benchmark);
    let unit = if across_benchmarks && plan.benchmarks == Reduction::Count { "benchmarks" } else { &metric.unit };
    let reference_lines = if across_benchmarks && matches!(plan.benchmarks, Reduction::Sum | Reduction::Count | Reduction::StdDev) {
        vec![]
    } else { metric.reference.clone().into_iter().collect() };
    Ok(AnalysisResult {
        title: format!("{} history", metric.label), description: metric.description.clone(),
        chart: ChartSpec { x_label: "Commit order".into(), y_label: format!("{} ({unit})", metric.label), reference_lines, series },
        coverage, tables: vec![],
    })
}

fn summarize(samples: MetricSamples, formula: MetricFormula, reduction: Reduction) -> Result<Measurement, String> {
    let primary = reduce(reduction, &samples.values).ok_or("Empty or invalid measurement samples")?;
    let reference = samples.reference.map(|values| reduce(reduction, &values).ok_or("Empty or invalid reference samples")).transpose()?;
    let value = match formula {
        MetricFormula::Identity => primary,
        MetricFormula::Ratio | MetricFormula::PercentSavings => {
            let reference = reference.filter(|v| *v != 0.0).ok_or("Missing or zero reference")?;
            if formula == MetricFormula::Ratio { primary / reference } else { 100.0 * (1.0 - primary / reference) }
        }
    };
    if !value.is_finite() { return Err("Derived measurement is not finite".into()); }
    Ok(Measurement { primary, reference, value })
}

/// Population standard deviation and linearly interpolated percentiles. Invalid
/// or empty samples remain absent; no reducer silently drops invalid samples.
pub fn reduce(reduction: Reduction, values: &[f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite()) { return None; }
    let scale = values.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
    let mean = || if scale == 0.0 { 0.0 } else { values.iter().map(|v| v / scale).sum::<f64>() / values.len() as f64 * scale };
    let value = match reduction {
        Reduction::Mean => mean(),
        Reduction::Min => values.iter().copied().reduce(f64::min)?,
        Reduction::Max => values.iter().copied().reduce(f64::max)?,
        Reduction::Sum => values.iter().sum(),
        Reduction::Count => values.len() as f64,
        Reduction::StdDev => {
            if scale == 0.0 { 0.0 } else {
                let mean = mean() / scale;
                (values.iter().map(|v| (v / scale - mean).powi(2)).sum::<f64>() / values.len() as f64).sqrt() * scale
            }
        }
        Reduction::Median | Reduction::P90 | Reduction::P95 => {
            let mut sorted = values.to_vec(); sorted.sort_by(f64::total_cmp);
            let percentile = match reduction { Reduction::Median => 0.5, Reduction::P90 => 0.9, _ => 0.95 };
            let position = (sorted.len() - 1) as f64 * percentile;
            let (low, high) = (position.floor() as usize, position.ceil() as usize);
            let fraction = position.fract();
            sorted[low] * (1.0 - fraction) + sorted[high] * fraction
        }
        Reduction::RatioOfMeans => return None,
    };
    value.is_finite().then_some(value)
}

fn matches_filter(value: Option<&Value>, filter: &FilterClause) -> bool {
    let value = value.filter(|v| !v.is_null());
    match filter.operator {
        FilterOperator::Present => value.is_some(), FilterOperator::Missing => value.is_none(),
        FilterOperator::Equal => value.is_some_and(|v| match (v.as_f64(), filter.value.as_f64()) {
            (Some(a), Some(b)) => a == b, _ => v == &filter.value,
        }),
        FilterOperator::NotEqual => value.is_some_and(|v| match (v.as_f64(), filter.value.as_f64()) {
            (Some(a), Some(b)) => a != b, _ => v != &filter.value,
        }),
        FilterOperator::Contains => value.and_then(Value::as_str).zip(filter.value.as_str()).is_some_and(|(v, query)| v.contains(query)),
        operator => value.and_then(Value::as_f64).zip(filter.value.as_f64()).is_some_and(|(a, b)| match operator {
            FilterOperator::Greater => a > b, FilterOperator::AtLeast => a >= b,
            FilterOperator::Less => a < b, FilterOperator::AtMost => a <= b, _ => false,
        }),
    }
}

fn display_value(value: &Value) -> String {
    value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{history::Commit, store::{HistoryEntry, RunPurpose, RunRecord}};
    use std::time::UNIX_EPOCH;
    use serde_json::json;

    fn definition() -> ExperimentDefinition {
        ExperimentDefinition {
            configuration: json!({}),
            benchmarks: [8, 16].into_iter().map(|width| BenchmarkDefinition {
                kind: "arbitrary-output".into(), version: 1, parameters: json!({"width": width}),
            }).collect(),
            commits: ["z", "a"].into_iter().map(|id| Commit {
                id: CommitId(id.into()), committed_at: UNIX_EPOCH, author: "Test".into(), commit_message: id.into(),
            }).collect(),
        }
    }

    fn fixture() -> (Frame<Vec<f64>>, MetricDescriptor, AnalysisPlan) {
        let definition = definition();
        let runs = [(0, 0, Outcome::Succeeded(vec![1.0, 2.0, 100.0])),
            (0, 1, Outcome::Succeeded(vec![10.0, 20.0])),
            (1, 0, Outcome::Failed("tool failed".into()))].into_iter().enumerate().map(|(id, (commit, index, outcome))| {
                HistoryEntry { id: id as i64, record: RunRecord {
                    experiment: 1, commit: definition.commits[commit].clone(), benchmark_index: index,
                    benchmark: definition.benchmarks[index].clone(), started_at: UNIX_EPOCH, finished_at: UNIX_EPOCH,
                    purpose: RunPurpose::Measurement, artifacts: None, outcome,
                } }
            }).collect();
        let metric = MetricDescriptor {
            id: "measurement".into(), label: "Measurement".into(), description: String::new(), unit: "s".into(),
            formula: MetricFormula::Identity,
            repetition_reductions: vec![Reduction::Mean, Reduction::Median],
            benchmark_reductions: vec![Reduction::Mean, Reduction::Min, Reduction::Max],
            default_repetitions: Reduction::Median, default_benchmarks: Reduction::Mean, reference: None,
        };
        let plan = AnalysisPlan { metric: metric.id.clone(), selection: Selection::default(), repetitions: Reduction::Median,
            benchmarks: Reduction::Mean, series: SeriesGrouping::Combined, filters: vec![], parameters: Default::default() };
        (Frame { experiment: 1, commits: definition.commits, benchmarks: definition.benchmarks.into_iter().enumerate().collect(), runs }, metric, plan)
    }

    fn extract(samples: &Vec<f64>, _: &str) -> Result<MetricSamples, String> { Ok(MetricSamples::values(samples.clone())) }

    #[test]
    fn reductions_respect_repetition_then_benchmark_axes_and_frozen_order() {
        let (frame, metric, plan) = fixture();
        let result = analyze_frame(frame, &metric, &plan, extract).unwrap();
        let points = &result.chart.series[0].points;
        assert_eq!(points[0].x, "z");
        assert_eq!(points[0].y, Some(8.5)); // mean(median(1,2,100), median(10,20))
        assert_eq!(points[0].coverage.included, 2);
        assert_eq!(points[1].x, "a");
        assert_eq!(points[1].y, None);
        assert_eq!(points[1].coverage.exclusions.len(), 2);
        assert!(points[1].coverage.exclusions.iter().any(|e| e.reason.contains("tool failed")));
        assert_eq!(result.coverage, CoverageSummary { included: 2, selected: 4 });
    }

    #[test]
    fn measurement_filters_keep_exclusion_reasons_and_grouping_keeps_per_benchmark_values() {
        let (frame, metric, mut plan) = fixture();
        plan.filters.push(FilterClause { field: "value".into(), operator: FilterOperator::Less, value: json!(10) });
        let result = analyze_frame(frame.clone(), &metric, &plan, extract).unwrap();
        let point = &result.chart.series[0].points[0];
        assert_eq!(point.y, Some(2.0));
        assert_eq!(point.coverage.included, 1);
        assert!(point.coverage.exclusions[0].reason.contains("filter"));
        plan.filters.clear();
        plan.series = SeriesGrouping::Benchmark;
        let result = analyze_frame(frame.clone(), &metric, &plan, extract).unwrap();
        assert_eq!(result.chart.series.len(), 2);
        assert_eq!(result.chart.series[1].points[0].y, Some(15.0));
        plan.series = SeriesGrouping::Parameter("parameter:/width".into());
        let result = analyze_frame(frame, &metric, &plan, extract).unwrap();
        assert_eq!(result.chart.series.len(), 2);
        assert!(result.chart.series.iter().any(|s| s.label == "width: 16"));
    }

    #[test]
    fn compound_metrics_distinguish_ratio_of_means_from_mean_of_ratios() {
        let (frame, mut metric, mut plan) = fixture();
        metric.formula = MetricFormula::Ratio;
        metric.benchmark_reductions.push(Reduction::RatioOfMeans);
        plan.repetitions = Reduction::Median;
        plan.benchmarks = Reduction::RatioOfMeans;
        let extract = |values: &Vec<f64>, _: &str| Ok(MetricSamples::compared(values.clone(), vec![if values.len() == 3 { 1.0 } else { 10.0 }]));
        let result = analyze_frame(frame.clone(), &metric, &plan, extract).unwrap();
        assert!((result.chart.series[0].points[0].y.unwrap() - 17.0 / 11.0).abs() < 1e-12);
        plan.benchmarks = Reduction::Mean;
        let result = analyze_frame(frame, &metric, &plan, extract).unwrap();
        assert_eq!(result.chart.series[0].points[0].y, Some(1.75));
    }

    #[test]
    fn query_applies_parameter_filters_and_commit_ranges_without_reordering() {
        let mut store = SqliteStore::open(":memory:").unwrap();
        let definition = definition();
        let experiment = store.register_experiment(&definition).unwrap();
        let (_, _, mut plan) = fixture();
        plan.selection.commits = CommitSelection::Range { start: Some("a".into()), end: None };
        plan.filters.push(FilterClause { field: "parameter:/width".into(), operator: FilterOperator::AtLeast, value: json!(16) });
        let frame = load_frame::<Vec<f64>>(&store, experiment, "arbitrary-output", &[1], &plan).unwrap();
        assert_eq!(frame.benchmarks.keys().copied().collect::<Vec<_>>(), vec![1]);
        assert_eq!(frame.commits.iter().map(|c| c.id.0.as_str()).collect::<Vec<_>>(), vec!["a"]);
        plan.selection.commits = CommitSelection::Range { start: Some("a".into()), end: Some("z".into()) };
        assert!(load_frame::<Vec<f64>>(&store, experiment, "arbitrary-output", &[1], &plan).is_err());
        plan.selection.commits = CommitSelection::All;
        plan.selection.benchmarks = Some(vec![]);
        assert!(load_frame::<Vec<f64>>(&store, experiment, "arbitrary-output", &[1], &plan).unwrap().benchmarks.is_empty());
    }

    #[test]
    fn numeric_reducers_handle_extremes_percentiles_and_invalid_samples() {
        assert_eq!(reduce(Reduction::Mean, &[f64::MAX, f64::MAX]), Some(f64::MAX));
        assert_eq!(reduce(Reduction::Median, &[f64::MAX, f64::MAX]), Some(f64::MAX));
        assert_eq!(reduce(Reduction::Mean, &[-f64::MAX, f64::MAX]), Some(0.0));
        assert_eq!(reduce(Reduction::P90, &[0.0, 10.0]), Some(9.0));
        assert_eq!(reduce(Reduction::StdDev, &[2.0, 4.0]), Some(1.0));
        assert_eq!(reduce(Reduction::Mean, &[]), None);
        assert_eq!(reduce(Reduction::Mean, &[1.0, f64::NAN]), None);
        assert_eq!(reduce(Reduction::Sum, &[f64::MAX, f64::MAX]), None);
    }
}
