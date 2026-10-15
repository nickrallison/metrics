use std::{
    path::PathBuf,
    process::{Command, Output},
    time::Duration,
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Error, Result,
    store::{Reference, ReferenceCache, ReferenceId},
};

/// Raw outputs are discarded unless explicitly requested; all flags default off.
///
/// Parsed statistics, timing samples, definitions, and outcome summaries are
/// always retained. Temporary files needed by another phase can exist without
/// becoming retained artifacts. Capturing stdout for parsing is not log retention.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ArtifactPolicy {
    pub logs: bool,
    pub scripts: bool,
    pub netlists: bool,
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub output: Output,
    pub elapsed: Duration,
}

/// Isolated workspace and process/cache helpers for one comparison attempt.
///
/// Use fresh temporary paths for repeated invocations and remove temporary
/// parser inputs after use. Retained artifacts need distinct attempt paths.
/// The permanent directory convention is still an open implementation decision.
pub struct RunContext<'a, C: ReferenceCache> {
    pub workspace: PathBuf,
    pub artifacts: ArtifactPolicy,
    cache: &'a mut C,
}

impl<'a, C: ReferenceCache> RunContext<'a, C> {
    pub fn new(workspace: PathBuf, artifacts: ArtifactPolicy, cache: &'a mut C) -> Self {
        Self {
            workspace,
            artifacts,
            cache,
        }
    }

    /// Measure immediately before spawning through exit/output collection.
    /// Preparation, parsing, and later artifact copying are outside this interval.
    /// Preserve nonzero termination and useful error context instead of treating
    /// captured stdout as proof of success. Timeout/cancellation support is pending.
    pub fn run_process(&mut self, _command: &mut Command) -> Result<ProcessOutput> {
        Err(Error::NotImplemented(
            "timed processes and artifact retention",
        ))
    }

    pub fn reference<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Reference<T>>> {
        self.cache.get(key)
    }

    pub fn cache_reference<T: Serialize>(&mut self, key: &str, value: &T) -> Result<ReferenceId> {
        self.cache.insert(key, value)
    }
}
