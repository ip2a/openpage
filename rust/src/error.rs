use thiserror::Error;

pub type OpenPageResult<T> = Result<T, OpenPageError>;

#[derive(Debug, Error)]
pub enum OpenPageError {
    #[error("browser launch failed: {0}")]
    BrowserLaunch(String),
    #[error("browser operation failed: {0}")]
    BrowserOperation(String),
    #[error("page operation failed: {0}")]
    PageOperation(String),
    #[error("element not found: {0}")]
    ElementNotFound(String),
    #[error("unsupported locator syntax: {0}")]
    UnsupportedLocator(String),
    #[error("javascript evaluation failed: {0}")]
    JavaScript(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("timeout waiting for locator: {0}")]
    Timeout(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<std::io::Error> for OpenPageError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
