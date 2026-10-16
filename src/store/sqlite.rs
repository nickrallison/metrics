use std::{path::PathBuf, time::SystemTime};

use serde::{Serialize, de::DeserializeOwned};

use super::{
    ExperimentDefinition, ExperimentId, History, HistoryQuery, Outcome, RecordId, Reference,
    ReferenceCache, ReferenceId, RunStart, Store,
};
use crate::{
    Error, Result,
    history::{Commit, CommitId},
};

/// Backend placeholder. Construction does not open or create a database yet.
#[derive(Debug)]
pub struct SqliteStore {
    pub path: PathBuf,
}

impl SqliteStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl ReferenceCache for SqliteStore {
    fn get<T: DeserializeOwned>(&self, _key: &str) -> Result<Option<Reference<T>>> {
        Err(Error::NotImplemented("SQLite reference lookup"))
    }

    fn insert<T: Serialize>(&mut self, _key: &str, _value: &T) -> Result<ReferenceId> {
        Err(Error::NotImplemented("SQLite reference insertion"))
    }
}

impl Store for SqliteStore {
    fn register_experiment(&mut self, _definition: &ExperimentDefinition) -> Result<ExperimentId> {
        Err(Error::NotImplemented("SQLite experiment registration"))
    }

    fn start_run(&mut self, _start: &RunStart) -> Result<RecordId> {
        Err(Error::NotImplemented("SQLite attempt start"))
    }

    fn finish_run<O: Serialize>(
        &mut self,
        _record: RecordId,
        _finished_at: SystemTime,
        _outcome: &Outcome<O>,
    ) -> Result<()> {
        Err(Error::NotImplemented("SQLite attempt finalization"))
    }

    fn record_build_failure(
        &mut self,
        _experiment: ExperimentId,
        _commit: &Commit,
        _reason: &str,
    ) -> Result<()> {
        Err(Error::NotImplemented("SQLite build failure recording"))
    }

    fn complete_commit(&mut self, _experiment: ExperimentId, _commit: &CommitId) -> Result<()> {
        Err(Error::NotImplemented("SQLite commit completion"))
    }

    fn history<O: DeserializeOwned>(&self, _query: &HistoryQuery) -> Result<History<O>> {
        Err(Error::NotImplemented("SQLite history queries"))
    }

    fn last_complete(&self, _experiment: ExperimentId) -> Result<Option<CommitId>> {
        Err(Error::NotImplemented("SQLite commit completion lookup"))
    }
}
