//! Select immutable commits from one branch's history.

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
