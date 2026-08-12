use std::fmt;

use crate::settings::localized_error_with_detail;

pub type OpenPageResult<T> = Result<T, OpenPageError>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorDiagnostic {
    pub operation: Option<String>,
    pub locator: Option<String>,
    pub url: Option<String>,
    pub timeout_ms: Option<u64>,
    pub matched_count: Option<usize>,
    pub element_state: Option<String>,
    pub failure_reason: Option<String>,
    pub current_revision: Option<String>,
    pub expected_revision: Option<String>,
}

#[derive(Debug)]
pub enum OpenPageError {
    BrowserLaunch(String),
    BrowserOperation(String),
    PageOperation(String),
    ElementNotFound(String),
    ElementDetached(String),
    ElementAmbiguous(String),
    UnsupportedLocator(String),
    UnsupportedOperation(String),
    JavaScript(String),
    Http(String),
    Io(String),
    Timeout(String),
    Serialization(String),
    Diagnosed {
        error: Box<OpenPageError>,
        diagnostic: ErrorDiagnostic,
    },
}

impl OpenPageError {
    pub fn diagnosed(self, diagnostic: ErrorDiagnostic) -> Self {
        Self::Diagnosed {
            error: Box::new(self),
            diagnostic,
        }
    }

    pub fn diagnostic(&self) -> Option<&ErrorDiagnostic> {
        match self {
            Self::Diagnosed { diagnostic, .. } => Some(diagnostic),
            _ => None,
        }
    }

    pub fn root(&self) -> &Self {
        match self {
            Self::Diagnosed { error, .. } => error.root(),
            error => error,
        }
    }
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
            Self::ElementDetached(detail) => {
                localized_error_with_detail("element is detached", "元素已失效", detail)
            }
            Self::ElementAmbiguous(detail) => localized_error_with_detail(
                "element relocation is ambiguous",
                "元素重新定位存在歧义",
                detail,
            ),
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
            Self::Diagnosed { error, .. } => return error.fmt(f),
        };
        f.write_str(&message)
    }
}

impl std::error::Error for OpenPageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Diagnosed { error, .. } => Some(error.as_ref()),
            _ => None,
        }
    }
}

impl From<std::io::Error> for OpenPageError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnosed_error_preserves_root_kind_and_fields() {
        let error =
            OpenPageError::ElementNotFound("missing".to_string()).diagnosed(ErrorDiagnostic {
                operation: Some("click".to_string()),
                locator: Some("#submit".to_string()),
                url: Some("https://example.com/login".to_string()),
                timeout_ms: Some(10_000),
                matched_count: Some(0),
                element_state: Some("not actionable".to_string()),
                failure_reason: Some("missing".to_string()),
                current_revision: Some("r_2".to_string()),
                expected_revision: Some("r_1".to_string()),
            });

        assert!(matches!(error.root(), OpenPageError::ElementNotFound(_)));
        let diagnostic = error.diagnostic().expect("diagnostic");
        assert_eq!(diagnostic.operation.as_deref(), Some("click"));
        assert_eq!(diagnostic.locator.as_deref(), Some("#submit"));
        assert_eq!(diagnostic.timeout_ms, Some(10_000));
        assert_eq!(diagnostic.matched_count, Some(0));
        assert_eq!(diagnostic.current_revision.as_deref(), Some("r_2"));
        assert_eq!(diagnostic.expected_revision.as_deref(), Some("r_1"));
        assert_eq!(error.to_string(), "element not found: missing");
    }
}
