//! One workload and its complete comparison at one commit.

mod context;

pub use context::{ArtifactPolicy, ProcessOutput, RunContext};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{Result, store::ReferenceCache};

pub trait Benchmark: Serialize {
    type Input;
    type Output: Serialize + DeserializeOwned;

    fn kind(&self) -> &'static str;
    fn version(&self) -> u32;

    fn run<C: ReferenceCache>(
        &self,
        input: &Self::Input,
        context: &mut RunContext<'_, C>,
    ) -> Result<Self::Output>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Samples<T> {
    pub values: Vec<T>,
}
