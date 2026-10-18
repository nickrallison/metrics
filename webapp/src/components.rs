//! Generic Dioxus controls and the isolated Plotly chart adapter.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::presentation::{
    AnalysisResult, Catalog, ChartPoint, ChartSpec, ParameterDescriptor, ParameterKind,
};

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
    analysis_id: String,
    parameters: serde_json::Map<String, serde_json::Value>,
    result: Option<AnalysisResult>,
    error: Option<String>,
    loading: bool,
    on_experiment_change: EventHandler<String>,
    on_analysis_change: EventHandler<String>,
    on_parameter_change: EventHandler<(String, String)>,
) -> Element {
    let active_analysis = catalog
        .analyses
        .iter()
        .find(|analysis| analysis.id == analysis_id);
    let mut selected_point = use_signal(|| None::<ChartPoint>);
    let chart_key = result
        .as_ref()
        .and_then(|result| serde_json::to_string(&result.chart).ok())
        .unwrap_or_default();
    let event_chart = result.as_ref().map(|result| result.chart.clone());
    let mut rendered_chart_key = use_signal(String::new);
    let effect_chart_key = chart_key.clone();
    use_effect(move || {
        if rendered_chart_key() != effect_chart_key {
            rendered_chart_key.set(effect_chart_key.clone());
            selected_point.set(None);
        }
    });

    rsx! {
        style { dangerous_inner_html: THEME }
        main { class: "workspace",
            header { class: "page-heading",
                h1 { "Results" }
                p { class: "muted", "Choose an analysis and its parameters." }
            }
            section { class: "controls", aria_label: "Analysis controls",
                label { class: "field",
                    "Experiment"
                    select {
                        value: if catalog.experiments.is_empty() { "".to_string() } else { "0".to_string() },
                        onchange: move |event| {
                            if let Ok(index) = event.value().parse::<usize>() {
                                if let Some(experiment) = catalog.experiments.get(index) {
                                    on_experiment_change.call(experiment.id.clone());
                                }
                            }
                        },
                        for (index, experiment) in catalog.experiments.iter().enumerate() {
                            option { value: index.to_string(), "{experiment.label}" }
                        }
                    }
                }
                label { class: "field",
                    "Analysis"
                    select {
                        value: analysis_id.clone(),
                        onchange: move |event| on_analysis_change.call(event.value()),
                        for analysis in &catalog.analyses {
                            option { value: analysis.id.clone(), "{analysis.label}" }
                        }
                    }
                }
                if let Some(analysis) = active_analysis {
                    for parameter in &analysis.parameters {
                        ParameterControl {
                            key: "{analysis.id}:{parameter.id}",
                            parameter: parameter.clone(),
                            value: parameters.get(&parameter.id)
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_owned)
                                .or_else(|| parameter.default.clone())
                                .unwrap_or_default(),
                            on_change: on_parameter_change,
                        }
                    }
                }
            }
            if loading {
                p { class: "muted", "Loading analysis…" }
            }
            if let Some(error) = error.as_ref() {
                p { class: "error", role: "alert", "{error}" }
            }
            if let Some(result) = result {
                section { class: "chart-card",
                    header { class: "chart-heading",
                        h2 { "{result.title}" }
                        p { class: "muted", "{result.description}" }
                    }
                    PlotlyChart {
                        chart: result.chart.clone(),
                        on_point: move |selection: ChartPointSelection| {
                            let point = event_chart.as_ref().and_then(|chart| {
                                chart
                                    .series
                                    .iter()
                                    .find(|series| series.id == selection.series_id)
                                    .and_then(|series| series.points.get(selection.point_index))
                            }).cloned();
                            selected_point.set(point);
                        },
                    }
                    if let Some(point) = selected_point() {
                        section { class: "details-card", aria_label: "Selected point details",
                            h3 { "Selected point" }
                            p { class: "muted", "{point.x}" }
                            dl {
                                dt { "Value" }
                                dd { "{point.y.map(|value| value.to_string()).unwrap_or_else(|| \"No measurement\".into())}" }
                                for detail in point.details {
                                    dt { "{detail.label}" }
                                    dd { "{detail.value}" }
                                }
                            }
                        }
                    }
                    for table in result.tables {
                        section { class: "result-table",
                            h3 { "{table.title}" }
                            table {
                                thead { tr { for column in table.columns { th { "{column}" } } } }
                                tbody {
                                    for row in table.rows {
                                        tr { for cell in row { td { "{cell}" } } }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if !loading && error.is_none() {
                section { class: "empty-state",
                    if catalog.analyses.is_empty() {
                        p { "No analyses are available for this experiment." }
                    } else {
                        p { "Select an analysis to view results." }
                    }
                }
            }
        }
    }
}

#[component]
fn ParameterControl(
    parameter: ParameterDescriptor,
    value: String,
    on_change: EventHandler<(String, String)>,
) -> Element {
    let id = parameter.id.clone();
    rsx! {
        label { class: "field",
            "{parameter.label}"
            match parameter.kind {
                ParameterKind::Select | ParameterKind::MultiSelect => rsx! {
                    select {
                        multiple: matches!(parameter.kind, ParameterKind::MultiSelect),
                        value,
                        onchange: move |event| on_change.call((id.clone(), event.value())),
                        for option in parameter.options {
                            option { value: option.value, "{option.label}" }
                        }
                    }
                },
                ParameterKind::Text => rsx! {
                    input {
                        value,
                        oninput: move |event| on_change.call((id.clone(), event.value())),
                    }
                },
            }
        }
    }
}

#[component]
fn PlotlyChart(chart: ChartSpec, on_point: EventHandler<ChartPointSelection>) -> Element {
    let mut eval_handle = use_signal(|| None);
    let mut last_sent = use_signal(|| None::<String>);
    let mut point_handler = use_signal(|| on_point);
    let mut chart = use_reactive!(|(chart,)| chart.clone());
    use_effect(move || {
        let chart = chart();
        let chart_key = serde_json::to_string(&chart).unwrap_or_default();
        point_handler.set(on_point);
        if last_sent().as_deref() == Some(chart_key.as_str()) {
            return;
        }

        let eval = match eval_handle() {
            Some(eval) => eval,
            None => {
                let eval = document::eval(PLOTLY_SCRIPT);
                let mut receiver = eval;
                let point_handler = point_handler;
                spawn(async move {
                    while let Ok(selection) = receiver.recv::<ChartPointSelection>().await {
                        point_handler().call(selection);
                    }
                });
                eval_handle.set(Some(eval));
                eval
            }
        };
        if eval.send(chart).is_ok() {
            last_sent.set(Some(chart_key.clone()));
        }
    });

    rsx! {
        div {
            id: "analysis-plot",
            class: "plot-host",
            role: "img",
            aria_label: "Interactive analysis chart. Drag to pan, use the toolbar to zoom.",
        }
    }
}
