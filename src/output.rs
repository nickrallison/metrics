//! Filesystem locations shared by experiment execution and result viewing.

use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// One named experiment collection underneath a user-selected output directory.
/// The database persists across invocations; each invocation gets a timestamped
/// directory so retained files from earlier runs are never overwritten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputLayout {
    pub output_dir: PathBuf,
    pub experiment: String,
}

impl OutputLayout {
    pub fn experiment_directory(&self) -> Result<PathBuf> {
        let mut components = Path::new(&self.experiment).components();
        if self.output_dir.as_os_str().is_empty()
            || self.experiment.trim().is_empty()
            || self.experiment.contains(['/', '\\'])
            || self.experiment.chars().any(char::is_control)
            || !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(Error::Output("output directory must be nonempty and experiment name must be a single directory name".into()));
        }
        Ok(self.output_dir.join(&self.experiment))
    }

    pub fn database_path(&self) -> Result<PathBuf> {
        Ok(self.experiment_directory()?.join("database.sqlite"))
    }

    /// UTC RFC 3339, with fixed nanosecond precision and an explicit `Z` suffix.
    /// Atomic directory creation also protects simultaneous invocations. If the
    /// clock gives an existing timestamp, advance one nanosecond and retry.
    pub fn create_run_directory(&self) -> Result<PathBuf> {
        self.create_run_directory_at(SystemTime::now())
    }

    fn create_run_directory_at(&self, mut started_at: SystemTime) -> Result<PathBuf> {
        let experiment = self.experiment_directory()?;
        fs::create_dir_all(&experiment)?;
        loop {
            let timestamp: DateTime<Utc> = started_at.into();
            let directory = experiment.join(timestamp.to_rfc3339_opts(SecondsFormat::Nanos, true));
            match fs::create_dir(&directory) {
                Ok(()) => return Ok(directory.canonicalize()?),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    started_at = started_at
                        .checked_add(Duration::from_nanos(1))
                        .ok_or_else(|| Error::Output("run timestamp overflow".into()))?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn timestamp_directories_are_parseable_ordered_and_never_reused() {
        let root = tempfile::tempdir().unwrap();
        let output = OutputLayout {
            output_dir: root.path().into(),
            experiment: "manual-mcm".into(),
        };
        assert_eq!(
            output.database_path().unwrap(),
            root.path().join("manual-mcm/database.sqlite")
        );
        assert!(!root.path().join("manual-mcm").exists());
        let first = output.create_run_directory_at(UNIX_EPOCH).unwrap();
        let second = output.create_run_directory_at(UNIX_EPOCH).unwrap();
        assert_eq!(first.file_name().unwrap(), "1970-01-01T00:00:00.000000000Z");
        assert_eq!(
            second.file_name().unwrap(),
            "1970-01-01T00:00:00.000000001Z"
        );
        assert!(first < second);
        assert_eq!(
            first.parent(),
            Some(root.path().join("manual-mcm").as_path())
        );
        let parsed =
            DateTime::parse_from_rfc3339(first.file_name().unwrap().to_str().unwrap()).unwrap();
        assert_eq!(parsed.timestamp(), 0);
    }

    #[test]
    fn experiment_names_cannot_escape_the_output_directory() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "",
            " ",
            ".",
            "..",
            "../other",
            "/absolute",
            "nested/name",
            "nested\\name",
            "line\nbreak",
        ] {
            let output = OutputLayout {
                output_dir: root.path().into(),
                experiment: name.into(),
            };
            assert!(
                matches!(output.experiment_directory(), Err(Error::Output(_))),
                "{name:?}"
            );
        }
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
