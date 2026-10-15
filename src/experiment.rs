//! Sequential orchestration of one suite over one branch.

use serde::Serialize;

use crate::{
    Error, Result,
    benchmark::ArtifactPolicy,
    build::Builder,
    history::{BranchHistory, RevisionSelection},
    store::Store,
    suite::Suite,
};

#[derive(Debug, Serialize)]
pub struct Experiment<S: Suite> {
    pub history: BranchHistory,
    pub revisions: RevisionSelection,
    pub suite: S,
    pub artifacts: ArtifactPolicy,
}

/// Freeze the suite, resolve commits, then build, run, and record sequentially.
pub fn run<S, B, D>(_experiment: &Experiment<S>, _builder: &mut B, _store: &mut D) -> Result<()>
where
    S: Suite,
    B: Builder<Tool = S::Tool>,
    D: Store,
{
    // TODO: Register the experiment and frozen benchmark collection.
    // TODO: Record attempt start/finish and advance commit completion safely.
    Err(Error::NotImplemented("sequential experiment orchestration"))
}
