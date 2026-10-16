use std::time::{Duration, UNIX_EPOCH};

use serde_json::json;

use super::*;
use crate::{
    benchmark::Samples,
    benchmarks::mcm::{PaperStats, YosysStats},
    history::{Commit, CommitId},
    store::{BenchmarkDefinition, HistoryEntry, Reference, RunRecord},
};

fn history(workloads: usize) -> History<McmOutput> {
    History {
        commits: ["b", "a"]
            .into_iter()
            .enumerate()
            .map(|(index, id)| Commit {
                id: CommitId(id.into()),
                // Frozen order deliberately differs from lexical/timestamp order.
                committed_at: UNIX_EPOCH + Duration::from_secs((2 - index) as u64),
                author: "Test".into(),
                commit_message: id.into(),
            })
            .collect(),
        benchmarks: (0..workloads)
            .map(|index| {
                (
                    index,
                    BenchmarkDefinition {
                        kind: KIND.into(),
                        version: VERSION,
                        parameters: json!({"workload": index}),
                    },
                )
            })
            .collect(),
        records: Vec::new(),
    }
}

fn output(
    yosys_ms: &[u64],
    paper_ms: &[u64],
    with_area: &[Option<f64>],
    baseline: &[Option<f64>],
) -> McmOutput {
    McmOutput {
        yosys_mcm: Samples {
            values: yosys_ms
                .iter()
                .map(|&ms| YosysStats {
                    wall_clock_time: Duration::from_millis(ms),
                    adds_subs: 0,
                    negations: 0,
                    multipliers: 0,
                    depth: None,
                })
                .collect(),
        },
        paper: Reference {
            id: 1,
            measured_at: UNIX_EPOCH,
            value: Samples {
                values: paper_ms
                    .iter()
                    .map(|&ms| PaperStats {
                        wall_clock_time: Duration::from_millis(ms),
                        adds_subs: 0,
                        shifts: 0,
                        negations: 0,
                        multipliers: 0,
                        depth: 0,
                    })
                    .collect(),
            },
        },
        synth_with_mcm: Samples {
            values: with_area
                .iter()
                .map(|&area| SynthStats {
                    wall_clock_time: Duration::ZERO,
                    cell_count: 0,
                    area,
                })
                .collect(),
        },
        synth_without_mcm: Samples {
            values: baseline
                .iter()
                .map(|&area| SynthStats {
                    wall_clock_time: Duration::ZERO,
                    cell_count: 0,
                    area,
                })
                .collect(),
        },
    }
}

fn record(
    history: &mut History<McmOutput>,
    commit: usize,
    workload: usize,
    outcome: Outcome<McmOutput>,
) {
    history.records.push(HistoryEntry {
        id: history.records.len() as i64 + 1,
        record: RunRecord {
            experiment: 1,
            commit: history.commits[commit].clone(),
            benchmark_index: workload,
            benchmark: history.benchmarks[&workload].clone(),
            started_at: UNIX_EPOCH,
            finished_at: UNIX_EPOCH,
            purpose: RunPurpose::Measurement,
            artifacts: None,
            outcome,
        },
    });
}

fn project(history: &History<McmOutput>, metric: McmMetric) -> ViewData {
    McmView.project(history, &McmSelection { metric }).unwrap()
}

fn close(actual: Option<f64>, expected: f64) {
    assert!(
        (actual.unwrap() - expected).abs() < 1e-10,
        "{actual:?} != {expected}"
    );
}

#[test]
fn runtime_averages_repetitions_then_uses_ratio_of_matched_workload_means() {
    let mut history = history(2);
    // Means 3/1 and 10/5 give 13/6, not mean(3, 2) = 2.5.
    record(
        &mut history,
        0,
        1,
        Outcome::Succeeded(output(&[10], &[4, 6], &[], &[])),
    );
    record(
        &mut history,
        0,
        0,
        Outcome::Succeeded(output(&[2, 4], &[1], &[], &[])),
    );
    let data = project(&history, McmMetric::RuntimeRatio);
    let points = &data.series[0].points;
    close(points[0].value, 13.0 / 6.0);
    assert_eq!(points[0].coverage.included, 2);
    assert_eq!(points[0].coverage.total, 2);
    assert_eq!(points[0].commit.0, "b");
    assert_eq!(points[1].commit.0, "a");
    assert!(points[1].value.is_none());
    assert_eq!(points[1].coverage.exclusions.len(), 2);
}

#[test]
fn area_averages_phases_then_workload_savings_including_regressions() {
    let mut history = history(2);
    // Workload savings: 50% and -20%; unequal repeat counts do not add weight.
    record(
        &mut history,
        0,
        0,
        Outcome::Succeeded(output(
            &[],
            &[],
            &[Some(1.0), Some(3.0)],
            &[Some(2.0), Some(6.0)],
        )),
    );
    record(
        &mut history,
        0,
        1,
        Outcome::Succeeded(output(&[], &[], &[Some(12.0)], &[Some(10.0)])),
    );
    let data = project(&history, McmMetric::AreaSavings);
    close(data.series[0].points[0].value, 15.0);
    assert_eq!(data.series[0].unit, "%");
}

#[test]
fn missing_failures_and_invalid_denominators_are_excluded_from_both_sides() {
    let mut history = history(7);
    record(
        &mut history,
        0,
        0,
        Outcome::Succeeded(output(&[2], &[1], &[], &[])),
    );
    record(&mut history, 0, 1, Outcome::Failed("tool failed".into()));
    record(
        &mut history,
        0,
        2,
        Outcome::Unsupported("old commit".into()),
    );
    record(
        &mut history,
        0,
        3,
        Outcome::Succeeded(output(&[], &[1], &[], &[])),
    );
    record(
        &mut history,
        0,
        4,
        Outcome::Succeeded(output(&[1000], &[0], &[], &[])),
    );
    record(
        &mut history,
        0,
        5,
        Outcome::Succeeded(output(&[1], &[], &[], &[])),
    );
    let data = project(&history, McmMetric::RuntimeRatio);
    let point = &data.series[0].points[0];
    close(point.value, 2.0);
    assert_eq!(point.coverage.included, 1);
    assert_eq!(point.coverage.total, 7);
    let reasons: Vec<_> = point
        .coverage
        .exclusions
        .iter()
        .map(|item| item.reason.as_str())
        .collect();
    assert_eq!(
        reasons,
        [
            "failed: tool failed",
            "unsupported: old commit",
            "empty Yosys timing samples",
            "zero paper runtime",
            "empty paper timing samples",
            "no finished measurement"
        ]
    );
    assert!(serde_json::to_string(&data).unwrap().contains("null"));
}

#[test]
fn area_rejects_missing_empty_negative_nonfinite_and_zero_baselines() {
    let invalid = [
        (vec![], vec![Some(1.0)]),
        (vec![Some(1.0)], vec![]),
        (vec![Some(1.0), None], vec![Some(1.0)]),
        (vec![Some(1.0)], vec![None]),
        (vec![Some(-1.0)], vec![Some(1.0)]),
        (vec![Some(1.0)], vec![Some(-1.0)]),
        (vec![Some(f64::NAN)], vec![Some(1.0)]),
        (vec![Some(1.0)], vec![Some(f64::INFINITY)]),
        (vec![Some(1.0)], vec![Some(0.0)]),
        (vec![Some(f64::MAX)], vec![Some(f64::MIN_POSITIVE)]),
    ];
    let mut history = history(invalid.len());
    for (index, (with, baseline)) in invalid.iter().enumerate() {
        record(
            &mut history,
            0,
            index,
            Outcome::Succeeded(output(&[], &[], with, baseline)),
        );
    }
    let data = project(&history, McmMetric::AreaSavings);
    let point = &data.series[0].points[0];
    assert!(point.value.is_none());
    assert_eq!(point.coverage.included, 0);
    assert_eq!(point.coverage.exclusions.len(), invalid.len());
    assert!(!serde_json::to_string(&data).unwrap().contains("NaN"));
}

#[test]
fn zero_numerators_are_valid_and_large_areas_do_not_overflow_means() {
    let mut history = history(2);
    record(
        &mut history,
        0,
        0,
        Outcome::Succeeded(output(&[0], &[1], &[Some(0.0)], &[Some(1.0)])),
    );
    record(
        &mut history,
        0,
        1,
        Outcome::Succeeded(output(
            &[0],
            &[1],
            &[Some(f64::MAX), Some(f64::MAX)],
            &[Some(f64::MAX), Some(f64::MAX)],
        )),
    );
    close(
        project(&history, McmMetric::RuntimeRatio).series[0].points[0].value,
        0.0,
    );
    close(
        project(&history, McmMetric::AreaSavings).series[0].points[0].value,
        50.0,
    );
}

#[test]
fn cached_references_are_used_per_workload_without_pooling_across_commits() {
    let mut history = history(1);
    let original = output(&[2], &[1, 3], &[], &[]);
    let mut later = original.clone();
    later.yosys_mcm.values[0].wall_clock_time = Duration::from_millis(8);
    record(&mut history, 0, 0, Outcome::Succeeded(original));
    record(&mut history, 1, 0, Outcome::Succeeded(later));
    let data = project(&history, McmMetric::RuntimeRatio);
    close(data.series[0].points[0].value, 1.0);
    close(data.series[0].points[1].value, 4.0);
    assert_eq!(history.records.len(), 2);
}

#[test]
fn empty_history_or_selection_does_not_fabricate_zero_metrics() {
    let mut history = history(0);
    let data = project(&history, McmMetric::RuntimeRatio);
    assert!(
        data.series[0]
            .points
            .iter()
            .all(|point| point.value.is_none() && point.coverage.total == 0)
    );
    history.commits.clear();
    assert!(
        project(&history, McmMetric::AreaSavings).series[0]
            .points
            .is_empty()
    );
}

#[test]
fn rejects_duplicate_attempts_and_inconsistent_history_metadata() {
    let mut original = history(1);
    record(
        &mut original,
        0,
        0,
        Outcome::Succeeded(output(&[2], &[1], &[], &[])),
    );
    let check = |history: &History<McmOutput>| {
        assert!(matches!(
            McmView.project(
                history,
                &McmSelection {
                    metric: McmMetric::RuntimeRatio
                }
            ),
            Err(Error::InvalidView(_))
        ))
    };
    let mut invalid = original.clone();
    invalid.records.push(invalid.records[0].clone());
    check(&invalid);
    let mut invalid = original.clone();
    invalid.commits.push(invalid.commits[0].clone());
    check(&invalid);
    let mut invalid = original.clone();
    invalid.records[0].record.benchmark_index = 99;
    check(&invalid);
    let mut invalid = original.clone();
    invalid.records[0].record.commit.id.0 = "unknown".into();
    check(&invalid);
    let mut invalid = original.clone();
    invalid.records[0].record.purpose = RunPurpose::Inspection { source: 1 };
    check(&invalid);
    let mut invalid = original;
    invalid.benchmarks.get_mut(&0).unwrap().version += 1;
    check(&invalid);
}
