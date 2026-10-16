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

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchHistory {
    pub repository: PathBuf,
    /// Local branch name, or a full `refs/heads/` or `refs/remotes/` reference.
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RevisionSelection {
    Explicit(Vec<CommitId>),
    Since(CommitId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub id: CommitId,
    pub committed_at: SystemTime,
    pub author: String,
    pub commit_message: String,
}

impl BranchHistory {
    /// Capture the tip and resolve the selection along the branch's main line.
    /// Returned IDs are full object names; author includes name and email, and
    /// messages retain their complete body and trailing newline.
    pub fn resolve(&self, selection: &RevisionSelection) -> Result<Vec<Commit>> {
        let branch =
            if self.branch.starts_with("refs/heads/") || self.branch.starts_with("refs/remotes/") {
                self.branch.clone()
            } else {
                format!("refs/heads/{}", self.branch)
            };
        let tip = self.resolve_commit(&branch)?;
        let output = self.git(&[
            "log",
            "--no-patch",
            "--no-color",
            "--no-decorate",
            "--no-notes",
            "--no-show-signature",
            "--first-parent",
            "--reverse",
            "-z",
            "--encoding=UTF-8",
            "--format=%H%x00%ct%x00%an <%ae>%x00%B",
            &tip.0,
            "--",
        ])?;
        let commits = parse_log(&output)?;
        let positions: HashMap<_, _> = commits
            .iter()
            .enumerate()
            .map(|(index, commit)| (commit.id.0.as_str(), index))
            .collect();
        let position = |id: &CommitId| -> Result<usize> {
            positions.get(id.0.as_str()).copied().ok_or_else(|| {
                Error::InvalidRevision(format!(
                    "{} is not on the first-parent history of {}",
                    id.0, self.branch,
                ))
            })
        };
        match selection {
            RevisionSelection::Since(anchor) => {
                let anchor = self.resolve_commit(&anchor.0)?;
                Ok(commits[position(&anchor)? + 1..].to_vec())
            }
            RevisionSelection::Explicit(requested) => {
                let mut seen = HashSet::new();
                let mut selected = Vec::with_capacity(requested.len());
                for requested in requested {
                    let id = self.resolve_commit(&requested.0)?;
                    let index = position(&id)?;
                    if !seen.insert(id.0.clone()) {
                        return Err(Error::InvalidRevision(format!("duplicate commit {}", id.0)));
                    }
                    selected.push(commits[index].clone());
                }
                Ok(selected)
            }
        }
    }

    fn resolve_commit(&self, revision: &str) -> Result<CommitId> {
        let revision = format!("{revision}^{{commit}}");
        let output = self.git(&["rev-parse", "--verify", "--end-of-options", &revision])?;
        parse_id(text(&output)?.trim())
    }

    fn git(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = Command::new("git")
            .arg("--no-pager")
            .arg("-C")
            .arg(&self.repository)
            .args(args)
            .output()?;
        if !output.status.success() {
            return Err(Error::Git {
                repository: self.repository.clone(),
                command: args.join(" "),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
            });
        }
        Ok(output.stdout)
    }
}

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|error| Error::InvalidGitOutput(error.to_string()))
}

fn parse_id(id: &str) -> Result<CommitId> {
    if !matches!(id.len(), 40 | 64) || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::InvalidGitOutput(format!("invalid object ID {id:?}")));
    }
    Ok(CommitId(id.into()))
}

fn parse_log(output: &[u8]) -> Result<Vec<Commit>> {
    // NUL-delimited fields preserve arbitrary newlines in names and messages.
    let fields: Vec<_> = text(output)?
        .strip_suffix('\0')
        .ok_or_else(|| Error::InvalidGitOutput("unterminated commit record".into()))?
        .split('\0')
        .collect();
    if !fields.len().is_multiple_of(4) {
        return Err(Error::InvalidGitOutput("incomplete commit record".into()));
    }
    fields
        .chunks_exact(4)
        .map(|fields| {
            let seconds: i64 = fields[1].parse().map_err(|_| {
                Error::InvalidGitOutput(format!("invalid timestamp {:?}", fields[1]))
            })?;
            let duration = Duration::from_secs(seconds.unsigned_abs());
            let committed_at = if seconds < 0 {
                UNIX_EPOCH.checked_sub(duration)
            } else {
                UNIX_EPOCH.checked_add(duration)
            }
            .ok_or_else(|| Error::InvalidGitOutput("timestamp out of range".into()))?;
            Ok(Commit {
                id: parse_id(fields[0])?,
                committed_at,
                author: fields[2].into(),
                commit_message: fields[3].into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
