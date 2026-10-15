//! One workload and its complete comparison at one commit.
//!
//! Serialize the benchmark value to retain its workload and procedure settings.
//! `kind` and `version` identify the procedure/output contract; changes affecting
//! comparability or serialized interpretation need a recorded version boundary.
//! Runtime `Input` contains tool handles and need not be serializable.
//!
//! Implementations choose process order, input conversion, repetition counts,
//! parsing, and independent reference computations. Keep each timing sample in the
//! output instead of replacing observations with an average. Repetition varies
//! within a workload; variation between workloads is a different statistic.
//!
//! Launch measured processes through `RunContext`. Preparation and parsing are
//! outside subprocess timings. Add an explicitly named complete-flow timing if
//! needed. Unsupported historical commands/formats produce an explicit outcome;
//! adaptations that change the procedure must remain visible in its version.
//!
//! These traits use static dispatch. Generic cache methods do not imply a
//! trait-object or plugin interface. Separate parser and stage traits are deferred.

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

/// Individual observations for one procedure and workload; successful collections
/// should be nonempty. The current scaffold does not enforce that invariant yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Samples<T> {
    pub values: Vec<T>,
}
