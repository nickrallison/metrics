//! Concrete benchmarks and their suites and views.

pub mod mcm;
pub mod synth;

/// Register analysis adapters supplied by compiled benchmark modules.
pub fn register_analyses(registry: &mut crate::analysis::AnalysisRegistry) {
    registry.register(mcm::McmPerformance);
}
