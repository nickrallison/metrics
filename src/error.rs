pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("git {command} failed in {repository:?} ({status}): {stderr}")]
    Git {
        repository: std::path::PathBuf,
        command: String,
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error("invalid revision selection: {0}")]
    InvalidRevision(String),
    #[error("invalid Git output: {0}")]
    InvalidGitOutput(String),
    #[error("invalid Yosys build cache: {0}")]
    BuildCache(String),
    #[error("Yosys build for {commit} failed during {stage}: {detail}; log: {log:?}")]
    YosysBuild {
        commit: String,
        stage: &'static str,
        detail: String,
        log: std::path::PathBuf,
    },
    #[error("{0}")]
    Serialization(#[from] serde_json::Error),
    #[error("{0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("invalid storage state: {0}")]
    Storage(String),
}
