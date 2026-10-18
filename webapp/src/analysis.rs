//! Registry boundary for typed, server-side analyses.
//!
//! Implementations can query `Store` with their concrete output type and use
//! `Analysis<FnOnce(Frame<_>)>` internally. Only the serializable contract in
//! `presentation` crosses to the browser.

use std::collections::BTreeMap;

use crate::presentation::{AnalysisDescriptor, AnalysisRequest, AnalysisResult};

pub trait AnalysisProvider<E>: Send + Sync {
    fn descriptor(&self) -> AnalysisDescriptor;

    fn supports(&self, _benchmark_types: &[(String, u32)]) -> bool {
        true
    }

    fn run(&self, request: &AnalysisRequest<E>) -> Result<AnalysisResult, String>;
}

#[derive(Default)]
pub struct AnalysisRegistry<E> {
    providers: BTreeMap<String, Box<dyn AnalysisProvider<E>>>,
}

impl<E> AnalysisRegistry<E> {
    pub fn register(&mut self, provider: impl AnalysisProvider<E> + 'static) {
        let id = provider.descriptor().id;
        self.providers.insert(id, Box::new(provider));
    }

    pub fn descriptors(&self) -> Vec<AnalysisDescriptor> {
        self.providers
            .values()
            .map(|provider| provider.descriptor())
            .collect()
    }

    pub fn descriptors_for(&self, benchmark_types: &[(String, u32)]) -> Vec<AnalysisDescriptor> {
        self.providers
            .values()
            .filter(|provider| provider.supports(benchmark_types))
            .map(|provider| provider.descriptor())
            .collect()
    }

    pub fn run(&self, request: &AnalysisRequest<E>) -> Result<AnalysisResult, String> {
        let provider = self
            .providers
            .get(&request.analysis)
            .ok_or_else(|| format!("Unknown analysis: {}", request.analysis))?;
        provider.run(request)
    }
}
