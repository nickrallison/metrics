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
    Sqlite(#[from] diesel::result::Error),
    #[error("{0}")]
    DatabaseConnection(#[from] diesel::ConnectionError),
    #[error("invalid storage state: {0}")]
    Storage(String),
    #[error("experiment {0} does not exist")]
    UnknownExperiment(i64),
    #[error("commit {commit} is not registered in experiment {experiment}")]
    UnregisteredCommit { experiment: i64, commit: String },
    #[error("workload {index} is not registered in experiment {experiment}")]
    UnregisteredBenchmark { experiment: i64, index: usize },
    #[error("duplicate selected commit {0}")]
    DuplicateCommit(String),
    #[error("commit position {position} is already registered in experiment {experiment}")]
    DuplicateCommitPosition { experiment: i64, position: usize },
    #[error("conflicting metadata for commit {0}")]
    CommitMetadataMismatch(String),
    #[error("workload {index} differs from the frozen definition in experiment {experiment}")]
    BenchmarkMismatch { experiment: i64, index: usize },
    #[error("inspection source {0} must be a finished measurement of the same workload and commit")]
    InvalidInspectionSource(i64),
    #[error("attempt {0} does not exist")]
    UnknownRun(i64),
    #[error("attempt {0} is already finished")]
    RunAlreadyFinished(i64),
    #[error("position {0} exceeds SQLite integer range")]
    PositionOutOfRange(usize),
    #[error("invalid view data: {0}")]
    InvalidView(String),
    #[error("benchmark error: {0}")]
    Benchmark(String),
    #[error("profiling error: {0}")]
    Profiling(String),
    #[error("invalid output configuration: {0}")]
    Output(String),
    #[error("{command} failed ({status}): {detail}")]
    Process {
        command: String,
        status: std::process::ExitStatus,
        detail: String,
    },
}
