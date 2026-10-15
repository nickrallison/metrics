//! Select immutable commits from one branch's history.
//!
//! Capture the branch tip once per invocation. Walk its first-parent lineage;
//! merged side-branch commits are not separate history points. `Since(anchor)`
//! excludes the anchor and includes the captured tip, oldest first. Reject an
//! anchor outside that lineage. Explicit selections preserve supplied order;
//! reject duplicate IDs and commits outside the same lineage.
//!
//! Resolve names to immutable IDs before building. Preserve the original selection
//! and resolved order in experiment metadata. Plotting order is separate from
//! commit timestamps and the wall-clock time at which measurements were collected.
//! Recheck lineage when resuming after a branch rewrite instead of silently
//! treating the old completion marker as an ancestor of the new tip.

use std::{path::PathBuf, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchHistory {
    pub repository: PathBuf,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RevisionSelection {
    Explicit(Vec<CommitId>),
    Since(CommitId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: CommitId,
    pub committed_at: SystemTime,
    pub author: String,
    pub commit_message: String,
}

impl BranchHistory {
    /// Capture the tip and resolve the selection along the branch's main line.
    pub fn resolve(&self, _selection: &RevisionSelection) -> Result<Vec<Commit>> {
        Err(Error::NotImplemented("Git revision selection"))
    }
}
