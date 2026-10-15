//! Statistics for comparisons over Yosys history.

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

/// First summarize repeated observations for each workload. On the same matched
/// workload set, runtime ratio is mean(Yosys time) / mean(paper time), matching
/// the reference script; it is not mean(per-workload time ratios). Area savings
/// is mean(100 * (1 - with_mcm_area / baseline_area)). Area ratio is
/// with_mcm_area / baseline_area for an individual workload.
///
/// Reject zero/absent denominators and report excluded coverage. Keep sample
/// standard deviation across repeated timings separate from the reference
/// script's deviation across different workloads. A cached paper reference does
/// not contribute new independent samples at each commit.
///
/// Quality ordering is lexicographic: fewer remaining multipliers first, then
/// fewer add/sub operations. Equal values mean equal quality under that rule;
/// depth and negations are separate observations. Additional count/depth series,
/// coverage, and comparison tables are intended extensions of the initial view.
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
