//! Server-side registry for typed benchmark analyses.

use std::collections::BTreeMap;

pub use benchmark_analysis_types::*;

use crate::store::{ExperimentId, SqliteStore};

/// Typed benchmark modules register adapters here. The browser only receives
/// descriptors and presentation data; stored output types remain server-side.
pub trait AnalysisProvider: Send + Sync {
    fn descriptor(&self) -> AnalysisDescriptor;

    fn supports(&self, benchmark_types: &[(String, u32)]) -> bool;

    fn run(
        &self,
        store: &SqliteStore,
        experiment: ExperimentId,
        parameters: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<AnalysisResult, String>;
}

#[derive(Default)]
pub struct AnalysisRegistry {
    providers: BTreeMap<String, Box<dyn AnalysisProvider>>,
}

impl AnalysisRegistry {
    pub fn register(&mut self, provider: impl AnalysisProvider + 'static) {
        let id = provider.descriptor().id;
        self.providers.insert(id, Box::new(provider));
    }

    pub fn descriptors_for(&self, benchmark_types: &[(String, u32)]) -> Vec<AnalysisDescriptor> {
        self.providers
            .values()
            .filter(|provider| provider.supports(benchmark_types))
            .map(|provider| provider.descriptor())
            .collect()
    }

    pub fn run(
        &self,
        id: &str,
        store: &SqliteStore,
        experiment: ExperimentId,
        benchmark_types: &[(String, u32)],
        parameters: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<AnalysisResult, String> {
        let provider = self
            .providers
            .get(id)
            .ok_or_else(|| format!("Unknown analysis: {id}"))?;
        if !provider.supports(benchmark_types) {
            return Err(format!("Analysis {id:?} does not support this experiment"));
        }
        provider.run(store, experiment, parameters)
    }
}
