pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Serialization(#[from] serde_json::Error),
}
