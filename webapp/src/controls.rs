//! Capability-driven analysis controls. No benchmark-specific fields live here.

use dioxus::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::presentation::*;

fn encoded(value: &impl Serialize) -> String { serde_json::to_string(value).unwrap_or_default() }
fn decoded<T: DeserializeOwned>(value: &str) -> Option<T> { serde_json::from_str(value).ok() }

#[derive(Clone)]
enum PlanEdit {
    Metric(String), Repetitions(Reduction), Benchmarks(Reduction), Series(SeriesGrouping),
    Selection(Option<Vec<usize>>), Commits(CommitSelection), Filters(Vec<FilterClause>),
    Parameter(String, Value),
}

#[component]
pub fn AnalysisSidebar(descriptor: AnalysisDescriptor, plan: AnalysisPlan, plot: PlotOptions,
    on_plan_change: EventHandler<AnalysisPlan>, on_plot_change: EventHandler<PlotOptions>) -> Element
{
    let metric = descriptor.metrics.iter().find(|m| m.id == plan.metric);
    let edit_descriptor = descriptor.clone();
    let edit_plan = plan.clone();
    let on_edit = use_callback(move |edit: PlanEdit| {
        let mut next = edit_plan.clone();
        match edit {
            PlanEdit::Metric(id) => if let Some(metric) = edit_descriptor.metrics.iter().find(|m| m.id == id) { next.select_metric(metric); },
            PlanEdit::Repetitions(value) => next.repetitions = value,
            PlanEdit::Benchmarks(value) => next.benchmarks = value,
            PlanEdit::Series(value) => next.series = value,
            PlanEdit::Selection(value) => next.selection.benchmarks = value,
            PlanEdit::Commits(value) => next.selection.commits = value,
            PlanEdit::Filters(value) => next.filters = value,
            PlanEdit::Parameter(id, value) => { next.parameters.insert(id, value); }
        }
        on_plan_change.call(next);
    });
    let mut grouping_options = vec![(SeriesGrouping::Combined, "Combined".to_string()), (SeriesGrouping::Benchmark, "By benchmark".into())];
    grouping_options.extend(descriptor.filter_fields.iter().filter(|f| f.id.starts_with("parameter:")).map(|f| (SeriesGrouping::Parameter(f.id.clone()), format!("By {}", f.label))));
    rsx! {
        aside { class: "analysis-sidebar", id: "analysis-controls", aria_label: "Analysis editor",
            section { class: "editor-section", id: "metric-controls",
                label { class: "field", "Metric"
                    select { value: plan.metric.clone(), onchange: move |e| on_edit.call(PlanEdit::Metric(e.value())),
                        for metric in &descriptor.metrics { option { value: metric.id.clone(), "{metric.label}" } }
                    }
                }
                if let Some(metric) = metric { p { class: "muted", "{metric.description}" } }
            }
            section { class: "editor-section", id: "data-controls",
                h2 { "Data" }
                BenchmarkPicker { options: descriptor.benchmarks.clone(), selected: plan.selection.benchmarks.clone(),
                    on_change: move |value| on_edit.call(PlanEdit::Selection(value)) }
                CommitPicker { options: descriptor.commits.clone(), selected: plan.selection.commits.clone(),
                    on_change: move |value| on_edit.call(PlanEdit::Commits(value)) }
                FilterList { fields: descriptor.filter_fields.clone(), filters: plan.filters.clone(),
                    on_change: move |value| on_edit.call(PlanEdit::Filters(value)) }
            }
            if let Some(metric) = metric {
                section { class: "editor-section", id: "summary-controls",
                    h2 { "Summarize" }
                    ReductionControl { label: "Repetitions", value: plan.repetitions, options: metric.repetition_reductions.clone(),
                        on_change: move |v| on_edit.call(PlanEdit::Repetitions(v)) }
                    ReductionControl { label: "Benchmarks", value: plan.benchmarks, options: metric.benchmark_reductions.clone(),
                        disabled: matches!(plan.series, SeriesGrouping::Benchmark),
                        on_change: move |v| on_edit.call(PlanEdit::Benchmarks(v)) }
                    label { class: "field", "Series"
                        select { value: encoded(&plan.series), onchange: move |e| {
                            if let Some(value) = decoded(&e.value()) { on_edit.call(PlanEdit::Series(value)); }
                        },
                            for (value, label) in grouping_options { option { value: encoded(&value), "{label}" } }
                        }
                    }
                    p { class: "muted", "First reduce repetitions within each benchmark, then combine benchmarks at each commit." }
                    if plan.benchmarks == Reduction::RatioOfMeans && !matches!(plan.series, SeriesGrouping::Benchmark) {
                        p { class: "muted", "Ratio of means reduces both phases across the same benchmarks before dividing." }
                    }
                }
            }
            PlotControls { options: plot, on_change: on_plot_change }
            if !descriptor.parameters.is_empty() {
                details { class: "editor-section", summary { "Analysis options" }
                    for parameter in &descriptor.parameters {
                        CustomParameter { parameter: parameter.clone(), value: plan.parameters.get(&parameter.id).cloned(),
                            on_change: move |(id, value)| on_edit.call(PlanEdit::Parameter(id, value)) }
                    }
                }
            }
        }
    }
}

#[component]
fn ReductionControl(label: String, value: Reduction, options: Vec<Reduction>, #[props(default)] disabled: bool,
    on_change: EventHandler<Reduction>) -> Element
{
    rsx! { label { class: "field", "{label}"
        select { value: encoded(&value), disabled, onchange: move |e| {
            if let Some(value) = decoded(&e.value()) { on_change.call(value); }
        }, for option in options { option { value: encoded(&option), "{option.label()}" } } }
    } }
}

#[component]
fn BenchmarkPicker(options: Vec<BenchmarkOption>, selected: Option<Vec<usize>>,
    on_change: EventHandler<Option<Vec<usize>>>) -> Element
{
    let mut search = use_signal(String::new);
    let query = search().to_lowercase();
    let all: Vec<_> = options.iter().map(|b| b.id).collect();
    let active = selected.clone().unwrap_or_else(|| all.clone());
    let active_for_toggle = active.clone();
    let toggle = use_callback(move |id: usize| {
        let mut next = active_for_toggle.clone();
        if next.contains(&id) { next.retain(|v| *v != id); } else { next.push(id); }
        next.sort_unstable();
        on_change.call(if next.len() == all.len() { None } else { Some(next) });
    });
    let count = active.len();
    let total = options.len();
    rsx! {
        div { class: "field", span { "Benchmarks" }
            details { class: "picker",
                summary { if selected.is_none() { "All {total} benchmarks" } else { "{count} of {total} benchmarks" } }
                div { class: "picker-body",
                    input { r#type: "search", placeholder: "Search benchmarks…", aria_label: "Search benchmarks", value: search(),
                        oninput: move |e| search.set(e.value()) }
                    div { class: "picker-actions",
                        button { r#type: "button", onclick: move |_| on_change.call(None), "All" }
                        button { r#type: "button", onclick: move |_| on_change.call(Some(vec![])), "None" }
                    }
                    div { class: "picker-list",
                        for option in options.iter().filter(|b| format!("{} {}", b.label, b.description).to_lowercase().contains(&query)) {
                            label { class: "check-row", title: option.description.clone(),
                                input { r#type: "checkbox", checked: active.contains(&option.id), onchange: {
                                    let id = option.id; move |_| toggle.call(id)
                                } }
                                span { "{option.label}" small { "{option.description}" } }
                            }
                        }
                    }
                }
            }
            if let Some(indices) = selected {
                div { class: "chips",
                    for index in indices {
                        button { r#type: "button", class: "chip", aria_label: "Remove benchmark {index}",
                            onclick: move |_| toggle.call(index), "{index} ×" }
                    }
                }
            }
        }
    }
}

#[component]
fn CommitPicker(options: Vec<CommitOption>, selected: CommitSelection, on_change: EventHandler<CommitSelection>) -> Element {
    let mode = match selected { CommitSelection::All => "all", CommitSelection::Range { .. } => "range", CommitSelection::Only { .. } => "only" };
    let (start, end) = match &selected { CommitSelection::Range { start, end } => (start.clone(), end.clone()), _ => (None, None) };
    let end_for_start = end.clone();
    let start_for_end = start.clone();
    let selected_ids = match &selected { CommitSelection::Only { ids } => ids.clone(), _ => vec![] };
    let toggle = use_callback(move |id: String| {
        let mut ids = selected_ids.clone();
        if ids.contains(&id) { ids.retain(|v| v != &id); } else { ids.push(id); }
        on_change.call(CommitSelection::Only { ids });
    });
    rsx! {
        label { class: "field", "Commits"
            select { value: mode, onchange: move |e| on_change.call(match e.value().as_str() {
                "range" => CommitSelection::Range { start: None, end: None },
                "only" => CommitSelection::Only { ids: vec![] }, _ => CommitSelection::All,
            }),
                option { value: "all", "All commits" }
                option { value: "range", "Commit range" }
                option { value: "only", "Individual commits" }
            }
        }
        if mode == "range" {
            label { class: "field", "From"
                select { value: start.clone().unwrap_or_default(), onchange: move |e| {
                    on_change.call(CommitSelection::Range { start: nonempty(e.value()), end: end_for_start.clone() });
                }, option { value: "", "First commit" }
                    for option in &options { option { value: option.id.clone(), "{option.label}" } }
                }
            }
            label { class: "field", "Through"
                select { value: end.unwrap_or_default(), onchange: move |e| {
                    on_change.call(CommitSelection::Range { start: start_for_end.clone(), end: nonempty(e.value()) });
                }, option { value: "", "Last commit" }
                    for option in &options { option { value: option.id.clone(), "{option.label}" } }
                }
            }
        }
        if let CommitSelection::Only { ids } = selected {
            details { class: "picker", summary { "{ids.len()} selected" }
                div { class: "picker-list",
                    for option in &options {
                        label { class: "check-row",
                            input { r#type: "checkbox", checked: ids.contains(&option.id), onchange: {
                                let id = option.id.clone(); move |_| toggle.call(id.clone())
                            } }
                            span { "{option.label}" }
                        }
                    }
                }
            }
        }
    }
}

fn nonempty(value: String) -> Option<String> { (!value.is_empty()).then_some(value) }

#[component]
fn FilterList(fields: Vec<FilterField>, filters: Vec<FilterClause>, on_change: EventHandler<Vec<FilterClause>>) -> Element {
    let edit_filters = filters.clone();
    let edit = use_callback(move |(index, clause): (usize, Option<FilterClause>)| {
        let mut next = edit_filters.clone();
        if let Some(clause) = clause { next[index] = clause; } else { next.remove(index); }
        on_change.call(next);
    });
    let added_filters = filters.clone();
    let first_field = fields.first().cloned();
    rsx! {
        div { class: "filters", id: "filter-controls",
            for (index, clause) in filters.iter().enumerate() {
                FilterRow { clause: clause.clone(), fields: fields.clone(),
                    on_change: move |value| edit.call((index, Some(value))),
                    on_remove: move |_| edit.call((index, None)) }
            }
            button { r#type: "button", class: "text-button", disabled: fields.is_empty(), onclick: move |_| {
                if let Some(field) = &first_field {
                    let mut next = added_filters.clone(); next.push(field.clause()); on_change.call(next);
                }
            }, "+ Add filter" }
        }
    }
}

#[component]
fn FilterRow(clause: FilterClause, fields: Vec<FilterField>, on_change: EventHandler<FilterClause>, on_remove: EventHandler<()>) -> Element {
    let field = fields.iter().find(|f| f.id == clause.field);
    let operators = field.map(|f| FilterOperator::for_kind(f.kind)).unwrap_or_default();
    let value_clause = clause.clone();
    let operator_clause = clause.clone();
    let on_value = use_callback(move |value: Value| { let mut next = value_clause.clone(); next.value = value; on_change.call(next); });
    let field_options = fields.clone();
    let text = clause.value.as_str().map(str::to_owned).unwrap_or_else(|| clause.value.to_string());
    rsx! {
        div { class: "filter-row",
            div { class: "filter-heading",
                select { aria_label: "Filter field", value: clause.field.clone(), onchange: move |e| {
                    if let Some(field) = field_options.iter().find(|f| f.id == e.value()) { on_change.call(field.clause()); }
                }, for field in &fields { option { value: field.id.clone(), "{field.label}" } } }
                button { r#type: "button", class: "remove-button", aria_label: "Remove filter", onclick: move |_| on_remove.call(()), "×" }
            }
            select { aria_label: "Filter operator", value: encoded(&clause.operator), onchange: move |e| {
                if let Some(operator) = decoded(&e.value()) { let mut next = operator_clause.clone(); next.operator = operator; on_change.call(next); }
            }, for operator in operators { option { value: encoded(&operator), "{operator.label()}" } } }
            if !matches!(clause.operator, FilterOperator::Present | FilterOperator::Missing) {
                if let Some(field) = field {
                    if !field.choices.is_empty() {
                        select { aria_label: "Filter value", value: text.clone(), onchange: move |e| on_value.call(Value::String(e.value())),
                            for choice in &field.choices { option { value: choice.clone(), "{choice}" } }
                        }
                    } else {
                        match field.kind {
                            FieldKind::Number => rsx! { input { r#type: "number", step: "any", aria_label: "Filter value", value: text.clone(),
                                onchange: move |e| { if let Ok(value) = e.value().parse::<f64>() { if value.is_finite() { on_value.call(Value::from(value)); } } }
                            } },
                            FieldKind::Boolean => rsx! { select { aria_label: "Filter value", value: text.clone(),
                                onchange: move |e| on_value.call(Value::Bool(e.value() == "true")),
                                option { value: "true", "True" } option { value: "false", "False" }
                            } },
                            FieldKind::Text => rsx! { input { r#type: "text", aria_label: "Filter value", value: text.clone(),
                                onchange: move |e| on_value.call(Value::String(e.value()))
                            } },
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum PlotEdit { Scale(AxisScale), Min(Option<f64>), Max(Option<f64>), References(bool) }

#[component]
fn PlotControls(options: PlotOptions, on_change: EventHandler<PlotOptions>) -> Element {
    let edit_options = options.clone();
    let edit = use_callback(move |edit: PlotEdit| {
        let mut next = edit_options.clone();
        match edit { PlotEdit::Scale(value) => next.y_scale = value, PlotEdit::Min(value) => next.y_min = value,
            PlotEdit::Max(value) => next.y_max = value, PlotEdit::References(value) => next.show_reference_lines = value }
        on_change.call(next);
    });
    rsx! {
        section { class: "editor-section", id: "plot-controls", h2 { "Plot" }
            label { class: "field", "Y scale"
                select { value: encoded(&options.y_scale), onchange: move |e| { if let Some(value) = decoded(&e.value()) { edit.call(PlotEdit::Scale(value)); } },
                    option { value: encoded(&AxisScale::Linear), "Linear" }
                    option { value: encoded(&AxisScale::Log), "Logarithmic" }
                }
            }
            details { class: "more-options", summary { "More options" }
                label { class: "field", "Y minimum"
                    input { r#type: "number", step: "any", placeholder: "Auto", value: options.y_min.map(|v| v.to_string()).unwrap_or_default(),
                        onchange: move |e| edit.call(PlotEdit::Min(e.value().parse::<f64>().ok().filter(|v| v.is_finite()))) }
                }
                label { class: "field", "Y maximum"
                    input { r#type: "number", step: "any", placeholder: "Auto", value: options.y_max.map(|v| v.to_string()).unwrap_or_default(),
                        onchange: move |e| edit.call(PlotEdit::Max(e.value().parse::<f64>().ok().filter(|v| v.is_finite()))) }
                }
                label { class: "check-row",
                    input { r#type: "checkbox", checked: options.show_reference_lines, onchange: move |e| edit.call(PlotEdit::References(e.checked())) }
                    "Show reference lines"
                }
            }
        }
    }
}

#[component]
fn CustomParameter(parameter: ParameterDescriptor, value: Option<Value>, on_change: EventHandler<(String, Value)>) -> Element {
    let id = parameter.id.clone();
    rsx! { label { class: "field", "{parameter.label}"
        match parameter.kind {
            ParameterKind::Select => rsx! { select { value: value.as_ref().and_then(Value::as_str).unwrap_or_default(),
                onchange: move |e| on_change.call((id.clone(), Value::String(e.value()))),
                for option in parameter.options { option { value: option.value, "{option.label}" } }
            } },
            ParameterKind::MultiSelect => rsx! { div {
                for option in parameter.options {
                    label { class: "check-row",
                        input { r#type: "checkbox", checked: value.as_ref().and_then(Value::as_array).is_some_and(|a| a.contains(&Value::String(option.value.clone()))),
                            onchange: {
                                let id = id.clone(); let value = value.clone(); let option_value = Value::String(option.value.clone());
                                move |_| { let mut values = value.as_ref().and_then(Value::as_array).cloned().unwrap_or_default();
                                    if values.contains(&option_value) { values.retain(|v| v != &option_value); } else { values.push(option_value.clone()); }
                                    on_change.call((id.clone(), Value::Array(values)));
                                }
                            }
                        }
                        "{option.label}"
                    }
                }
            } },
            ParameterKind::Text => rsx! { input { value: value.as_ref().and_then(Value::as_str).unwrap_or_default(),
                onchange: move |e| on_change.call((id.clone(), Value::String(e.value())))
            } },
        }
    } }
}
