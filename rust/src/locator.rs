use crate::error::{OpenPageError, OpenPageResult};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LocatorKind {
    Css,
    XPath,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Locator {
    raw: String,
    kind: LocatorKind,
    query: String,
}

impl Locator {
    pub fn parse(raw: impl AsRef<str>) -> OpenPageResult<Self> {
        let raw = raw.as_ref().trim();
        if raw.is_empty() {
            return Err(OpenPageError::UnsupportedLocator(
                "empty locator is not allowed".to_string(),
            ));
        }

        if let Some(query) = raw.strip_prefix("css:") {
            return Ok(Self::new(raw, LocatorKind::Css, query.trim()));
        }

        if let Some(query) = raw.strip_prefix("xpath:") {
            return Ok(Self::new(raw, LocatorKind::XPath, query.trim()));
        }

        if let Some(query) = raw.strip_prefix("tag:") {
            return Ok(Self::new(raw, LocatorKind::Css, query.trim()));
        }

        if let Some(query) = raw.strip_prefix("t:") {
            return Ok(Self::new(raw, LocatorKind::Css, query.trim()));
        }

        if let Some(query) = raw.strip_prefix('@') {
            let (name, value) = query.split_once('=').ok_or_else(|| {
                OpenPageError::UnsupportedLocator(format!(
                    "attribute locator requires @name=value form: {raw}"
                ))
            })?;
            let name = name.trim();
            let value = value.trim();
            let css = match name {
                "id" => format!("#{}", css_escape_ident(value)),
                "class" => format!(".{}", css_escape_ident(value)),
                _ => format!(r#"[{}="{}"]"#, name, css_escape_string(value)),
            };
            return Ok(Self::new(raw, LocatorKind::Css, css));
        }

        Ok(Self::new(raw, LocatorKind::Css, raw))
    }

    pub fn kind(&self) -> LocatorKind {
        self.kind
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    fn new(raw: impl Into<String>, kind: LocatorKind, query: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            kind,
            query: query.into(),
        }
    }
}

fn css_escape_ident(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('.', "\\.")
        .replace('#', "\\#")
        .replace(':', "\\:")
        .replace(' ', "\\ ")
}

fn css_escape_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}
