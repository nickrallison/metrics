//! Registry and reusable analysis stages for typed benchmark outputs.

use std::collections::BTreeMap;

pub use benchmark_analysis_types::*;

use crate::store::{BenchmarkDefinition, ExperimentDefinition, ExperimentId, SqliteStore};

mod pipeline;
pub use pipeline::{MetricSamples, analyze_frame, load_frame, reduce};

pub trait AnalysisProvider: Send + Sync {
    fn descriptor(&self) -> AnalysisDescriptor;
    fn supports(&self, benchmark_types: &[(String, u32)]) -> bool;
    fn run(&self, store: &SqliteStore, experiment: ExperimentId, plan: &AnalysisPlan)
    -> Result<AnalysisResult, String>;
}

#[derive(Default)]
pub struct AnalysisRegistry {
    providers: BTreeMap<String, Box<dyn AnalysisProvider>>,
}

impl AnalysisRegistry {
    pub fn register(&mut self, provider: impl AnalysisProvider + 'static) {
        self.providers.insert(provider.descriptor().id, Box::new(provider));
    }

    pub fn descriptors_for(&self, definition: &ExperimentDefinition) -> Vec<AnalysisDescriptor> {
        let types = benchmark_types(definition);
        self.providers.values().filter(|provider| provider.supports(&types)).map(|provider| {
            describe(provider.as_ref(), definition)
        }).collect()
    }

    pub fn run(&self, id: &str, store: &SqliteStore, experiment: ExperimentId,
        definition: &ExperimentDefinition, plan: &AnalysisPlan) -> Result<AnalysisResult, String>
    {
        let provider = self.providers.get(id).ok_or_else(|| format!("Unknown analysis: {id}"))?;
        if !provider.supports(&benchmark_types(definition)) {
            return Err(format!("Analysis {id:?} does not support this experiment"));
        }
        validate_plan(&describe(provider.as_ref(), definition), plan)?;
        provider.run(store, experiment, plan)
    }
}

fn benchmark_types(definition: &ExperimentDefinition) -> Vec<(String, u32)> {
    definition.benchmarks.iter().map(|b| (b.kind.clone(), b.version)).collect()
}

fn describe(provider: &dyn AnalysisProvider, definition: &ExperimentDefinition) -> AnalysisDescriptor {
    let mut descriptor = provider.descriptor();
    let benchmarks: Vec<_> = definition.benchmarks.iter().enumerate().filter(|(_, b)| {
        provider.supports(&[(b.kind.clone(), b.version)])
    }).collect();
    descriptor.benchmarks = benchmarks.iter().map(|(index, b)| BenchmarkOption {
        id: *index,
        label: format!("Benchmark {index}"),
        description: b.parameters.to_string(),
    }).collect();
    descriptor.commits = definition.commits.iter().map(|commit| CommitOption {
        id: commit.id.0.clone(),
        label: format!("{} {}", commit.id.0.chars().take(10).collect::<String>(),
            commit.commit_message.lines().next().unwrap_or_default()),
    }).collect();
    descriptor.filter_fields = vec![
        FilterField { id: "outcome".into(), label: "Outcome".into(), kind: FieldKind::Text,
            choices: vec!["succeeded".into(), "failed".into(), "unsupported".into(), "missing".into()] },
        FilterField { id: "value".into(), label: "Measurement".into(), kind: FieldKind::Number, choices: vec![] },
    ];
    let mut fields = BTreeMap::<String, Option<FieldKind>>::new();
    for (_, benchmark) in benchmarks {
        parameter_fields(&benchmark.parameters, "", &mut fields);
    }
    descriptor.filter_fields.extend(fields.into_iter().filter_map(|(path, kind)| kind.map(|kind| FilterField {
        id: format!("parameter:{path}"), label: path.trim_start_matches('/').replace('/', "."),
        kind, choices: vec![],
    })));
    descriptor
}

fn parameter_fields(value: &serde_json::Value, path: &str, fields: &mut BTreeMap<String, Option<FieldKind>>) {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            parameter_fields(value, &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")), fields);
        }
        return;
    }
    let kind = match value {
        serde_json::Value::Number(_) => Some(FieldKind::Number),
        serde_json::Value::String(_) => Some(FieldKind::Text),
        serde_json::Value::Bool(_) => Some(FieldKind::Boolean),
        _ => return,
    };
    fields.entry(path.into()).and_modify(|existing| { if *existing != kind { *existing = None; } }).or_insert(kind);
}

pub(crate) fn parameter<'a>(benchmark: &'a BenchmarkDefinition, field: &str) -> Option<&'a serde_json::Value> {
    field.strip_prefix("parameter:").and_then(|pointer| benchmark.parameters.pointer(pointer))
}

fn validate_plan(descriptor: &AnalysisDescriptor, plan: &AnalysisPlan) -> Result<(), String> {
    let metric = descriptor.metrics.iter().find(|m| m.id == plan.metric).ok_or("Unknown metric")?;
    if !metric.repetition_reductions.contains(&plan.repetitions) || !metric.benchmark_reductions.contains(&plan.benchmarks) {
        return Err("The selected reduction is not supported by this metric".into());
    }
    if let Some(indices) = &plan.selection.benchmarks {
        if indices.iter().any(|index| !descriptor.benchmarks.iter().any(|b| b.id == *index)) {
            return Err("A selected benchmark is not supported by this analysis".into());
        }
    }
    if let SeriesGrouping::Parameter(field) = &plan.series {
        if !field.starts_with("parameter:") || !descriptor.filter_fields.iter().any(|f| &f.id == field) {
            return Err("Unknown grouping parameter".into());
        }
    }
    for filter in &plan.filters {
        let field = descriptor.filter_fields.iter().find(|f| f.id == filter.field).ok_or("Unknown filter field")?;
        if !FilterOperator::for_kind(field.kind).contains(&filter.operator) {
            return Err(format!("Unsupported operator for {}", field.label));
        }
        if matches!(filter.operator, FilterOperator::Present | FilterOperator::Missing) { continue; }
        let valid = match field.kind {
            FieldKind::Number => filter.value.as_f64().is_some_and(f64::is_finite),
            FieldKind::Text => filter.value.is_string(),
            FieldKind::Boolean => filter.value.is_boolean(),
        };
        if !valid { return Err(format!("Invalid filter value for {}", field.label)); }
    }
    Ok(())
}
