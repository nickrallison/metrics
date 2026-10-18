//! Typed sample extraction and capabilities for MCM analyses.

use crate::{
    analysis::*,
    benchmarks::{mcm::{self, McmOutput}, synth::SynthStats},
    store::{ExperimentId, SqliteStore},
};

pub struct McmPerformance;

impl AnalysisProvider for McmPerformance {
    fn descriptor(&self) -> AnalysisDescriptor {
        AnalysisDescriptor {
            id: "mcm.performance".into(), label: "Performance history".into(),
            description: "Compare measurements across the experiment's frozen commit order.".into(),
            metrics: vec![
                metric("runtime_ratio", "Runtime ratio", "ratio", MetricFormula::Ratio,
                    "Runtime divided by the paper reference. Lower is better.", Some(ReferenceLine { value: 1.0, label: "Equal runtime".into() })),
                metric("runtime", "Runtime", "s", MetricFormula::Identity, "Yosys MCM wall-clock time.", None),
                metric("paper_runtime", "Reference runtime", "s", MetricFormula::Identity, "Paper reference wall-clock time.", None),
                metric("area_savings", "Area savings", "%", MetricFormula::PercentSavings,
                    "Area saved against synthesis without MCM. Higher is better.", Some(ReferenceLine { value: 0.0, label: "Equal area".into() })),
                metric("area_with", "Area with MCM", "area", MetricFormula::Identity, "Synthesis area with MCM enabled.", None),
                metric("area_without", "Baseline area", "area", MetricFormula::Identity, "Synthesis area without MCM.", None),
                metric("adds_subs", "Add/sub operations", "operations", MetricFormula::Identity, "Add and subtract operations after MCM.", None),
                metric("multipliers", "Remaining multipliers", "operations", MetricFormula::Identity, "Multipliers remaining after MCM.", None),
                metric("depth", "Depth", "levels", MetricFormula::Identity, "Reported MCM depth; absent when unavailable.", None),
            ],
            benchmarks: vec![], commits: vec![], filter_fields: vec![], parameters: vec![],
        }
    }

    fn supports(&self, types: &[(String, u32)]) -> bool {
        types.iter().any(|(kind, version)| kind == mcm::KIND && [1, mcm::VERSION].contains(version))
    }

    fn run(&self, store: &SqliteStore, experiment: ExperimentId, plan: &AnalysisPlan) -> Result<AnalysisResult, String> {
        let descriptor = self.descriptor();
        let metric = descriptor.metrics.iter().find(|m| m.id == plan.metric).ok_or("Unknown metric")?;
        let frame = load_frame::<McmOutput>(store, experiment, mcm::KIND, &[1, mcm::VERSION], plan)?;
        analyze_frame(frame, metric, plan, samples)
    }
}

fn metric(id: &str, label: &str, unit: &str, formula: MetricFormula, description: &str, reference: Option<ReferenceLine>) -> MetricDescriptor {
    let mut benchmark_reductions = vec![Reduction::Mean, Reduction::Median, Reduction::Min, Reduction::Max,
        Reduction::StdDev, Reduction::P90, Reduction::P95, Reduction::Count];
    if formula == MetricFormula::Identity { benchmark_reductions.push(Reduction::Sum); }
    if formula == MetricFormula::Ratio { benchmark_reductions.insert(0, Reduction::RatioOfMeans); }
    MetricDescriptor {
        id: id.into(), label: label.into(), description: description.into(), unit: unit.into(), formula,
        repetition_reductions: vec![Reduction::Mean, Reduction::Median, Reduction::Min, Reduction::Max, Reduction::P90, Reduction::P95],
        benchmark_reductions, default_repetitions: Reduction::Mean,
        default_benchmarks: if formula == MetricFormula::Ratio { Reduction::RatioOfMeans } else { Reduction::Mean }, reference,
    }
}

fn areas(samples: &[SynthStats]) -> Result<Vec<f64>, String> {
    samples.iter().map(|sample| sample.area.filter(|area| area.is_finite() && *area >= 0.0)
        .ok_or_else(|| "Missing or invalid synthesis area".into())).collect()
}

fn samples(output: &McmOutput, metric: &str) -> Result<MetricSamples, String> {
    let runtime = || output.yosys_mcm.values.iter().map(|v| v.wall_clock_time.as_secs_f64()).collect();
    let reference_runtime = || output.paper.value.values.iter().map(|v| v.wall_clock_time.as_secs_f64()).collect();
    Ok(match metric {
        "runtime" => MetricSamples::values(runtime()),
        "paper_runtime" => MetricSamples::values(reference_runtime()),
        "runtime_ratio" => MetricSamples::compared(runtime(), reference_runtime()),
        "area_with" => MetricSamples::values(areas(&output.synth_with_mcm.values)?),
        "area_without" => MetricSamples::values(areas(&output.synth_without_mcm.values)?),
        "area_savings" => MetricSamples::compared(areas(&output.synth_with_mcm.values)?, areas(&output.synth_without_mcm.values)?),
        "adds_subs" => MetricSamples::values(output.yosys_mcm.values.iter().map(|v| v.adds_subs as f64).collect()),
        "multipliers" => MetricSamples::values(output.yosys_mcm.values.iter().map(|v| v.multipliers as f64).collect()),
        "depth" => MetricSamples::values(output.yosys_mcm.values.iter().map(|v| v.depth.map(|v| v as f64)
            .ok_or_else(|| "Depth is unavailable".to_string())).collect::<Result<_, _>>()?),
        _ => return Err("Unknown metric".into()),
    })
}
