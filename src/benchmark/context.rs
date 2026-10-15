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

/// Raw outputs are discarded unless explicitly requested.
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
