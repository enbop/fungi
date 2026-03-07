use hyper::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DockerAgentError {
    #[error("invalid container spec: {0}")]
    InvalidSpec(String),
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("docker api error ({status}): {message}")]
    DockerApi { status: StatusCode, message: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("http error: {0}")]
    Http(#[from] hyper::Error),
    #[error("http request error: {0}")]
    HttpRequest(#[from] hyper::http::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, DockerAgentError>;