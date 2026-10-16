//! Reference-script tables derived from stored observations, without rerunning tools.

use std::{cmp::Ordering, collections::HashMap, fmt};

use serde::{Deserialize, Serialize};

use super::{
    McmMetric, McmOutput, McmSelection, McmView, PaperStats, YosysStats,
    view::{area_mean, mean, summarize},
};
use crate::{
    Result,
    history::CommitId,
    store::{History, Outcome, ReferenceId},
    view::{Coverage, View},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmReport {
    pub commits: Vec<McmCommitReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmCommitReport {
    pub commit: CommitId,
    pub workloads: Vec<McmWorkloadReport>,
    pub summary: McmSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmWorkloadReport {
    pub benchmark_index: usize,
    pub coefficients: Option<Vec<i64>>,
    /// None means no finished measurement, rather than an unsuccessful process.
    pub outcome: Option<Outcome<McmWorkloadSummary>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Better,
    Same,
    Worse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingSummary {
    pub samples: usize,
    pub mean_ms: Option<f64>,
    /// Sample deviation, n-1. None for fewer than two observations.
    pub sample_stdev_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSummary {
    pub timing: TimingSummary,
    pub adds_subs: f64,
    pub multipliers: f64,
    pub negations: f64,
    pub shifts: Option<f64>,
    pub depth: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmWorkloadSummary {
    pub yosys: Option<ToolSummary>,
    pub paper: Option<ToolSummary>,
    pub paper_reference: ReferenceId,
    pub baseline_area: Option<f64>,
    pub with_mcm_area: Option<f64>,
    pub area_ratio: Option<f64>,
    pub runtime_ratio: Option<f64>,
    pub quality: Option<Quality>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComparisonCounts {
    pub better: usize,
    pub same: usize,
    pub worse: usize,
    pub excluded: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McmSummary {
    /// Each observation here is a workload's mean, not an individual repetition.
    pub yosys_across_workloads: TimingSummary,
    pub paper_across_workloads: TimingSummary,
    pub runtime_ratio: Option<f64>,
    pub runtime_coverage: Coverage,
    pub quality: ComparisonCounts,
    pub area_smaller_or_equal: usize,
    pub area_larger: usize,
    pub area_excluded: usize,
    pub average_area_savings_percent: Option<f64>,
    pub area_savings_coverage: Coverage,
}

impl McmView {
    pub fn report(&self, history: &History<McmOutput>) -> Result<McmReport> {
        // The scalar projection owns validation, matching, and exclusion rules.
        let runtime = self.project(
            history,
            &McmSelection {
                metric: McmMetric::RuntimeRatio,
            },
        )?;
        let area = self.project(
            history,
            &McmSelection {
                metric: McmMetric::AreaSavings,
            },
        )?;
        let records: HashMap<_, _> = history
            .records
            .iter()
            .map(|entry| {
                (
                    (
                        entry.record.commit.id.0.as_str(),
                        entry.record.benchmark_index,
                    ),
                    &entry.record.outcome,
                )
            })
            .collect();
        let mut commits = Vec::new();
        for (position, commit) in history.commits.iter().enumerate() {
            let mut workloads = Vec::new();
            let mut yosys_times = Vec::new();
            let mut paper_times = Vec::new();
            let mut quality = ComparisonCounts::default();
            let mut area_smaller_or_equal = 0;
            let mut area_larger = 0;
            for (&index, benchmark) in &history.benchmarks {
                let outcome =
                    records
                        .get(&(commit.id.0.as_str(), index))
                        .map(|outcome| match outcome {
                            Outcome::Failed(reason) => Outcome::Failed(reason.clone()),
                            Outcome::Unsupported(reason) => Outcome::Unsupported(reason.clone()),
                            Outcome::Succeeded(output) => {
                                let stats = workload(output);
                                if let Ok((yosys, paper)) =
                                    summarize(output, McmMetric::RuntimeRatio)
                                {
                                    yosys_times.push(yosys * 1000.0);
                                    paper_times.push(paper * 1000.0);
                                }
                                match stats.quality {
                                    Some(Quality::Better) => quality.better += 1,
                                    Some(Quality::Same) => quality.same += 1,
                                    Some(Quality::Worse) => quality.worse += 1,
                                    None => {}
                                }
                                // Area ordering remains meaningful for a zero baseline;
                                // area ratios and savings require a positive baseline.
                                if let Some((with, baseline)) =
                                    stats.with_mcm_area.zip(stats.baseline_area)
                                {
                                    if with <= baseline {
                                        area_smaller_or_equal += 1;
                                    } else {
                                        area_larger += 1;
                                    }
                                }
                                Outcome::Succeeded(stats)
                            }
                        });
                let coefficients = benchmark
                    .parameters
                    .get("coefficients")
                    .and_then(|value| value.as_array())
                    .and_then(|values| values.iter().map(|value| value.as_i64()).collect());
                workloads.push(McmWorkloadReport {
                    benchmark_index: index,
                    coefficients,
                    outcome,
                });
            }
            quality.excluded = workloads.len() - quality.better - quality.same - quality.worse;
            let runtime = &runtime.series[0].points[position];
            let area = &area.series[0].points[position];
            let summary = McmSummary {
                yosys_across_workloads: timing(&yosys_times),
                paper_across_workloads: timing(&paper_times),
                runtime_ratio: runtime.value,
                runtime_coverage: runtime.coverage.clone(),
                quality,
                area_smaller_or_equal,
                area_larger,
                area_excluded: workloads.len() - area_smaller_or_equal - area_larger,
                average_area_savings_percent: area.value,
                area_savings_coverage: area.coverage.clone(),
            };
            commits.push(McmCommitReport {
                commit: commit.id.clone(),
                workloads,
                summary,
            });
        }
        Ok(McmReport { commits })
    }
}

fn workload(output: &McmOutput) -> McmWorkloadSummary {
    let yosys = yosys(&output.yosys_mcm.values);
    let paper = paper(&output.paper.value.values);
    let quality = yosys.as_ref().zip(paper.as_ref()).map(|(yosys, paper)| {
        match yosys
            .multipliers
            .total_cmp(&paper.multipliers)
            .then_with(|| yosys.adds_subs.total_cmp(&paper.adds_subs))
        {
            Ordering::Less => Quality::Better,
            Ordering::Equal => Quality::Same,
            Ordering::Greater => Quality::Worse,
        }
    });
    let runtime = summarize(output, McmMetric::RuntimeRatio);
    let area = summarize(output, McmMetric::AreaSavings);
    let mut notes = Vec::new();
    if let Err(reason) = &runtime {
        notes.push(format!("runtime: {reason}"));
    }
    if let Err(reason) = &area {
        notes.push(format!("area: {reason}"));
    }
    McmWorkloadSummary {
        yosys,
        paper,
        paper_reference: output.paper.id,
        baseline_area: area_mean(&output.synth_without_mcm.values, "baseline").ok(),
        with_mcm_area: area_mean(&output.synth_with_mcm.values, "with MCM").ok(),
        runtime_ratio: runtime.ok().map(|(yosys, paper)| yosys / paper),
        area_ratio: area.ok().map(|(with, baseline)| with / baseline),
        quality,
        notes,
    }
}

fn timing(values: &[f64]) -> TimingSummary {
    let average = mean(values.iter().copied());
    let sample_stdev_ms = average.filter(|_| values.len() >= 2).map(|average| {
        (values
            .iter()
            .map(|value| (value - average).powi(2))
            .sum::<f64>()
            / (values.len() - 1) as f64)
            .sqrt()
    });
    TimingSummary {
        samples: values.len(),
        mean_ms: average,
        sample_stdev_ms,
    }
}

fn yosys(values: &[YosysStats]) -> Option<ToolSummary> {
    if values.is_empty() {
        return None;
    }
    Some(ToolSummary {
        timing: timing(
            &values
                .iter()
                .map(|value| value.wall_clock_time.as_secs_f64() * 1000.0)
                .collect::<Vec<_>>(),
        ),
        adds_subs: mean(values.iter().map(|value| value.adds_subs as f64))?,
        multipliers: mean(values.iter().map(|value| value.multipliers as f64))?,
        negations: mean(values.iter().map(|value| value.negations as f64))?,
        shifts: None,
        depth: values
            .iter()
            .map(|value| value.depth.map(f64::from))
            .collect::<Option<Vec<_>>>()
            .and_then(|values| mean(values.into_iter())),
    })
}

fn paper(values: &[PaperStats]) -> Option<ToolSummary> {
    if values.is_empty() {
        return None;
    }
    Some(ToolSummary {
        timing: timing(
            &values
                .iter()
                .map(|value| value.wall_clock_time.as_secs_f64() * 1000.0)
                .collect::<Vec<_>>(),
        ),
        adds_subs: mean(values.iter().map(|value| value.adds_subs as f64))?,
        multipliers: mean(values.iter().map(|value| value.multipliers as f64))?,
        negations: mean(values.iter().map(|value| value.negations as f64))?,
        shifts: mean(values.iter().map(|value| value.shifts as f64)),
        depth: mean(values.iter().map(|value| f64::from(value.depth))),
    })
}

fn number(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "n/a".into())
}
fn percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}%"))
        .unwrap_or_else(|| "n/a".into())
}
fn stats(value: &Option<ToolSummary>) -> String {
    value
        .as_ref()
        .map(|value| {
            format!(
                "add+sub={}, mul={}, depth={}",
                value.adds_subs,
                value.multipliers,
                value
                    .depth
                    .map(|depth| depth.to_string())
                    .unwrap_or_else(|| "unknown".into())
            )
        })
        .unwrap_or_else(|| "n/a".into())
}

fn table(f: &mut fmt::Formatter<'_>, title: &str, rows: &[(&str, String)]) -> fmt::Result {
    writeln!(f, "{title}")?;
    let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
    for (label, value) in rows {
        writeln!(f, "  {label:width$}: {value}")?;
    }
    writeln!(f)
}

impl fmt::Display for McmReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for commit in &self.commits {
            writeln!(f, "Commit {}\n", commit.commit.0)?;
            for workload in &commit.workloads {
                let title = format!("Test {}", workload.benchmark_index);
                let coefficients = workload
                    .coefficients
                    .as_ref()
                    .map(|values| {
                        values
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "unknown".into());
                match &workload.outcome {
                    Some(Outcome::Succeeded(value)) => {
                        let mut rows = vec![
                            (
                                "yosys runtime (ms)",
                                number(value.yosys.as_ref().and_then(|value| value.timing.mean_ms)),
                            ),
                            (
                                "hcub_paper runtime (ms)",
                                number(value.paper.as_ref().and_then(|value| value.timing.mean_ms)),
                            ),
                            ("yosys stats", stats(&value.yosys)),
                            ("hcub_paper stats", stats(&value.paper)),
                            ("synth area", number(value.baseline_area)),
                            ("synth with MCM area", number(value.with_mcm_area)),
                            (
                                "MCM / baseline area",
                                percent(value.area_ratio.map(|value| value * 100.0)),
                            ),
                            ("runtime ratio (Yosys / paper)", number(value.runtime_ratio)),
                            ("coeffs", coefficients),
                        ];
                        if let Some(yosys) = &value.yosys
                            && yosys.timing.samples > 1
                        {
                            rows.push((
                                "yosys repetition stdev (ms)",
                                number(yosys.timing.sample_stdev_ms),
                            ));
                        }
                        if let Some(paper) = &value.paper
                            && paper.timing.samples > 1
                        {
                            rows.push((
                                "paper repetition stdev (ms)",
                                number(paper.timing.sample_stdev_ms),
                            ));
                        }
                        if !value.notes.is_empty() {
                            rows.push(("excluded metrics", value.notes.join("; ")));
                        }
                        table(f, &title, &rows)?;
                    }
                    other => {
                        let reason = match other {
                            Some(Outcome::Failed(reason)) => format!("failed: {reason}"),
                            Some(Outcome::Unsupported(reason)) => format!("unsupported: {reason}"),
                            _ => "no finished measurement".into(),
                        };
                        table(f, &title, &[("coeffs", coefficients), ("outcome", reason)])?;
                    }
                }
            }
            let summary = &commit.summary;
            table(
                f,
                "Results",
                &[
                    (
                        "Avg Yosys runtime (ms)",
                        number(summary.yosys_across_workloads.mean_ms),
                    ),
                    (
                        "Avg hcub_paper runtime (ms)",
                        number(summary.paper_across_workloads.mean_ms),
                    ),
                    (
                        "Stdev Yosys runtime across workloads (ms)",
                        number(summary.yosys_across_workloads.sample_stdev_ms),
                    ),
                    (
                        "Stdev hcub_paper runtime across workloads (ms)",
                        number(summary.paper_across_workloads.sample_stdev_ms),
                    ),
                    (
                        "Average runtime ratio (Yosys / paper)",
                        number(summary.runtime_ratio),
                    ),
                    (
                        "Yosys results smaller than HCUB paper",
                        summary.quality.better.to_string(),
                    ),
                    (
                        "Yosys results same as HCUB paper",
                        summary.quality.same.to_string(),
                    ),
                    (
                        "Yosys results larger than HCUB paper",
                        summary.quality.worse.to_string(),
                    ),
                    (
                        "MCM area smaller or same as baseline",
                        summary.area_smaller_or_equal.to_string(),
                    ),
                    (
                        "MCM area larger than baseline",
                        summary.area_larger.to_string(),
                    ),
                    (
                        "Avg MCM area savings",
                        percent(summary.average_area_savings_percent),
                    ),
                    (
                        "Runtime workloads included / selected",
                        format!(
                            "{}/{}",
                            summary.runtime_coverage.included, summary.runtime_coverage.total
                        ),
                    ),
                    (
                        "Quality workloads excluded",
                        summary.quality.excluded.to_string(),
                    ),
                    (
                        "Area comparison workloads excluded",
                        summary.area_excluded.to_string(),
                    ),
                    (
                        "Area savings workloads included / selected",
                        format!(
                            "{}/{}",
                            summary.area_savings_coverage.included,
                            summary.area_savings_coverage.total
                        ),
                    ),
                ],
            )?;
        }
        Ok(())
    }
}
