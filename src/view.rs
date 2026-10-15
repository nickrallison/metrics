//! Project domain outputs into frontend-independent series.

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
