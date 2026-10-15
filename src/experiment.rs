//! Sequential orchestration of one suite over one branch.
//!
//! Freeze and register the suite, resolve commits, then build and run each commit
//! in order. Do not overlap builds, reference computation, or other benchmark
//! processes with a timed process. Host activity outside this program remains
//! possible; record the execution environment and actual measurement order.
//!
//! Persist attempt start before execution and finalize its outcome afterward.
//! Interrupted attempts remain identifiable. Resume only against the same frozen
//! definitions and verified input contents. A commit is processed when every
//! expected benchmark has a terminal outcome, including failure or unsupported;
//! an interrupted attempt or build failure leaves it incomplete.
//!
//! Advance completion only through the processed prefix of the selected history.
//! A recent individual result is not a completion marker. Failed measurements can
//! be explicitly rerun; append attempts rather than overwriting history. Reruns
//! must not become extra independent repetition samples. Automatic retries and a
//! job graph are deferred; storage lifecycle methods still need to be added.

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
