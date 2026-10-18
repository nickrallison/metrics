//! Generic Dioxus workspace and the isolated Plotly adapter.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{controls::AnalysisSidebar, presentation::*};

const THEME: &str = include_str!("../../ui/theme.css");
const PLOTLY_SCRIPT: &str = concat!(
    "/*\n",
    include_str!("../../ui/vendor/plotly.LICENSE"),
    "\n*/\n",
    include_str!("../../ui/vendor/plotly-basic.min.js"),
    "\n",
    include_str!("../assets/plotly_bridge.js")
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ChartPointSelection {
    series_id: String,
    point_index: usize,
}

#[component]
pub fn AnalysisWorkspace(
    catalog: Catalog<String>,
    selected_experiment: String,
    analysis_id: String,
    plan: Option<AnalysisPlan>,
    plot: PlotOptions,
    result: Option<AnalysisResult>,
    error: Option<String>,
    loading: bool,
    on_experiment_change: EventHandler<String>,
    on_analysis_change: EventHandler<String>,
    on_plan_change: EventHandler<AnalysisPlan>,
    on_plot_change: EventHandler<PlotOptions>,
) -> Element {
    let active = catalog
        .experiments
        .iter()
        .find(|e| e.id == selected_experiment);
    let analyses = active.map(|e| e.analyses.as_slice()).unwrap_or_default();
    let descriptor = analyses.iter().find(|a| a.id == analysis_id);
    rsx! {
        style { dangerous_inner_html: THEME }
        main { class: "workspace",
            header { class: "page-heading",
                h1 { "Results" }
                div { class: "controls",
                    label { class: "field", "Experiment"
                        select { value: selected_experiment,
                            onchange: move |e| on_experiment_change.call(e.value()),
                            for experiment in &catalog.experiments {
                                option { value: experiment.id.clone(), "{experiment.label}" }
                            }
                        }
                    }
                    label { class: "field", "Analysis"
                        select { value: analysis_id, onchange: move |e| on_analysis_change.call(e.value()),
                            for analysis in analyses {
                                option { value: analysis.id.clone(), "{analysis.label}" }
                            }
                        }
                    }
                }
            }
            if let (Some(descriptor), Some(plan)) = (descriptor, plan) {
                div { class: "analysis-layout",
                    AnalysisSidebar { descriptor: descriptor.clone(), plan: plan.clone(), plot: plot.clone(),
                        on_plan_change, on_plot_change }
                    section { class: "analysis-output", aria_label: "Analysis results", aria_busy: loading,
                        Recipe { descriptor: descriptor.clone(), plan, plot: plot.clone() }
                        if let Some(error) = error {
                            p { class: "error", role: "alert", "{error}" }
                        }
                        ChartPanel { result, plot, loading }
                    }
                }
            } else {
                section { class: "empty-state",
                    if catalog.experiments.is_empty() { "No experiments are recorded yet." }
                    else { "No analyses are available for this experiment." }
                }
            }
        }
    }
}

#[component]
fn Recipe(descriptor: AnalysisDescriptor, plan: AnalysisPlan, plot: PlotOptions) -> Element {
    let metric = descriptor
        .metrics
        .iter()
        .find(|m| m.id == plan.metric)
        .map(|m| m.label.as_str())
        .unwrap_or("Metric");
    let benchmarks = match &plan.selection.benchmarks {
        None => format!("All {} benchmarks", descriptor.benchmarks.len()),
        Some(ids) => format!("{} benchmarks", ids.len()),
    };
    let commit_label = |id: &Option<String>, fallback: &str| {
        id.as_deref()
            .map(|id| id.chars().take(8).collect::<String>())
            .unwrap_or_else(|| fallback.into())
    };
    let commits = match &plan.selection.commits {
        CommitSelection::All => "All commits".into(),
        CommitSelection::Only { ids } => format!("{} commits", ids.len()),
        CommitSelection::Range { start, end } => format!(
            "{}–{}",
            commit_label(start, "First"),
            commit_label(end, "Last")
        ),
    };
    let grouping = match &plan.series {
        SeriesGrouping::Combined => format!("{} benchmarks", plan.benchmarks.label()),
        SeriesGrouping::Benchmark => "Separate benchmarks".into(),
        SeriesGrouping::Parameter(field) => format!(
            "{} by {}",
            plan.benchmarks.label(),
            descriptor
                .filter_fields
                .iter()
                .find(|f| &f.id == field)
                .map(|f| f.label.as_str())
                .unwrap_or(field)
        ),
    };
    rsx! {
        nav { class: "recipe", aria_label: "Current analysis recipe",
            a { href: "#metric-controls", "{metric}" }
            span { "·" }
            a { href: "#data-controls", "{benchmarks} · {commits}" }
            span { "·" }
            a { href: "#summary-controls", "{plan.repetitions.label()} repetitions → {grouping}" }
            span { "·" }
            a { href: "#plot-controls", if plot.y_scale == AxisScale::Log { "Log Y" } else { "Linear Y" } }
            if !plan.filters.is_empty() {
                a { href: "#filter-controls", "· {plan.filters.len()} filters" }
            }
        }
    }
}

#[component]
fn ChartPanel(result: Option<AnalysisResult>, plot: PlotOptions, loading: bool) -> Element {
    let mut selected = use_signal(|| None::<ChartPoint>);
    let chart = result.as_ref().map(|r| r.chart.clone());
    let reset_chart = chart.clone();
    let mut changed_chart = use_reactive!(|(reset_chart,)| reset_chart.clone());
    use_effect(move || {
        let _ = changed_chart();
        selected.set(None);
    });
    let event_result = result.clone();
    let on_point = use_callback(move |selection: ChartPointSelection| {
        let point = event_result
            .as_ref()
            .and_then(|r| r.chart.series.iter().find(|s| s.id == selection.series_id))
            .and_then(|s| s.points.get(selection.point_index))
            .cloned();
        selected.set(point);
    });
    let nonpositive = chart
        .as_ref()
        .map(|c| {
            c.series
                .iter()
                .flat_map(|s| &s.points)
                .filter(|p| p.y.is_some_and(|v| v <= 0.0))
                .count()
        })
        .unwrap_or_default();
    let current_point = selected();
    rsx! {
        section { class: "chart-card",
            if let Some(result) = &result {
                header { class: "chart-heading",
                    h2 { "{result.title}" }
                    p { class: "muted", "{result.description}" }
                    details { class: "coverage-summary",
                        summary { "{result.coverage.included}/{result.coverage.selected} benchmark measurements included" }
                        p { class: "muted", "Coverage counts benchmark–commit pairs. Click a point for its benchmark exclusions." }
                        for series in &result.chart.series {
                            for point in series.points.iter().filter(|p| !p.coverage.exclusions.is_empty()) {
                                details { class: "coverage-entry",
                                    summary { "{series.label} · {point.x.chars().take(10).collect::<String>()}: {point.coverage.included}/{point.coverage.selected}" }
                                    ul { for exclusion in &point.coverage.exclusions {
                                        li { "Benchmark {exclusion.benchmark}: {exclusion.reason}" }
                                    } }
                                }
                            }
                        }
                    }
                }
            } else {
                p { class: "chart-placeholder", role: "status", if loading { "Loading analysis…" } else { "No analysis result." } }
            }
            if let Some(error) = plot.bounds_error() {
                p { class: "notice", role: "status", "Bounds not applied: {error}" }
            }
            if plot.y_scale == AxisScale::Log && nonpositive > 0 {
                p { class: "notice", role: "status", "{nonpositive} zero or negative values cannot be plotted on a log axis. They remain in the analysis; choose Linear to view them." }
            }
            PlotlyChart { chart, options: plot, on_point }
            if let Some(point) = current_point {
                section { class: "details-card", aria_label: "Selected point details",
                    h3 { "Selected point" }
                    dl {
                        dt { "Value" }
                        dd { {point.y.map(|v| v.to_string()).unwrap_or_else(|| "No measurement".into())} }
                        dt { "Benchmarks included" }
                        dd { "{point.coverage.included}/{point.coverage.selected}" }
                        for detail in point.details { dt { "{detail.label}" } dd { "{detail.value}" } }
                        for exclusion in point.coverage.exclusions {
                            dt { "Benchmark {exclusion.benchmark} excluded" } dd { "{exclusion.reason}" }
                        }
                    }
                }
            }
            if let Some(result) = &result {
                for table in &result.tables {
                    section { class: "result-table", h3 { "{table.title}" }
                        table {
                            thead { tr { for column in &table.columns { th { "{column}" } } } }
                            tbody { for row in &table.rows { tr { for cell in row { td { "{cell}" } } } } }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Serialize)]
struct PlotSpec {
    chart: Option<ChartSpec>,
    options: PlotOptions,
}

#[component]
fn PlotlyChart(
    chart: Option<ChartSpec>,
    options: PlotOptions,
    on_point: EventHandler<ChartPointSelection>,
) -> Element {
    let mut eval_handle = use_signal(|| None::<document::Eval>);
    let mut last_sent = use_signal(|| String::new());
    let mut handler = use_signal(|| on_point);
    let hidden = chart.is_none();
    let mut desired =
        use_reactive!(|(chart, options, on_point)| (chart.clone(), options.clone(), on_point));
    use_effect(move || {
        let (chart, mut options, callback) = desired();
        handler.set(callback);
        if options.bounds_error().is_some() {
            options.y_min = None;
            options.y_max = None;
        }
        let spec = PlotSpec { chart, options };
        let key = serde_json::to_string(&spec).unwrap_or_default();
        if *last_sent.peek() == key {
            return;
        }
        let existing = *eval_handle.peek();
        let eval = match existing {
            Some(eval) => eval,
            None => {
                let eval = document::eval(PLOTLY_SCRIPT);
                let mut receiver = eval;
                spawn(async move {
                    while let Ok(selection) = receiver.recv::<ChartPointSelection>().await {
                        handler().call(selection);
                    }
                });
                eval_handle.set(Some(eval));
                eval
            }
        };
        if eval.send(Some(spec)).is_ok() {
            last_sent.set(key);
        }
    });
    use_drop(move || {
        if let Some(eval) = *eval_handle.peek() {
            let _ = eval.send(Option::<PlotSpec>::None);
        }
    });
    rsx! { div { id: "analysis-plot", class: "plot-host", hidden, role: "img",
    aria_label: "Interactive analysis chart. Drag to pan, use the toolbar to zoom." } }
}
