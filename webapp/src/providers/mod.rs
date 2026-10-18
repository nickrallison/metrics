mod mcm;

use crate::analysis::AnalysisRegistry;

pub fn registry() -> AnalysisRegistry<String> {
    let mut registry = AnalysisRegistry::default();
    registry.register(mcm::McmPerformance);
    registry
}

pub(super) fn database_path() -> std::path::PathBuf {
    std::env::var_os("BENCHMARKS_DATABASE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../output/mcm/database.sqlite")
        })
}
