use std::{
    fs,
    path::{Path, PathBuf},
};

use diesel::{
    connection::SimpleConnection,
    prelude::*,
    sql_types::{BigInt, Text},
};

use crate::{Error, Result};

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

/// One catalog key mapped to an experiment in a separate SQLite store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogExperiment {
    pub id: i64,
    pub database: PathBuf,
    pub experiment: i64,
    pub label: String,
}

/// Central index of experiment databases. Experiment payloads remain in their
/// own stores; this database contains only locations and opaque catalog IDs.
pub struct ExperimentCatalog {
    connection: SqliteConnection,
}

#[derive(QueryableByName)]
struct CatalogRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
    #[diesel(sql_type = Text)]
    database: String,
    #[diesel(sql_type = BigInt)]
    experiment: i64,
    #[diesel(sql_type = Text)]
    label: String,
}

impl ExperimentCatalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let path = path
            .to_str()
            .ok_or_else(|| Error::Storage("catalog path must be UTF-8".into()))?;
        let mut connection = SqliteConnection::establish(path)?;
        connection.batch_execute("PRAGMA busy_timeout = 5000;")?;
        connection.batch_execute(
            "CREATE TABLE IF NOT EXISTS experiment_catalog (
                id INTEGER PRIMARY KEY,
                database_path TEXT NOT NULL,
                experiment_id INTEGER NOT NULL,
                label TEXT NOT NULL,
                UNIQUE(database_path, experiment_id)
            );",
        )?;
        Ok(Self { connection })
    }

    /// Add or refresh a pointer and return its stable catalog ID.
    pub fn register(
        &mut self,
        database: impl AsRef<Path>,
        experiment: i64,
        label: &str,
    ) -> Result<i64> {
        let database = database.as_ref();
        let database = database
            .canonicalize()
            .unwrap_or_else(|_| database.to_path_buf());
        let database = encode_path(&database)?;
        self.connection.transaction(|connection| {
            diesel::sql_query(
                "INSERT INTO experiment_catalog (database_path, experiment_id, label)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(database_path, experiment_id)
                 DO UPDATE SET label = excluded.label",
            )
            .bind::<Text, _>(&database)
            .bind::<BigInt, _>(experiment)
            .bind::<Text, _>(label)
            .execute(connection)?;
            let row = diesel::sql_query(
                "SELECT id, database_path AS database, experiment_id AS experiment, label
                 FROM experiment_catalog WHERE database_path = ?1 AND experiment_id = ?2",
            )
            .bind::<Text, _>(&database)
            .bind::<BigInt, _>(experiment)
            .get_result::<CatalogRow>(connection)?;
            Ok(row.id)
        })
    }

    pub fn list(&mut self) -> Result<Vec<CatalogExperiment>> {
        let rows = diesel::sql_query(
            "SELECT id, database_path AS database, experiment_id AS experiment, label
             FROM experiment_catalog ORDER BY label, experiment_id, id",
        )
        .load::<CatalogRow>(&mut self.connection)?;
        rows.into_iter().map(CatalogExperiment::try_from).collect()
    }

    pub fn get(&mut self, id: i64) -> Result<Option<CatalogExperiment>> {
        let row = diesel::sql_query(
            "SELECT id, database_path AS database, experiment_id AS experiment, label
             FROM experiment_catalog WHERE id = ?1",
        )
        .bind::<BigInt, _>(id)
        .get_result::<CatalogRow>(&mut self.connection)
        .optional()?;
        row.map(CatalogExperiment::try_from).transpose()
    }
}

impl TryFrom<CatalogRow> for CatalogExperiment {
    type Error = Error;

    fn try_from(row: CatalogRow) -> Result<Self> {
        Ok(Self {
            id: row.id,
            database: decode_path(&row.database)?,
            experiment: row.experiment,
            label: row.label,
        })
    }
}

#[cfg(unix)]
fn encode_path(path: &Path) -> Result<String> {
    Ok(format!(
        "unixhex:{}",
        hex_encode(path.as_os_str().as_bytes())
    ))
}

#[cfg(not(unix))]
fn encode_path(path: &Path) -> Result<String> {
    path.to_str()
        .map(|path| format!("utf8:{path}"))
        .ok_or_else(|| Error::Storage("experiment database path is not valid UTF-8".into()))
}

#[cfg(unix)]
fn decode_path(encoded: &str) -> Result<PathBuf> {
    let bytes = encoded.strip_prefix("unixhex:").ok_or_else(|| {
        Error::Storage("catalog path encoding does not match this platform".into())
    })?;
    Ok(PathBuf::from(std::ffi::OsString::from_vec(hex_decode(
        bytes,
    )?)))
}

#[cfg(not(unix))]
fn decode_path(encoded: &str) -> Result<PathBuf> {
    encoded
        .strip_prefix("utf8:")
        .map(PathBuf::from)
        .ok_or_else(|| Error::Storage("catalog path encoding does not match this platform".into()))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn hex_decode(encoded: &str) -> Result<Vec<u8>> {
    if encoded.len() % 2 != 0 {
        return Err(Error::Storage(
            "invalid hex path in experiment catalog".into(),
        ));
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                b'A'..=b'F' => Some(byte - b'A' + 10),
                _ => None,
            };
            let high = digit(pair[0])?;
            let low = digit(pair[1])?;
            Some((high << 4) | low)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Error::Storage("invalid hex path in experiment catalog".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_stably_point_to_experiment_databases() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join("experiment.sqlite");
        fs::write(&database, []).unwrap();
        let catalog_path = root.path().join("catalog.sqlite");
        let mut catalog = ExperimentCatalog::open(&catalog_path).unwrap();

        let first_id = catalog.register(&database, 7, "suite-a").unwrap();
        let duplicate_id = catalog.register(&database, 7, "suite-a").unwrap();
        catalog.register(&database, 8, "suite-a").unwrap();

        assert_eq!(first_id, duplicate_id);
        assert_eq!(catalog.get(first_id).unwrap().unwrap().experiment, 7);
        let entries = catalog.list().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .all(|entry| entry.database == database.canonicalize().unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn catalog_preserves_non_utf8_database_paths() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join(std::ffi::OsString::from_vec(
            b"experiment-\xff.sqlite".to_vec(),
        ));
        fs::write(&database, []).unwrap();
        let mut catalog = ExperimentCatalog::open(root.path().join("catalog.sqlite")).unwrap();

        let id = catalog.register(&database, 2, "suite").unwrap();

        assert_eq!(
            catalog.get(id).unwrap().unwrap().database,
            database.canonicalize().unwrap()
        );
    }
}
