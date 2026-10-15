use std::path::PathBuf;

use serde::{Serialize, de::DeserializeOwned};

use super::{
    ExperimentId, History, HistoryQuery, RecordId, Reference, ReferenceCache, ReferenceId,
    RunRecord, Store,
};
use crate::{Error, Result, history::CommitId};

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
    fn append<O: Serialize>(&mut self, _record: &RunRecord<O>) -> Result<RecordId> {
        Err(Error::NotImplemented("SQLite result insertion"))
    }

    fn history<O: DeserializeOwned>(&self, _query: &HistoryQuery) -> Result<History<O>> {
        Err(Error::NotImplemented("SQLite history queries"))
    }

    fn last_complete(&self, _experiment: ExperimentId) -> Result<Option<CommitId>> {
        Err(Error::NotImplemented("SQLite commit completion lookup"))
    }
}
