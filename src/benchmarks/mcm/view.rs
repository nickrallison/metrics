use super::McmOutput;
use crate::{
    Error, Result,
    store::History,
    view::{View, ViewData},
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

pub struct McmView;

impl View for McmView {
    type Output = McmOutput;
    type Selection = McmSelection;

    fn project(
        &self,
        _history: &History<McmOutput>,
        _selection: &McmSelection,
    ) -> Result<ViewData> {
        // TODO: Summarize repetitions per workload before aggregating workloads.
        Err(Error::NotImplemented("MCM history projection"))
    }
}
