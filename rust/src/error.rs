use std::fmt;

use crate::settings::localized_error_with_detail;

pub type OpenPageResult<T> = Result<T, OpenPageError>;

#[derive(Debug)]
pub enum OpenPageError {
    BrowserLaunch(String),
    BrowserOperation(String),
    PageOperation(String),
    ElementNotFound(String),
    UnsupportedLocator(String),
    UnsupportedOperation(String),
    JavaScript(String),
    Http(String),
    Io(String),
    Timeout(String),
    Serialization(String),
}

impl fmt::Display for OpenPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::BrowserLaunch(detail) => {
                localized_error_with_detail("browser launch failed", "浏览器启动失败", detail)
            }
            Self::BrowserOperation(detail) => {
                localized_error_with_detail("browser operation failed", "浏览器操作失败", detail)
            }
            Self::PageOperation(detail) => {
                localized_error_with_detail("page operation failed", "页面操作失败", detail)
            }
            Self::ElementNotFound(detail) => {
                localized_error_with_detail("element not found", "没有找到元素", detail)
            }
            Self::UnsupportedLocator(detail) => localized_error_with_detail(
                "unsupported locator syntax",
                "定位符语法不受支持",
                detail,
            ),
            Self::UnsupportedOperation(detail) => {
                localized_error_with_detail("unsupported operation", "不支持的操作", detail)
            }
            Self::JavaScript(detail) => localized_error_with_detail(
                "javascript evaluation failed",
                "JavaScript 执行失败",
                detail,
            ),
            Self::Http(detail) => {
                localized_error_with_detail("http operation failed", "HTTP 操作失败", detail)
            }
            Self::Io(detail) => localized_error_with_detail("io error", "IO 错误", detail),
            Self::Timeout(detail) => {
                localized_error_with_detail("timeout waiting for locator", "等待超时", detail)
            }
            Self::Serialization(detail) => {
                localized_error_with_detail("serialization error", "序列化错误", detail)
            }
        };
        f.write_str(&message)
    }
}

impl std::error::Error for OpenPageError {}

impl From<std::io::Error> for OpenPageError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
