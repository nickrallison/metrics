use std::{collections::BTreeMap, time::SystemTime};

use super::{McmMetric, McmOutput, McmSelection, McmView};
use crate::{
    Error, Result,
    store::{ExperimentId, History},
    view::{
        Series, View,
        html::{Metric, Snapshot, SnapshotCommit, Workload, timestamp},
    },
};

impl McmView {
    /// Freeze aggregate and per-input projections for an offline HTML viewer.
    pub fn snapshot(
        &self,
        experiment: ExperimentId,
        history: &History<McmOutput>,
        metrics: &[McmMetric],
    ) -> Result<Snapshot> {
        let project = |history: &History<McmOutput>, metric| -> Result<Series> {
            self.project(history, &McmSelection { metric })?
                .series
                .into_iter()
                .next()
                .ok_or_else(|| Error::InvalidView("MCM projection has no series".into()))
        };
        let mut projected = Vec::new();
        for &metric in metrics {
            let aggregate = project(history, metric)?;
            let mut inputs = BTreeMap::new();
            for (&index, benchmark) in &history.benchmarks {
                let selected = History {
                    commits: history.commits.clone(),
                    benchmarks: [(index, benchmark.clone())].into(),
                    records: history
                        .records
                        .iter()
                        .filter(|entry| entry.record.benchmark_index == index)
                        .cloned()
                        .collect(),
                };
                inputs.insert(index, project(&selected, metric)?);
            }
            let (id, description, reference, reference_label) = match metric {
                McmMetric::RuntimeRatio => (
                    "runtime",
                    "Yosys / paper runtime. Lower is better. Aggregate: ratio of mean runtimes across matched inputs.",
                    1.0,
                    "Equal runtime",
                ),
                McmMetric::AreaSavings => (
                    "area",
                    "Area saved against synthesis without MCM. Higher is better. Aggregate: mean per-input percentage savings.",
                    0.0,
                    "Equal area",
                ),
            };
            projected.push(Metric {
                id: id.into(),
                description: description.into(),
                reference,
                reference_label: reference_label.into(),
                aggregate,
                inputs,
            });
        }
        Ok(Snapshot {
            experiment,
            generated_at: timestamp(SystemTime::now()),
            commits: history
                .commits
                .iter()
                .map(|commit| SnapshotCommit {
                    hash: commit.id.0.clone(),
                    author: commit.author.clone(),
                    message: commit.commit_message.clone(),
                    committed_at: timestamp(commit.committed_at),
                })
                .collect(),
            workloads: history
                .benchmarks
                .iter()
                .map(|(&index, benchmark)| {
                    let coefficients = benchmark.parameters.get("coefficients");
                    Workload {
                        index,
                        label: format!(
                            "#{index}: {}",
                            coefficients
                                .map_or_else(|| "unknown coefficients".into(), |v| v.to_string())
                        ),
                        parameters: benchmark.parameters.clone(),
                    }
                })
                .collect(),
            metrics: projected,
        })
    }
}
