use crate::error::{OpenPageError, OpenPageResult};

#[derive(Debug, Clone)]
pub struct LocatorMatch<T> {
    pub locator: String,
    pub elements: Vec<T>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LocatorKind {
    Css,
    XPath,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LocatorInput<'a> {
    Raw(&'a str),
    By(&'a str, &'a str),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LocatorBatchInput<'a> {
    Single(LocatorInput<'a>),
    Many(Vec<LocatorInput<'a>>),
}

impl<'a> From<&'a str> for LocatorInput<'a> {
    fn from(value: &'a str) -> Self {
        Self::Raw(value)
    }
}

impl<'a> From<&'a String> for LocatorInput<'a> {
    fn from(value: &'a String) -> Self {
        Self::Raw(value.as_str())
    }
}

impl<'a> From<(&'a str, &'a str)> for LocatorInput<'a> {
    fn from(value: (&'a str, &'a str)) -> Self {
        Self::By(value.0, value.1)
    }
}

impl<'a> From<&'a str> for LocatorBatchInput<'a> {
    fn from(value: &'a str) -> Self {
        Self::Single(LocatorInput::Raw(value))
    }
}

impl<'a> From<&'a String> for LocatorBatchInput<'a> {
    fn from(value: &'a String) -> Self {
        Self::Single(LocatorInput::Raw(value.as_str()))
    }
}

impl<'a> From<(&'a str, &'a str)> for LocatorBatchInput<'a> {
    fn from(value: (&'a str, &'a str)) -> Self {
        Self::Single(LocatorInput::By(value.0, value.1))
    }
}

impl<'a> From<&'a [String]> for LocatorBatchInput<'a> {
    fn from(value: &'a [String]) -> Self {
        Self::Many(
            value
                .iter()
                .map(|item| LocatorInput::Raw(item.as_str()))
                .collect(),
        )
    }
}

impl<'a> From<&'a Vec<String>> for LocatorBatchInput<'a> {
    fn from(value: &'a Vec<String>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a [LocatorInput<'a>]> for LocatorBatchInput<'a> {
    fn from(value: &'a [LocatorInput<'a>]) -> Self {
        Self::Many(value.to_vec())
    }
}

impl<'a> From<&'a Vec<LocatorInput<'a>>> for LocatorBatchInput<'a> {
    fn from(value: &'a Vec<LocatorInput<'a>>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<Vec<LocatorInput<'a>>> for LocatorBatchInput<'a> {
    fn from(value: Vec<LocatorInput<'a>>) -> Self {
        Self::Many(value)
    }
}

impl<'a, const N: usize> From<[LocatorInput<'a>; N]> for LocatorBatchInput<'a> {
    fn from(value: [LocatorInput<'a>; N]) -> Self {
        Self::Many(value.into_iter().collect())
    }
}

impl<'a, const N: usize> From<&'a [LocatorInput<'a>; N]> for LocatorBatchInput<'a> {
    fn from(value: &'a [LocatorInput<'a>; N]) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<&'a [(&'a str, &'a str)]> for LocatorBatchInput<'a> {
    fn from(value: &'a [(&'a str, &'a str)]) -> Self {
        Self::Many(value.iter().copied().map(LocatorInput::from).collect())
    }
}

impl<'a> From<&'a Vec<(&'a str, &'a str)>> for LocatorBatchInput<'a> {
    fn from(value: &'a Vec<(&'a str, &'a str)>) -> Self {
        Self::from(value.as_slice())
    }
}

impl<'a> From<Vec<(&'a str, &'a str)>> for LocatorBatchInput<'a> {
    fn from(value: Vec<(&'a str, &'a str)>) -> Self {
        Self::Many(value.into_iter().map(LocatorInput::from).collect())
    }
}

impl<'a, const N: usize> From<[(&'a str, &'a str); N]> for LocatorBatchInput<'a> {
    fn from(value: [(&'a str, &'a str); N]) -> Self {
        Self::Many(value.into_iter().map(LocatorInput::from).collect())
    }
}

impl<'a, const N: usize> From<&'a [(&'a str, &'a str); N]> for LocatorBatchInput<'a> {
    fn from(value: &'a [(&'a str, &'a str); N]) -> Self {
        Self::from(value.as_slice())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Locator {
    raw: String,
    kind: LocatorKind,
    query: String,
}

impl Locator {
    pub fn from_input<'a, L>(input: L) -> OpenPageResult<Self>
    where
        L: Into<LocatorInput<'a>>,
    {
        match input.into() {
            LocatorInput::Raw(raw) => Self::parse(raw),
            LocatorInput::By(by, value) => Self::from_by(by, value),
        }
    }

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

        if let Some(query) = raw.strip_prefix("text=") {
            let query = query.trim();
            if query.is_empty() {
                return Err(OpenPageError::UnsupportedLocator(
                    "text locator requires non-empty text".to_string(),
                ));
            }
            let text = xpath_string_literal(query);
            let xpath = format!(
                ".//*[contains(normalize-space(.), {text}) and not(.//*[contains(normalize-space(.), {text})])]"
            );
            return Ok(Self::new(raw, LocatorKind::XPath, xpath));
        }

        if let Some(query) = raw.strip_prefix('@') {
            let query = query.trim();
            if query.starts_with('e')
                && query.len() > 1
                && query[1..].chars().all(|c| c.is_ascii_digit())
            {
                let selector = format!(r#"[data-op-ref="{}"]"#, query);
                return Ok(Self::new(raw, LocatorKind::Css, selector));
            }
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

    pub fn from_by(by: &str, value: &str) -> OpenPageResult<Self> {
        let by = by.trim().to_ascii_lowercase();
        let raw = match by.as_str() {
            "xpath" => format!("xpath:{value}"),
            "css selector" => format!("css:{value}"),
            "id" => format!("@id={value}"),
            "class name" => format!("@class={value}"),
            "name" => format!("@name={value}"),
            "tag name" => format!("tag:{value}"),
            "link text" => format!("xpath://a[text()={}]", xpath_string_literal(value)),
            "partial link text" => {
                format!(
                    "xpath://a[contains(text(), {})]",
                    xpath_string_literal(value)
                )
            }
            _ => {
                return Err(OpenPageError::UnsupportedLocator(format!(
                    "unsupported By locator: {by}"
                )));
            }
        };
        Self::parse(raw)
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

fn xpath_string_literal(value: &str) -> String {
    if !value.contains('"') {
        return format!("\"{value}\"");
    }
    if !value.contains('\'') {
        return format!("'{value}'");
    }

    let segments = value.split('"').collect::<Vec<_>>();
    let mut parts = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        if !segment.is_empty() {
            parts.push(format!("\"{segment}\""));
        }
        if index + 1 < segments.len() {
            parts.push("'\"'".to_string());
        }
    }
    format!("concat({})", parts.join(", "))
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

pub(crate) fn collect_locator_matches<T, F>(
    locators: &[String],
    any_one: bool,
    first_match_only: bool,
    mut finder: F,
) -> OpenPageResult<Vec<LocatorMatch<T>>>
where
    F: FnMut(&str) -> OpenPageResult<Vec<T>>,
{
    let mut results = Vec::new();
    for locator in locators {
        let mut elements = finder(locator)?;
        if first_match_only && elements.len() > 1 {
            elements.truncate(1);
        }

        if any_one {
            if !elements.is_empty() {
                results.push(LocatorMatch {
                    locator: locator.clone(),
                    elements,
                });
                break;
            }
        } else {
            results.push(LocatorMatch {
                locator: locator.clone(),
                elements,
            });
        }
    }
    Ok(results)
}

pub(crate) fn parse_optional_locator_input<'a, L>(
    locator: Option<L>,
) -> OpenPageResult<Option<Locator>>
where
    L: Into<LocatorInput<'a>>,
{
    locator.map(Locator::from_input).transpose()
}

pub(crate) fn parse_locator_batch_input<'a, L>(locators: L) -> OpenPageResult<Vec<String>>
where
    L: Into<LocatorBatchInput<'a>>,
{
    match locators.into() {
        LocatorBatchInput::Single(locator) => {
            Ok(vec![Locator::from_input(locator)?.raw().to_string()])
        }
        LocatorBatchInput::Many(locators) => locators
            .into_iter()
            .map(Locator::from_input)
            .map(|locator| locator.map(|locator| locator.raw().to_string()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Locator, LocatorBatchInput, LocatorInput, LocatorKind};

    #[test]
    fn from_input_accepts_raw_and_by_forms() {
        let css = Locator::from_input(".item").expect("raw css");
        let by = Locator::from_input(("id", "main")).expect("by id");

        assert_eq!(css.kind(), LocatorKind::Css);
        assert_eq!(css.raw(), ".item");
        assert_eq!(by.raw(), "@id=main");
    }

    #[test]
    fn locator_input_from_tuple_keeps_components() {
        let input = LocatorInput::from(("class name", "item"));

        assert_eq!(input, LocatorInput::By("class name", "item"));
    }

    #[test]
    fn locator_batch_input_accepts_single_and_string_lists() {
        let single = super::parse_locator_batch_input(("id", "main")).expect("single by");
        let items = vec!["#one".to_string(), "xpath://div".to_string()];
        let list = super::parse_locator_batch_input(&items).expect("string list");

        assert_eq!(single, vec!["@id=main".to_string()]);
        assert_eq!(list, vec!["#one".to_string(), "xpath://div".to_string()]);
        assert_eq!(
            LocatorBatchInput::from(&items),
            LocatorBatchInput::Many(vec![
                LocatorInput::Raw("#one"),
                LocatorInput::Raw("xpath://div"),
            ])
        );
    }

    #[test]
    fn locator_batch_input_accepts_tuple_and_mixed_lists() {
        let tuple_list = [("id", "main"), ("class name", "item")];
        let mixed_list = [
            LocatorInput::from("#one"),
            LocatorInput::from(("xpath", "//div")),
        ];

        let tuples = super::parse_locator_batch_input(&tuple_list).expect("tuple list");
        let mixed = super::parse_locator_batch_input(&mixed_list).expect("mixed list");

        assert_eq!(
            tuples,
            vec!["@id=main".to_string(), "@class=item".to_string()]
        );
        assert_eq!(mixed, vec!["#one".to_string(), "xpath://div".to_string()]);
        assert_eq!(
            LocatorBatchInput::from(&mixed_list),
            LocatorBatchInput::Many(vec![
                LocatorInput::Raw("#one"),
                LocatorInput::By("xpath", "//div"),
            ])
        );
    }

    #[test]
    fn parse_optional_locator_input_accepts_optional_by_tuple() {
        let locator =
            super::parse_optional_locator_input(Some(("id", "main"))).expect("optional by");

        assert_eq!(locator.expect("locator").raw(), "@id=main");
    }

    #[test]
    fn parse_ref_locator_to_data_attr() {
        let locator = Locator::parse("@e5").expect("ref locator");
        assert_eq!(locator.kind(), LocatorKind::Css);
        assert_eq!(locator.query(), r#"[data-op-ref="e5"]"#);
    }

    #[test]
    fn parse_text_locator_targets_smallest_text_element() {
        let locator = Locator::parse("text=Learn more").expect("text locator");

        assert_eq!(locator.kind(), LocatorKind::XPath);
        assert!(locator.query().contains("normalize-space(.)"));
        assert!(locator.query().contains("not(.//*"));
    }

    #[test]
    fn from_by_maps_basic_strategies() {
        let css = Locator::from_by("css selector", ".item").expect("css selector");
        let id = Locator::from_by("id", "main").expect("id");
        let tag = Locator::from_by("tag name", "div").expect("tag name");

        assert_eq!(css.kind(), LocatorKind::Css);
        assert_eq!(css.query(), ".item");
        assert_eq!(id.query(), "#main");
        assert_eq!(tag.query(), "div");
    }

    #[test]
    fn from_by_maps_link_text_with_mixed_quotes() {
        let locator = Locator::from_by("link text", "a\"b'c").expect("link text");

        assert_eq!(locator.kind(), LocatorKind::XPath);
        assert!(locator.query().contains("concat("));
    }

    #[test]
    fn from_by_rejects_unknown_values() {
        let error = Locator::from_by("unsupported", "demo").expect_err("unsupported by");

        match error {
            crate::OpenPageError::UnsupportedLocator(message) => {
                assert!(message.contains("unsupported By locator"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
