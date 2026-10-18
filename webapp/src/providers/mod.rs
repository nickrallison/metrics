use std::{fs, path::PathBuf};

use benchmarks::store::{CatalogExperiment, ExperimentCatalog, SqliteStore};

pub fn catalog_path() -> PathBuf {
    std::env::var_os("BENCHMARKS_CATALOG_DATABASE")
        .map(PathBuf::from)
        .unwrap_or_else(|| output_root().join("catalog.sqlite"))
}

pub fn output_root() -> PathBuf {
    std::env::var_os("BENCHMARKS_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../output"))
}

/// Index existing stores once, then resolve all browser IDs through the central
/// catalog. The fallback discovers the standard output layout and older explicit
/// `BENCHMARKS_DATABASE` configuration.
pub fn experiments() -> Result<Vec<CatalogExperiment>, String> {
    let mut catalog = ExperimentCatalog::open(catalog_path()).map_err(|error| error.to_string())?;
    let mut databases = Vec::new();
    let root = output_root();
    if let Ok(children) = fs::read_dir(&root) {
        for child in children.flatten() {
            let path = child.path().join("database.sqlite");
            if path.is_file() {
                databases.push(path);
            }
        }
    }
    if let Some(path) = std::env::var_os("BENCHMARKS_DATABASE").map(PathBuf::from) {
        if path.is_file() {
            databases.push(path);
        }
    }

    databases.sort();
    databases.dedup();
    for database in databases {
        let store = SqliteStore::open(&database).map_err(|error| error.to_string())?;
        let label = database
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Experiment")
            .to_owned();
        for experiment in store.experiment_ids().map_err(|error| error.to_string())? {
            catalog
                .register(&database, experiment, &label)
                .map_err(|error| error.to_string())?;
        }
    }
    catalog.list().map_err(|error| error.to_string())
}

pub fn experiment(id: i64) -> Result<Option<CatalogExperiment>, String> {
    ExperimentCatalog::open(catalog_path())
        .and_then(|mut catalog| catalog.get(id))
        .map_err(|error| error.to_string())
}
