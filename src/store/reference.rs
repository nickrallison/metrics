//! Independent reference observations and their cache contract.

use std::time::SystemTime;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::Result;

pub type ReferenceId = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference<T> {
    pub id: ReferenceId,
    pub value: T,
    pub measured_at: SystemTime,
}

/// Reuse a computation independent of the tool revision under test.
///
/// The benchmark constructs a key covering the reference tool's executable
/// identity, ordered workload values, widths, arguments, seed, parser version,
/// and sampling policy. A pathname alone is not tool identity. Exclude the Yosys
/// commit and unrelated synthesis-library settings from an MCM paper-tool key.
///
/// Cached timing requires a compatible measurement environment. Record that
/// environment in cache metadata and either restrict reuse or refresh explicitly.
/// The refresh policy and concrete key encoding still need choosing.
///
/// A refresh creates a new immutable reference ID. Comparisons retain the ID,
/// timestamp, and individual observations they used. Reusing that ID at another
/// commit is not a new independent reference measurement. Rerun Yosys measurements
/// at each commit even when their reference is cached.
pub trait ReferenceCache {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Reference<T>>>;

    /// Append a new reference; older comparisons retain their original IDs.
    fn insert<T: Serialize>(&mut self, key: &str, value: &T) -> Result<ReferenceId>;
}
