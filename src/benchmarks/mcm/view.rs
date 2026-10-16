//! Statistics for comparisons over Yosys history.

use std::collections::{HashMap, HashSet};

use super::{KIND, McmOutput, VERSION};
use crate::{
    Error, Result,
    benchmarks::synth::SynthStats,
    store::{History, Outcome, RunPurpose},
    view::{Coverage, ExcludedWorkload, Point, Series, View, ViewData},
};

#[derive(Debug, Clone, Copy)]
pub enum McmMetric {
    RuntimeRatio,
    AreaSavings,
}

#[derive(Debug, Clone)]
pub struct McmSelection {
    pub metric: McmMetric,
}

/// First summarize repeated observations for each workload. On the same matched
/// workload set, runtime ratio is mean(Yosys time) / mean(paper time), matching
/// the reference script; it is not mean(per-workload time ratios). Area savings
/// is mean(100 * (1 - mean(with_mcm_area) / mean(baseline_area))).
///
/// Empty samples, incomplete/nonfinite/negative areas, and zero denominators
/// exclude the workload from that metric. Points report every excluded workload;
/// no valid workloads gives an absent value. Revision order is frozen history
/// order. Workloads have equal weight regardless of their repetition counts.
/// No repetitions or cached references are pooled across workloads or commits.
///
/// Quality ordering is lexicographic: fewer remaining multipliers first, then
/// fewer add/sub operations. Equal values mean equal quality under that rule;
/// depth and negations are separate observations. Quality, depth, distributions,
/// and standard deviations are future projections, not these scalar metrics.
pub struct McmView;

impl View for McmView {
    type Output = McmOutput;
    type Selection = McmSelection;

    fn project(&self, history: &History<McmOutput>, selection: &McmSelection) -> Result<ViewData> {
        if history
            .benchmarks
            .values()
            .any(|benchmark| benchmark.kind != KIND || benchmark.version != VERSION)
        {
            return Err(Error::InvalidView(
                "MCM history must use the supported MCM kind/version".into(),
            ));
        }
        let commits: HashSet<_> = history
            .commits
            .iter()
            .map(|commit| commit.id.0.as_str())
            .collect();
        if commits.len() != history.commits.len() {
            return Err(Error::InvalidView(
                "duplicate commits in frozen history".into(),
            ));
        }
        let mut records = HashMap::new();
        for entry in &history.records {
            let record = &entry.record;
            if !commits.contains(record.commit.id.0.as_str())
                || history.benchmarks.get(&record.benchmark_index) != Some(&record.benchmark)
                || !matches!(record.purpose, RunPurpose::Measurement)
            {
                return Err(Error::InvalidView(
                    "record does not match the selected measurement history".into(),
                ));
            }
            if records
                .insert(
                    (record.commit.id.0.as_str(), record.benchmark_index),
                    &record.outcome,
                )
                .is_some()
            {
                return Err(Error::InvalidView(
                    "history must select one attempt per workload and commit".into(),
                ));
            }
        }
        let mut points = Vec::new();
        for commit in &history.commits {
            let mut pairs = Vec::new();
            let mut exclusions = Vec::new();
            for &benchmark_index in history.benchmarks.keys() {
                let pair = match records.get(&(commit.id.0.as_str(), benchmark_index)) {
                    Some(Outcome::Succeeded(output)) => summarize(output, selection.metric),
                    Some(Outcome::Failed(reason)) => Err(format!("failed: {reason}")),
                    Some(Outcome::Unsupported(reason)) => Err(format!("unsupported: {reason}")),
                    None => Err("no finished measurement".into()),
                };
                match pair {
                    Ok(pair) => pairs.push(pair),
                    Err(reason) => exclusions.push(ExcludedWorkload {
                        benchmark_index,
                        reason,
                    }),
                }
            }
            let value = match selection.metric {
                McmMetric::RuntimeRatio => mean(pairs.iter().map(|pair| pair.0))
                    .zip(mean(pairs.iter().map(|pair| pair.1)))
                    .map(|(yosys, paper)| yosys / paper),
                McmMetric::AreaSavings => mean(
                    pairs
                        .iter()
                        .map(|&(with, baseline)| 100.0 * (1.0 - with / baseline)),
                ),
            };
            points.push(Point {
                commit: commit.id.clone(),
                value,
                coverage: Coverage {
                    included: pairs.len(),
                    total: history.benchmarks.len(),
                    exclusions,
                },
            });
        }
        let (label, unit) = match selection.metric {
            McmMetric::RuntimeRatio => ("MCM runtime ratio (Yosys / paper)", "ratio"),
            McmMetric::AreaSavings => ("MCM area savings", "%"),
        };
        Ok(ViewData {
            series: vec![Series {
                label: label.into(),
                unit: unit.into(),
                points,
            }],
        })
    }
}

fn summarize(output: &McmOutput, metric: McmMetric) -> std::result::Result<(f64, f64), String> {
    let pair = match metric {
        McmMetric::RuntimeRatio => (
            mean(
                output
                    .yosys_mcm
                    .values
                    .iter()
                    .map(|sample| sample.wall_clock_time.as_secs_f64()),
            )
            .ok_or("empty Yosys timing samples")?,
            mean(
                output
                    .paper
                    .value
                    .values
                    .iter()
                    .map(|sample| sample.wall_clock_time.as_secs_f64()),
            )
            .ok_or("empty paper timing samples")?,
        ),
        McmMetric::AreaSavings => (
            area_mean(&output.synth_with_mcm.values, "with MCM")?,
            area_mean(&output.synth_without_mcm.values, "baseline")?,
        ),
    };
    if pair.1 == 0.0 {
        return Err(match metric {
            McmMetric::RuntimeRatio => "zero paper runtime",
            McmMetric::AreaSavings => "zero baseline area",
        }
        .into());
    }
    if matches!(metric, McmMetric::AreaSavings) && !(100.0 * (1.0 - pair.0 / pair.1)).is_finite() {
        return Err("area savings is not finite".into());
    }
    Ok(pair)
}

fn area_mean(samples: &[SynthStats], phase: &str) -> std::result::Result<f64, String> {
    let mut areas = Vec::new();
    for sample in samples {
        let area = sample.area.ok_or_else(|| format!("missing {phase} area"))?;
        if !area.is_finite() || area < 0.0 {
            return Err(format!("invalid {phase} area"));
        }
        areas.push(area);
    }
    mean(areas.into_iter()).ok_or_else(|| format!("empty {phase} area samples"))
}

// Incremental means avoid overflowing a sum of large, otherwise valid areas.
fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut result = None;
    for (index, value) in values.enumerate() {
        result = Some(match result {
            None => value,
            Some(previous) => previous + (value - previous) / (index + 1) as f64,
        });
    }
    result
}

#[cfg(test)]
mod tests;
