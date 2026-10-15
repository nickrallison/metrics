use std::path::PathBuf;

use super::Builder;
use crate::{
    Error, Result,
    history::{Commit, CommitId},
};

#[derive(Debug, Clone)]
pub struct Yosys {
    pub commit: CommitId,
    pub executable: PathBuf,
    pub data_dir: PathBuf,
}

#[derive(Debug)]
pub struct YosysBuilder {
    pub repository: PathBuf,
    pub cache_dir: PathBuf,
}

impl Builder for YosysBuilder {
    type Tool = Yosys;

    fn build(&mut self, _commit: &Commit) -> Result<Yosys> {
        Err(Error::NotImplemented("Yosys builds and commit cache"))
    }
}
