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

pub trait ReferenceCache {
    fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Reference<T>>>;

    /// Append a new reference; older comparisons retain their original IDs.
    fn insert<T: Serialize>(&mut self, key: &str, value: &T) -> Result<ReferenceId>;
}
