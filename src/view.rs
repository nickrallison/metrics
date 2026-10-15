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
//! Access to tables, distributions, and coverage counts may extend `ViewData`;
//! the current scaffold only defines scalar series.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Result, history::CommitId, store::History};

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
}
