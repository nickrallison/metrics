//! Project domain outputs into frontend-independent series.
//!
//! A custom view consumes typed history and owns domain-specific filtering and
//! comparisons. Frontends consume the projected data without deserializing every
//! benchmark output. Shared synthesis views can be reused by different suites.
//!
//! Summarize repetitions within each workload before aggregating matching
//! workloads. Surface missing coverage and absent/failed observations rather than
//! converting them to zero. Keep units and measurement scope explicit. Preserve
//! recorded revision order instead of sorting by commit or execution timestamps.
//! Points include workload coverage and exclusion reasons. Tables and
//! distributions may extend `ViewData` later.

use std::fmt;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Result, history::CommitId, store::History};

pub mod html;

pub trait View {
    type Output: DeserializeOwned;
    type Selection;

    fn project(
        &self,
        history: &History<Self::Output>,
        selection: &Self::Selection,
    ) -> Result<ViewData>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewData {
    pub series: Vec<Series>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub label: String,
    pub unit: String,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub commit: CommitId,
    pub value: Option<f64>,
    pub coverage: Coverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coverage {
    pub included: usize,
    pub total: usize,
    pub exclusions: Vec<ExcludedWorkload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExcludedWorkload {
    pub benchmark_index: usize,
    pub reason: String,
}

/// A small text renderer; graphical frontends can consume the same series.
impl fmt::Display for ViewData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, series) in self.series.iter().enumerate() {
            if index != 0 {
                writeln!(f)?;
            }
            writeln!(f, "{} [{}]", series.label, series.unit)?;
            writeln!(f, "{:<12} {:>12} {:>12}", "commit", "value", "workloads")?;
            if series.points.is_empty() {
                writeln!(f, "No revisions selected.")?;
            }
            for point in &series.points {
                let commit: String = point.commit.0.chars().take(12).collect();
                let value = point
                    .value
                    .map(|value| {
                        if value != 0.0 && !(1e-4..1e6).contains(&value.abs()) {
                            format!("{value:.4e}")
                        } else {
                            format!("{value:.4}")
                        }
                    })
                    .unwrap_or_else(|| "n/a".into());
                let coverage = format!("{}/{}", point.coverage.included, point.coverage.total);
                writeln!(f, "{commit:<12} {value:>12} {coverage:>12}")?;
                for exclusion in &point.coverage.exclusions {
                    writeln!(
                        f,
                        "  workload {}: {}",
                        exclusion.benchmark_index, exclusion.reason
                    )?;
                }
            }
        }
        Ok(())
    }
}
