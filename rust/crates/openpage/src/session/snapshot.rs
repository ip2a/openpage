use super::*;

pub fn snapshot_root(html: &str) -> OpenPageResult<DocumentElement> {
    snapshot_root_arc(Arc::new(html.to_string()), None, None)
}

pub fn snapshot_find(html: &str, locator: &str) -> OpenPageResult<DocumentElement> {
    snapshot_find_arc(Arc::new(html.to_string()), locator, None, None)
}

pub fn snapshot_find_all(html: &str, locator: &str) -> OpenPageResult<Vec<DocumentElement>> {
    snapshot_find_all_arc(Arc::new(html.to_string()), locator, None, None)
}

pub fn snapshot_query_xpath(
    html: &str,
    expression: &str,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    snapshot_query_xpath_arc(Arc::new(html.to_string()), expression, None, None)
}

pub fn snapshot_fragment_root(html: &str) -> OpenPageResult<DocumentElement> {
    snapshot_fragment_root_arc(Arc::new(html.to_string()), None, None)
}

pub fn snapshot_fragment_find(html: &str, locator: &str) -> OpenPageResult<DocumentElement> {
    snapshot_fragment_find_arc(Arc::new(html.to_string()), locator, None, None)
}

pub fn snapshot_fragment_find_all(
    html: &str,
    locator: &str,
) -> OpenPageResult<Vec<DocumentElement>> {
    snapshot_fragment_find_all_arc(Arc::new(html.to_string()), locator, None, None)
}

pub fn snapshot_fragment_query_xpath(
    html: &str,
    expression: &str,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    snapshot_fragment_query_xpath_arc(Arc::new(html.to_string()), expression, None)
}

pub fn snapshot_root_with_base_url(
    html: &str,
    base_url: Option<&str>,
) -> OpenPageResult<DocumentElement> {
    snapshot_root_arc(
        Arc::new(html.to_string()),
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_find_with_base_url(
    html: &str,
    locator: &str,
    base_url: Option<&str>,
) -> OpenPageResult<DocumentElement> {
    snapshot_find_arc(
        Arc::new(html.to_string()),
        locator,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_find_all_with_base_url(
    html: &str,
    locator: &str,
    base_url: Option<&str>,
) -> OpenPageResult<Vec<DocumentElement>> {
    snapshot_find_all_arc(
        Arc::new(html.to_string()),
        locator,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_query_xpath_with_base_url(
    html: &str,
    expression: &str,
    base_url: Option<&str>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    snapshot_query_xpath_arc(
        Arc::new(html.to_string()),
        expression,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_fragment_root_with_base_url(
    html: &str,
    base_url: Option<&str>,
) -> OpenPageResult<DocumentElement> {
    snapshot_fragment_root_arc(
        Arc::new(html.to_string()),
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_fragment_find_with_base_url(
    html: &str,
    locator: &str,
    base_url: Option<&str>,
) -> OpenPageResult<DocumentElement> {
    snapshot_fragment_find_arc(
        Arc::new(html.to_string()),
        locator,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_fragment_find_all_with_base_url(
    html: &str,
    locator: &str,
    base_url: Option<&str>,
) -> OpenPageResult<Vec<DocumentElement>> {
    snapshot_fragment_find_all_arc(
        Arc::new(html.to_string()),
        locator,
        base_url.map(|value| Arc::new(value.to_string())),
        None,
    )
}

pub fn snapshot_fragment_query_xpath_with_base_url(
    html: &str,
    expression: &str,
    base_url: Option<&str>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    snapshot_fragment_query_xpath_arc(
        Arc::new(html.to_string()),
        expression,
        base_url.map(|value| Arc::new(value.to_string())),
    )
}

pub(super) fn snapshot_root_arc(
    html: Arc<String>,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<DocumentElement> {
    let parsed = Html::parse_document(html.as_ref());
    Ok(session_element_from_ref(
        &html,
        base_url.as_ref(),
        parsed.root_element(),
        none_element_config,
    ))
}

pub(super) fn snapshot_find_arc(
    html: Arc<String>,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<DocumentElement> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(html.as_ref());
            let selector = parse_selector_query(locator.query())?;
            parsed
                .select(&selector)
                .next()
                .map(|element| {
                    session_element_from_ref(&html, base_url.as_ref(), element, none_element_config)
                })
                .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string()))
        }
        LocatorKind::XPath => xpath_find_all_with_scope(
            &html,
            base_url.as_ref(),
            locator.query(),
            None,
            false,
            none_element_config,
        )?
        .into_iter()
        .next()
        .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string())),
    }
}

pub(super) fn snapshot_find_all_arc(
    html: Arc<String>,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<DocumentElement>> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(html.as_ref());
            let selector = parse_selector_query(locator.query())?;
            Ok(parsed
                .select(&selector)
                .map(|element| {
                    session_element_from_ref(&html, base_url.as_ref(), element, none_element_config)
                })
                .collect())
        }
        LocatorKind::XPath => xpath_find_all_with_scope(
            &html,
            base_url.as_ref(),
            locator.query(),
            None,
            false,
            none_element_config,
        ),
    }
}

pub(super) fn snapshot_query_xpath_arc(
    html: Arc<String>,
    expression: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    xpath_query_with_scope(
        &html,
        base_url.as_ref(),
        expression,
        None,
        false,
        none_element_config,
    )
}

pub(super) fn snapshot_fragment_root_arc(
    html: Arc<String>,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<DocumentElement> {
    let wrapped = wrap_fragment_html(html);
    let parsed = Html::parse_document(wrapped.as_ref());
    let wrapper_selector = Selector::parse(&format!("[{FRAGMENT_WRAPPER_ATTR}='1']"))
        .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
    let mut element = parsed.select(&wrapper_selector).next().ok_or_else(|| {
        OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message())
    })?;
    while element.attr(FRAGMENT_WRAPPER_ATTR).is_some() {
        element = element.child_elements().next().ok_or_else(|| {
            OpenPageError::ElementNotFound(snapshot_fragment_root_not_found_message())
        })?;
    }
    Ok(session_element_from_ref(
        &wrapped,
        base_url.as_ref(),
        element,
        none_element_config,
    ))
}

pub(super) fn snapshot_fragment_find_arc(
    html: Arc<String>,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<DocumentElement> {
    let wrapped = wrap_fragment_html(html);
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(wrapped.as_ref());
            let selector = parse_selector_query(locator.query())?;
            let wrapper_selector = Selector::parse(&format!("[{FRAGMENT_WRAPPER_ATTR}='1']"))
                .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
            parsed
                .select(&wrapper_selector)
                .next()
                .ok_or_else(|| {
                    OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message())
                })?
                .select(&selector)
                .next()
                .map(|element| {
                    session_element_from_ref(
                        &wrapped,
                        base_url.as_ref(),
                        element,
                        none_element_config,
                    )
                })
                .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string()))
        }
        LocatorKind::XPath => xpath_find_all_with_scope(
            &wrapped,
            base_url.as_ref(),
            locator.query(),
            None,
            true,
            none_element_config,
        )?
        .into_iter()
        .next()
        .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string())),
    }
}

pub(super) fn snapshot_fragment_find_all_arc(
    html: Arc<String>,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<DocumentElement>> {
    let wrapped = wrap_fragment_html(html);
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(wrapped.as_ref());
            let selector = parse_selector_query(locator.query())?;
            let wrapper_selector = Selector::parse(&format!("[{FRAGMENT_WRAPPER_ATTR}='1']"))
                .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
            let wrapper = parsed.select(&wrapper_selector).next().ok_or_else(|| {
                OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message())
            })?;
            Ok(wrapper
                .select(&selector)
                .map(|element| {
                    session_element_from_ref(
                        &wrapped,
                        base_url.as_ref(),
                        element,
                        none_element_config,
                    )
                })
                .collect())
        }
        LocatorKind::XPath => xpath_find_all_with_scope(
            &wrapped,
            base_url.as_ref(),
            locator.query(),
            None,
            true,
            none_element_config,
        ),
    }
}

pub(super) fn snapshot_fragment_query_xpath_arc(
    html: Arc<String>,
    expression: &str,
    base_url: Option<Arc<String>>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    let wrapped = wrap_fragment_html(html);
    xpath_query_with_scope(&wrapped, base_url.as_ref(), expression, None, true, None)
}

pub(super) fn find_in_scope(
    html: Arc<String>,
    scope_id: NodeId,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<DocumentElement> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(html.as_ref());
            let scope = element_from_document(&parsed, scope_id)?;
            let selector = parse_selector_query(locator.query())?;
            scope
                .select(&selector)
                .next()
                .map(|element| {
                    session_element_from_ref(&html, base_url.as_ref(), element, none_element_config)
                })
                .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string()))
        }
        LocatorKind::XPath => {
            let parsed = Html::parse_document(html.as_ref());
            let scope = element_from_document(&parsed, scope_id)?;
            xpath_find_all_from_scope_element(
                &html,
                base_url.as_ref(),
                scope,
                locator.query(),
                none_element_config,
            )?
            .into_iter()
            .next()
            .ok_or_else(|| OpenPageError::ElementNotFound(locator.raw().to_string()))
        }
    }
}

pub(super) fn find_all_in_scope(
    html: Arc<String>,
    scope_id: NodeId,
    locator: &str,
    base_url: Option<Arc<String>>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<DocumentElement>> {
    let locator = Locator::parse(locator)?;
    match locator.kind() {
        LocatorKind::Css => {
            let parsed = Html::parse_document(html.as_ref());
            let scope = element_from_document(&parsed, scope_id)?;
            let selector = parse_selector_query(locator.query())?;
            Ok(scope
                .select(&selector)
                .map(|element| {
                    session_element_from_ref(&html, base_url.as_ref(), element, none_element_config)
                })
                .collect())
        }
        LocatorKind::XPath => {
            let parsed = Html::parse_document(html.as_ref());
            let scope = element_from_document(&parsed, scope_id)?;
            xpath_find_all_from_scope_element(
                &html,
                base_url.as_ref(),
                scope,
                locator.query(),
                none_element_config,
            )
        }
    }
}

pub(super) fn parse_selector_query(query: &str) -> OpenPageResult<Selector> {
    Selector::parse(query).map_err(|err| {
        OpenPageError::ElementNotFound(invalid_css_selector_message(query, &err.to_string()))
    })
}

pub(super) fn session_element_from_ref(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    element: ElementRef<'_>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> DocumentElement {
    DocumentElement {
        html: Arc::clone(html),
        node_id: element.id(),
        base_url: base_url.cloned(),
        none_element_config: none_element_config.cloned(),
    }
}

pub(super) fn wrap_fragment_html(html: Arc<String>) -> Arc<String> {
    Arc::new(format!(
        "<!doctype html><html><body><div {FRAGMENT_WRAPPER_ATTR}=\"1\">{}</div></body></html>",
        html.as_ref()
    ))
}

pub(super) fn element_from_document<'a>(
    document: &'a Html,
    node_id: NodeId,
) -> OpenPageResult<ElementRef<'a>> {
    document
        .tree
        .get(node_id)
        .and_then(ElementRef::wrap)
        .ok_or_else(|| OpenPageError::ElementNotFound(snapshot_node_no_longer_exists_message()))
}

#[derive(Clone, Copy)]
pub(super) enum RelativeDirection {
    Before,
    After,
}

pub(super) fn collect_matching_elements<'a, I>(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    elements: I,
    locator: Option<&str>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<DocumentElement>>
where
    I: IntoIterator<Item = ElementRef<'a>>,
{
    let selector = parse_optional_selector(locator)?;
    Ok(elements
        .into_iter()
        .filter(|element| {
            selector
                .as_ref()
                .is_none_or(|selector| selector.matches(element))
        })
        .map(|element| session_element_from_ref(html, base_url, element, none_element_config))
        .collect())
}

pub(super) fn document_relatives(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    element: ElementRef<'_>,
    direction: RelativeDirection,
    locator: Option<&str>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<DocumentElement>> {
    let selector = parse_optional_selector(locator)?;
    let root = element.tree().root();
    let elements: Vec<_> = root.descendants().filter_map(ElementRef::wrap).collect();
    let current_id = element.id();
    let current_index = elements
        .iter()
        .position(|candidate| candidate.id() == current_id)
        .ok_or_else(|| OpenPageError::ElementNotFound(snapshot_node_no_longer_exists_message()))?;

    let ancestor_ids: HashSet<_> = element.ancestors().map(|node| node.id()).collect();
    let descendant_ids: HashSet<_> = element
        .descendants()
        .skip(1)
        .map(|node| node.id())
        .collect();

    let iter: Box<dyn Iterator<Item = ElementRef<'_>> + '_> = match direction {
        RelativeDirection::Before => Box::new(
            elements[..current_index]
                .iter()
                .copied()
                .filter(|candidate| !ancestor_ids.contains(&candidate.id())),
        ),
        RelativeDirection::After => Box::new(
            elements[current_index + 1..]
                .iter()
                .copied()
                .filter(|candidate| !descendant_ids.contains(&candidate.id())),
        ),
    };

    Ok(iter
        .filter(|candidate| {
            selector
                .as_ref()
                .is_none_or(|selector| selector.matches(candidate))
        })
        .map(|candidate| session_element_from_ref(html, base_url, candidate, none_element_config))
        .collect())
}

pub(super) fn collect_document_relative_nodes(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    element: ElementRef<'_>,
    direction: RelativeDirection,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    let mut path: Vec<_> = element.ancestors().collect();
    path.reverse();
    path.push(*element);
    let mut candidates = Vec::new();

    match direction {
        RelativeDirection::Before => {
            for window in path.windows(2) {
                append_atomic_siblings(&mut candidates, window[1], SiblingDirection::Before);
            }
        }
        RelativeDirection::After => {
            for window in path.windows(2).rev() {
                append_atomic_siblings(&mut candidates, window[1], SiblingDirection::After);
            }
        }
    }

    filter_relative_node_results(
        candidates
            .into_iter()
            .map(|node| {
                scraper_node_to_session_xpath_result(html, base_url, node, none_element_config)
            })
            .collect::<OpenPageResult<Vec<_>>>()?,
        false,
    )
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SiblingDirection {
    Before,
    After,
}

pub(super) fn append_atomic_siblings<'a>(
    output: &mut Vec<NodeRef<'a, Node>>,
    node: NodeRef<'a, Node>,
    direction: SiblingDirection,
) {
    let mut siblings: Vec<_> = match direction {
        SiblingDirection::Before => node.prev_siblings().collect(),
        SiblingDirection::After => node.next_siblings().collect(),
    };
    if matches!(direction, SiblingDirection::Before) {
        siblings.reverse();
    }
    output.extend(siblings);
}

pub(super) fn scraper_node_to_session_xpath_result(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    node: NodeRef<'_, Node>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<SessionXPathResult> {
    match node.value() {
        Node::Element(_) => Ok(SessionXPathResult::Element(session_element_from_ref(
            html,
            base_url,
            ElementRef::wrap(node).ok_or_else(|| {
                OpenPageError::ElementNotFound(snapshot_node_no_longer_exists_message())
            })?,
            none_element_config,
        ))),
        Node::Text(text) => Ok(SessionXPathResult::Text(text.to_string())),
        Node::Comment(comment) => Ok(SessionXPathResult::Comment(comment.to_string())),
        Node::Doctype(doctype) => Ok(SessionXPathResult::Doctype {
            name: doctype.name.to_string(),
            public_id: (!doctype.public_id.is_empty()).then(|| doctype.public_id.to_string()),
            system_id: (!doctype.system_id.is_empty()).then(|| doctype.system_id.to_string()),
        }),
        _ => Err(OpenPageError::ElementNotFound(
            unsupported_snapshot_node_kind_message(),
        )),
    }
}

pub(super) fn resolve_href_attr(value: &str, base_url: Option<&String>) -> Option<String> {
    if value.is_empty()
        || value.to_ascii_lowercase().starts_with("javascript:")
        || value.to_ascii_lowercase().starts_with("mailto:")
    {
        return Some(value.to_string());
    }
    resolve_src_attr(value, base_url)
}

pub(super) fn resolve_src_attr(value: &str, base_url: Option<&String>) -> Option<String> {
    if value.is_empty() {
        return Some(String::new());
    }
    Some(make_absolute_url(value, base_url))
}

pub(super) fn make_absolute_url(value: &str, base_url: Option<&String>) -> String {
    let Some(base_url) = base_url else {
        return value.to_string();
    };
    Url::parse(base_url)
        .and_then(|base| base.join(value))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| value.to_string())
}

pub(super) fn normalize_text_item(value: &str) -> Option<String> {
    let normalized = value.replace('\u{a0}', " ").trim().to_string();
    if normalized.chars().any(|char| !char.is_whitespace()) {
        Some(normalized)
    } else {
        None
    }
}

pub(super) fn xpath_for_element(element: ElementRef<'_>) -> String {
    let mut parts = Vec::new();
    let mut current = Some(element);
    while let Some(node) = current {
        if node.value().attr(FRAGMENT_WRAPPER_ATTR).is_some() {
            break;
        }
        let tag = node.value().name();
        let index = node
            .prev_siblings()
            .filter_map(ElementRef::wrap)
            .filter(|sibling| sibling.value().name() == tag)
            .count()
            + 1;
        parts.push(format!("/{tag}[{index}]"));
        current = nearest_parent_element(node);
    }
    parts.reverse();
    parts.join("")
}

pub(super) fn css_path_for_element(element: ElementRef<'_>) -> String {
    let mut parts = Vec::new();
    let mut current = Some(element);
    while let Some(node) = current {
        if node.value().attr(FRAGMENT_WRAPPER_ATTR).is_some() {
            break;
        }
        let tag = node.value().name();
        if let Some(id) = node.attr("id") {
            parts.push(format!(">{tag}#{id}"));
            current = nearest_parent_element(node);
            continue;
        }
        let index = node.prev_siblings().filter_map(ElementRef::wrap).count() + 1;
        parts.push(format!(">{tag}:nth-child({index})"));
        current = nearest_parent_element(node);
    }
    parts.reverse();
    parts.join("").trim_start_matches('>').to_string()
}

pub(super) fn nth_from_start<T>(
    elements: Vec<T>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<T> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            relative_index_must_start_message(error_message),
        ));
    }
    elements
        .into_iter()
        .nth(index - 1)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

pub(super) fn nth_from_end<T>(
    elements: Vec<T>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<T> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            relative_index_must_start_message(error_message),
        ));
    }
    let len = elements.len();
    if index > len {
        return Err(OpenPageError::ElementNotFound(error_message.to_string()));
    }
    elements
        .into_iter()
        .nth(len - index)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

pub(super) fn parse_optional_locator(locator: Option<&str>) -> OpenPageResult<Option<Locator>> {
    locator
        .map(str::trim)
        .filter(|locator| !locator.is_empty())
        .map(Locator::parse)
        .transpose()
}

pub(super) fn parse_optional_xpath_locator_input<'a, L>(
    locator: Option<L>,
) -> OpenPageResult<Option<Locator>>
where
    L: Into<LocatorInput<'a>>,
{
    let locator = parse_optional_locator_input(locator)?;
    match locator {
        Some(locator) if locator.kind() == LocatorKind::Css => Err(
            OpenPageError::UnsupportedLocator(css_locator_unsupported_for_node_queries_message()),
        ),
        other => Ok(other),
    }
}

pub(super) fn parse_optional_selector(locator: Option<&str>) -> OpenPageResult<Option<Selector>> {
    let locator = parse_optional_locator(locator)?;
    locator
        .map(|locator| match locator.kind() {
            LocatorKind::Css => parse_selector_query(locator.query()),
            LocatorKind::XPath => Err(OpenPageError::UnsupportedLocator(
                xpath_locator_invalid_for_css_filtering_message(),
            )),
        })
        .transpose()
}

pub(super) fn direct_child_xpath_query(query: &str) -> String {
    format!("./{}", trim_xpath_axis_target(query))
}

pub(super) fn relative_axis_xpath_query(axis: &str, query: &str) -> String {
    format!("./{axis}::{}", trim_xpath_axis_target(query))
}

pub(super) fn trim_xpath_axis_target(query: &str) -> &str {
    let trimmed = query.trim().trim_start_matches(['.', '/']);
    if trimmed.is_empty() { "*" } else { trimmed }
}

pub(super) fn normalize_scoped_xpath_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.starts_with('/') {
        format!(".{trimmed}")
    } else {
        trimmed.to_string()
    }
}

pub(super) fn xpath_find_all_from_scope_element(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    scope: ElementRef<'_>,
    query: &str,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<DocumentElement>> {
    let scope_path = xpath_for_element(scope);
    let scope_at_fragment_root = nearest_fragment_wrapper(scope).is_some();
    xpath_find_all_with_scope(
        html,
        base_url,
        query,
        Some(scope_path.as_str()),
        scope_at_fragment_root,
        none_element_config,
    )
}

pub(super) fn xpath_query_from_scope_element(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    scope: ElementRef<'_>,
    query: &str,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    let scope_path = xpath_for_element(scope);
    let scope_at_fragment_root = nearest_fragment_wrapper(scope).is_some();
    xpath_query_with_scope(
        html,
        base_url,
        query,
        Some(scope_path.as_str()),
        scope_at_fragment_root,
        none_element_config,
    )
}

pub(super) fn relative_node_xpath_query_with_locator<F>(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    scope: ElementRef<'_>,
    locator: Option<&Locator>,
    default_query: &str,
    query_builder: F,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>>
where
    F: FnOnce(&str) -> String,
{
    let (query, keep_attributes) = match locator {
        Some(locator) => (
            query_builder(locator.query()),
            xpath_query_requests_attributes(locator.query()),
        ),
        None => (default_query.to_string(), false),
    };
    filter_relative_node_results(
        xpath_query_from_scope_element(html, base_url, scope, &query, none_element_config)?,
        keep_attributes,
    )
}

pub(super) fn filter_relative_node_results(
    items: Vec<SessionXPathResult>,
    keep_attributes: bool,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    Ok(items
        .into_iter()
        .filter(|item| match item {
            SessionXPathResult::Text(value) => normalize_text_item(value).is_some(),
            SessionXPathResult::Attribute { .. } => keep_attributes,
            _ => true,
        })
        .collect())
}

pub(super) fn xpath_query_requests_attributes(query: &str) -> bool {
    let query = query.trim();
    query.contains('@') || query.contains("attribute::")
}

pub(super) fn xpath_find_all_with_scope(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    query: &str,
    scope_path: Option<&str>,
    scope_at_fragment_root: bool,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<DocumentElement>> {
    let parsed = Html::parse_document(html.as_ref());
    let xpath_tree = xpath_html::parse(html.as_ref()).map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_html_message(&err.to_string()))
    })?;
    let xpath = xpath_engine::parse(&if scope_path.is_some() || scope_at_fragment_root {
        normalize_scoped_xpath_query(query)
    } else {
        query.trim().to_string()
    })
    .map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_query_message(query, &err.to_string()))
    })?;

    let wrapper_scraper = if scope_at_fragment_root {
        Some(fragment_wrapper_from_document(&parsed)?)
    } else {
        None
    };
    let wrapper_xpath = if scope_at_fragment_root {
        Some(fragment_wrapper_from_xpath_tree(&xpath_tree)?)
    } else {
        None
    };

    let xpath_matches = match scope_path {
        Some(scope_path) => {
            let scope = find_xpath_element_by_path(
                &xpath_tree,
                wrapper_xpath,
                scope_path,
                scope_at_fragment_root,
            )?;
            xpath.find_elements_from_element(&xpath_tree, scope)
        }
        None => match wrapper_xpath {
            Some(wrapper) => xpath.find_elements_from_element(&xpath_tree, wrapper),
            None => xpath.find_elements(&xpath_tree),
        },
    }
    .map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_query_message(query, &err.to_string()))
    })?;

    let mapping_root = wrapper_scraper.map_or_else(|| parsed.tree.root(), |wrapper| *wrapper);
    xpath_matches
        .into_iter()
        .filter(|element| {
            !scope_at_fragment_root || xpath_element_is_within_fragment_root(&xpath_tree, element)
        })
        .map(|element| {
            let path = xpath_path_for_xpath_element(&xpath_tree, element, scope_at_fragment_root)?;
            let mapped = find_scraper_element_by_path(mapping_root, &path)?;
            Ok(session_element_from_ref(
                html,
                base_url,
                mapped,
                none_element_config,
            ))
        })
        .collect()
}

pub(super) fn xpath_query_with_scope(
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    query: &str,
    scope_path: Option<&str>,
    scope_at_fragment_root: bool,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<Vec<SessionXPathResult>> {
    let parsed = Html::parse_document(html.as_ref());
    let xpath_tree = xpath_html::parse(html.as_ref()).map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_html_message(&err.to_string()))
    })?;
    let xpath = xpath_engine::parse(&if scope_path.is_some() || scope_at_fragment_root {
        normalize_scoped_xpath_query(query)
    } else {
        query.trim().to_string()
    })
    .map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_query_message(query, &err.to_string()))
    })?;

    let wrapper_scraper = if scope_at_fragment_root {
        Some(fragment_wrapper_from_document(&parsed)?)
    } else {
        None
    };
    let wrapper_xpath = if scope_at_fragment_root {
        Some(fragment_wrapper_from_xpath_tree(&xpath_tree)?)
    } else {
        None
    };

    let xpath_items = match scope_path {
        Some(scope_path) => {
            let scope = find_xpath_element_by_path(
                &xpath_tree,
                wrapper_xpath,
                scope_path,
                scope_at_fragment_root,
            )?;
            xpath.apply_to_element(&xpath_tree, scope)
        }
        None => match wrapper_xpath {
            Some(wrapper) => xpath.apply_to_element(&xpath_tree, wrapper),
            None => xpath.apply(&xpath_tree),
        },
    }
    .map_err(|err| {
        OpenPageError::UnsupportedLocator(invalid_xpath_query_message(query, &err.to_string()))
    })?;

    let mapping_root = wrapper_scraper.map_or_else(|| parsed.tree.root(), |wrapper| *wrapper);
    xpath_items
        .into_iter()
        .filter(|item| {
            !scope_at_fragment_root || xpath_item_is_within_fragment_root(&xpath_tree, item)
        })
        .map(|item| {
            xpath_item_to_session_result(
                &xpath_tree,
                html,
                base_url,
                mapping_root,
                scope_at_fragment_root,
                item,
                none_element_config,
            )
        })
        .collect()
}

pub(super) fn xpath_item_to_session_result(
    tree: &XpathItemTree,
    html: &Arc<String>,
    base_url: Option<&Arc<String>>,
    mapping_root: NodeRef<'_, Node>,
    stop_at_fragment_root: bool,
    item: XpathItem<'_>,
    none_element_config: Option<&ElementsOneConfigHandle>,
) -> OpenPageResult<SessionXPathResult> {
    match item {
        XpathItem::Node(node) => match node {
            XpathItemTreeNode::DocumentNode(_) => Ok(SessionXPathResult::Document),
            XpathItemTreeNode::ElementNode(element) => {
                let path = xpath_path_for_xpath_element(tree, element, stop_at_fragment_root)?;
                let mapped = find_scraper_element_by_path(mapping_root, &path)?;
                Ok(SessionXPathResult::Element(session_element_from_ref(
                    html,
                    base_url,
                    mapped,
                    none_element_config,
                )))
            }
            XpathItemTreeNode::TextNode(text) => Ok(SessionXPathResult::Text(text.content.clone())),
            XpathItemTreeNode::CommentNode(comment) => {
                Ok(SessionXPathResult::Comment(comment.content.clone()))
            }
            XpathItemTreeNode::AttributeNode(attribute) => Ok(SessionXPathResult::Attribute {
                name: attribute.name.clone(),
                value: attribute.value.clone(),
            }),
            XpathItemTreeNode::PINode(node) => Ok(SessionXPathResult::ProcessingInstruction {
                target: node.target.clone(),
                data: node.data.clone(),
            }),
            XpathItemTreeNode::DoctypeNode(node) => Ok(SessionXPathResult::Doctype {
                name: node.name.clone(),
                public_id: node.public_id.clone(),
                system_id: node.system_id.clone(),
            }),
        },
        XpathItem::AnyAtomicType(value) => Ok(xpath_atomic_to_session_result(value)),
        XpathItem::Function(function) => Ok(SessionXPathResult::Function(function.to_string())),
    }
}

pub(super) fn xpath_atomic_to_session_result(value: AnyAtomicType) -> SessionXPathResult {
    match value {
        AnyAtomicType::Boolean(value) => SessionXPathResult::Boolean(value),
        AnyAtomicType::Integer(value) => SessionXPathResult::Integer(value),
        AnyAtomicType::Float(value) => SessionXPathResult::Number(value.0 as f64),
        AnyAtomicType::Double(value) => SessionXPathResult::Number(value.0),
        AnyAtomicType::String(value) => SessionXPathResult::String(value),
        AnyAtomicType::QName {
            namespace_uri,
            local_name,
            prefix,
        } => SessionXPathResult::QName {
            namespace_uri,
            local_name,
            prefix,
        },
    }
}

pub(super) fn fragment_wrapper_from_document(document: &Html) -> OpenPageResult<ElementRef<'_>> {
    let selector = Selector::parse(&format!("[{FRAGMENT_WRAPPER_ATTR}='1']"))
        .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
    document.select(&selector).next().ok_or_else(|| {
        OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message())
    })
}

pub(super) fn fragment_wrapper_from_xpath_tree(
    tree: &XpathItemTree,
) -> OpenPageResult<&XpathElementNode> {
    tree.iter()
        .filter_map(|node| node.as_element_node().ok())
        .find(|element| element.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some())
        .ok_or_else(
            || OpenPageError::ElementNotFound(snapshot_fragment_wrapper_not_found_message()),
        )
}

pub(super) fn nearest_fragment_wrapper(element: ElementRef<'_>) -> Option<ElementRef<'_>> {
    element
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|candidate| candidate.attr(FRAGMENT_WRAPPER_ATTR).is_some())
}

pub(super) fn xpath_path_for_xpath_element(
    tree: &XpathItemTree,
    element: &XpathElementNode,
    stop_at_fragment_root: bool,
) -> OpenPageResult<String> {
    let mut parts = Vec::new();
    let mut current = Some(element);

    while let Some(node) = current {
        if stop_at_fragment_root && node.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() {
            break;
        }
        let parent = node.parent(tree);
        let index = xpath_element_index_in_parent(tree, node)?;
        parts.push(format!("/{}[{index}]", node.name));
        current = match parent.and_then(|parent| parent.as_element_node().ok()) {
            Some(parent)
                if stop_at_fragment_root
                    && parent.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() =>
            {
                None
            }
            other => other,
        };
    }

    parts.reverse();
    Ok(parts.join(""))
}

pub(super) fn xpath_element_index_in_parent(
    tree: &XpathItemTree,
    element: &XpathElementNode,
) -> OpenPageResult<usize> {
    let Some(parent) = element.parent(tree) else {
        return Ok(1);
    };

    let mut index = 0;
    for child in parent.children(tree) {
        let Ok(child_element) = child.as_element_node() else {
            continue;
        };
        if child_element.name != element.name {
            continue;
        }
        index += 1;
        if std::ptr::eq(child_element, element) {
            return Ok(index);
        }
    }

    Err(OpenPageError::ElementNotFound(
        xpath_node_no_longer_exists_message(),
    ))
}

pub(super) fn find_xpath_element_by_path<'a>(
    tree: &'a XpathItemTree,
    wrapper: Option<&'a XpathElementNode>,
    path: &str,
    scope_at_fragment_root: bool,
) -> OpenPageResult<&'a XpathElementNode> {
    let segments = parse_xpath_path(path)?;
    let mut current_children = if let Some(wrapper) = wrapper {
        wrapper.children(tree).collect::<Vec<_>>()
    } else {
        tree.root().children(tree)
    };
    let mut current = None;

    for (index, segment) in segments.iter().enumerate() {
        let next = nth_xpath_child_by_tag(&current_children, &segment.tag, segment.index)?;
        current = Some(next);
        if index + 1 < segments.len() {
            current_children = next.children(tree).collect();
        }
    }

    let element = current.ok_or_else(|| OpenPageError::ElementNotFound(path.to_string()))?;
    if scope_at_fragment_root && element.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() {
        return Err(OpenPageError::ElementNotFound(path.to_string()));
    }
    Ok(element)
}

pub(super) fn nth_xpath_child_by_tag<'a>(
    children: &[&'a XpathItemTreeNode],
    tag: &str,
    index: usize,
) -> OpenPageResult<&'a XpathElementNode> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            invalid_xpath_segment_index_message(tag),
        ));
    }
    children
        .iter()
        .filter_map(|child| child.as_element_node().ok())
        .filter(|child| child.name == tag)
        .nth(index - 1)
        .ok_or_else(|| OpenPageError::ElementNotFound(xpath_segment_not_found_message(tag, index)))
}

pub(super) fn find_scraper_element_by_path<'a>(
    start: NodeRef<'a, Node>,
    path: &str,
) -> OpenPageResult<ElementRef<'a>> {
    let root = top_scraper_node(start);
    let start_id = start.id();
    match find_scraper_element_by_path_from(start, path) {
        Ok(element) => Ok(element),
        Err(err) if path.starts_with('/') && root.id() != start_id => {
            let element = find_scraper_element_by_path_from(root, path)?;
            if element.id() == start_id || element.ancestors().any(|node| node.id() == start_id) {
                Ok(element)
            } else {
                Err(err)
            }
        }
        Err(err) => Err(err),
    }
}

pub(super) fn find_scraper_element_by_path_from<'a>(
    start: NodeRef<'a, Node>,
    path: &str,
) -> OpenPageResult<ElementRef<'a>> {
    let segments = parse_xpath_path(path)?;
    let mut current = start;

    for segment in segments {
        current = nth_scraper_child_by_tag(current, &segment.tag, segment.index)?;
    }

    ElementRef::wrap(current)
        .ok_or_else(|| OpenPageError::ElementNotFound(xpath_path_not_found_message(path)))
}

pub(super) fn nth_scraper_child_by_tag<'a>(
    node: NodeRef<'a, Node>,
    tag: &str,
    index: usize,
) -> OpenPageResult<NodeRef<'a, Node>> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            invalid_xpath_segment_index_message(tag),
        ));
    }
    node.children()
        .filter_map(ElementRef::wrap)
        .filter(|child| child.value().name() == tag)
        .nth(index - 1)
        .map(|child| *child)
        .ok_or_else(|| OpenPageError::ElementNotFound(xpath_segment_not_found_message(tag, index)))
}

pub(super) fn top_scraper_node(node: NodeRef<'_, Node>) -> NodeRef<'_, Node> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        current = parent;
    }
    current
}

pub(super) fn xpath_element_is_within_fragment_root(
    tree: &XpathItemTree,
    element: &XpathElementNode,
) -> bool {
    let mut current = Some(element);
    while let Some(node) = current {
        if node.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() {
            return true;
        }
        current = node
            .parent(tree)
            .and_then(|parent| parent.as_element_node().ok());
    }
    false
}

pub(super) fn xpath_item_is_within_fragment_root(
    tree: &XpathItemTree,
    item: &XpathItem<'_>,
) -> bool {
    let XpathItem::Node(node) = item else {
        return true;
    };
    let mut current = Some(*node);
    while let Some(node) = current {
        if let Ok(element) = node.as_element_node() {
            if element.get_attribute(tree, FRAGMENT_WRAPPER_ATTR).is_some() {
                return true;
            }
        }
        current = node.parent(tree);
    }
    false
}

#[derive(Debug)]
pub(super) struct XPathPathSegment {
    tag: String,
    index: usize,
}

pub(super) fn parse_xpath_path(path: &str) -> OpenPageResult<Vec<XPathPathSegment>> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let (tag, index) = segment
                .rsplit_once('[')
                .and_then(|(tag, rest)| rest.strip_suffix(']').map(|index| (tag, index)))
                .ok_or_else(|| {
                    OpenPageError::ElementNotFound(unsupported_xpath_path_message(path))
                })?;
            let index = index.parse::<usize>().map_err(|_| {
                OpenPageError::ElementNotFound(unsupported_xpath_path_message(path))
            })?;
            Ok(XPathPathSegment {
                tag: tag.to_string(),
                index,
            })
        })
        .collect()
}

pub(super) fn nearest_parent_element(element: ElementRef<'_>) -> Option<ElementRef<'_>> {
    let mut current = element.parent();
    while let Some(node) = current {
        if let Some(parent) = ElementRef::wrap(node) {
            return Some(parent);
        }
        current = node.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        CookieInput, DocumentElement, Session, SessionCert, SessionCookieParam, SessionHooks,
        SessionOptions, SessionRequestOptions, SessionXPathResult, append_query_params,
        cookie_assignment, cookie_input_to_params, cookies_from_header, default_referer_header,
        nth_scraper_child_by_tag, parse_headers_input, parse_optional_selector, parse_xpath_path,
        remove_cookie_from_header, resolve_local_file_path, resolve_session_options_ini_path,
        session_cookie_header_decode_error, snapshot_find, snapshot_find_all,
        snapshot_fragment_find, snapshot_fragment_root, snapshot_fragment_root_with_base_url,
        snapshot_root,
    };
    use crate::settings::scoped_test_settings;
    use crate::{By, ElementsListExt, LocatorInput, OpenPageError, Settings};
    use base64::Engine;
    use base64::prelude::BASE64_STANDARD;
    use scraper::Html;
    use serde_json::json;
    use std::env;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{LazyLock, Mutex};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use url::Url;

    static CURRENT_DIR_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    const HTML: &str = r#"
<!doctype html>
<html>
  <body>
    <section id="root">
      <h1>OpenPage</h1>
      <input id="name" value="openpage" />
      <button id="submit">Go</button>
      <div id="out"></div>
      <ul class="items">
        <li class="item" data-kind="a">alpha</li>
        <li class="item" data-kind="b">beta</li>
      </ul>
    </section>
  </body>
</html>
"#;

    fn load_session_html_for_test(
        page: &Session,
        html: &str,
        url: Option<&str>,
    ) -> crate::OpenPageResult<()> {
        let mut state = page
            .inner
            .lock()
            .map_err(|_| OpenPageError::PageOperation("session state lock poisoned".to_string()))?;
        state.url = url.map(str::to_string);
        state.response_content_type = Some("text/html; charset=utf-8".to_string());
        state.pending_response = None;
        state.raw_data = Some(Arc::new(html.as_bytes().to_vec()));
        super::refresh_state_body_encoding(&mut state);
        Ok(())
    }

    fn poison_mutex<T: Send + 'static>(mutex: Arc<Mutex<T>>) {
        let join = thread::spawn(move || {
            let _guard = mutex.lock().expect("lock poisoned test mutex");
            panic!("poison mutex");
        })
        .join();
        assert!(join.is_err(), "poison helper thread should panic");
    }

    #[test]
    fn snapshot_find_supports_nested_queries() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let heading = root.find("h1").expect("nested heading should exist");
        let first_item = root.find(".item").expect("nested item should exist");

        assert_eq!(
            heading.text().expect("heading text"),
            Some("OpenPage".to_string())
        );
        assert_eq!(heading.tag().expect("heading tag"), "h1".to_string());
        assert_eq!(
            first_item.attr("data-kind").expect("item attr"),
            Some("a".to_string())
        );
        let attrs = first_item.attrs().expect("item attrs");
        assert!(attrs.contains(&("class".to_string(), "item".to_string())));
        assert!(attrs.contains(&("data-kind".to_string(), "a".to_string())));
    }

    #[test]
    fn snapshot_find_all_keeps_match_order() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let items = root.find_all(".item").expect("items should exist");

        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].text().expect("first item text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            items[1].text().expect("second item text"),
            Some("beta".to_string())
        );

        let top_level = snapshot_find_all(HTML, ".item").expect("top-level items should exist");
        assert_eq!(top_level.len(), 2);
    }

    #[test]
    fn session_elements_one_runtime_config_supports_none_value_and_raise() {
        let page = Session::new(SessionOptions::default()).expect("session page");
        load_session_html_for_test(
            &page,
            r#"
            <html>
              <body>
                <div class="item" data-role="keep">Alpha</div>
                <div class="item" data-role="other">Beta</div>
              </body>
            </html>
            "#,
            Some("https://example.com/items"),
        )
        .expect("load session html");

        page.set_none_element_value(Some("missing"), true)
            .expect("set session none element value");
        let items = page.find_all(".item").expect("session items");
        let missing = items
            .filter_one()
            .attr("data-role", "missing", true)
            .expect("session missing filter");
        assert_eq!(
            missing.text().expect("missing text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.attr("id").expect("missing attr"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.texts(false).expect("missing texts"),
            Some(vec!["missing".to_string()])
        );
        assert_eq!(
            missing.comments().expect("missing comments"),
            Some(vec!["missing".to_string()])
        );

        page.set_raise_when_ele_not_found(true)
            .expect("set session raise when missing");
        let error = items
            .filter_one()
            .attr("data-role", "missing", true)
            .expect_err("session missing filter should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected session filter error: {error}"
        );
    }

    #[test]
    fn session_page_inherits_global_raise_when_ele_not_found_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        Settings::set_raise_when_ele_not_found(true);

        let page = Session::new(SessionOptions::default()).expect("session page");
        load_session_html_for_test(
            &page,
            r#"
            <html>
              <body>
                <div class="item" data-role="keep">Alpha</div>
                <div class="item" data-role="other">Beta</div>
              </body>
            </html>
            "#,
            Some("https://example.com/items"),
        )
        .expect("load session html");

        let error = page
            .find_all(".item")
            .expect("session items")
            .filter_one()
            .attr("data-role", "missing", true)
            .expect_err("session missing filter should use global raise setting");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected session filter error: {error}"
        );
    }

    #[test]
    fn session_ele_runtime_config_supports_none_value_and_nested_queries() {
        let page = Session::new(SessionOptions::default()).expect("session page");
        load_session_html_for_test(
            &page,
            r#"
            <html>
              <body>
                <section id="card">
                  <span class="name">Alpha</span>
                  <span class="phone">10086</span>
                </section>
                <section id="tail">Omega</section>
              </body>
            </html>
            "#,
            Some("https://example.com/card"),
        )
        .expect("load session html");

        assert_eq!(page.eles(".missing").expect("missing list").len(), 0);

        let card = page.ele("#card").expect("card ele");
        assert!(card.is_some());
        assert_eq!(
            card.ele(".name")
                .expect("name ele")
                .text()
                .expect("name text"),
            Some("Alpha".to_string())
        );
        assert_eq!(
            card.child()
                .expect("card child")
                .text()
                .expect("card child text"),
            Some("Alpha".to_string())
        );
        assert_eq!(
            page.ele(".name")
                .expect("name ele")
                .parent()
                .expect("name parent")
                .attr("id")
                .expect("name parent attr"),
            Some("card".to_string())
        );
        assert_eq!(
            page.ele(".name")
                .expect("name ele")
                .next()
                .expect("name next")
                .text()
                .expect("name next text"),
            Some("10086".to_string())
        );
        assert_eq!(
            page.ele(".phone")
                .expect("phone ele")
                .after()
                .expect("phone after")
                .text()
                .expect("phone after text"),
            Some("Omega".to_string())
        );

        let missing_default = page.ele(".missing").expect("missing ele");
        assert!(missing_default.is_none());
        assert_eq!(missing_default.text().expect("missing default text"), None);

        page.set_none_element_value(Some("missing"), true)
            .expect("set session none element value");

        let missing = page.ele(".missing").expect("missing ele after none value");
        assert_eq!(
            missing.text().expect("missing text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing.attr("id").expect("missing attr"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .ele(".child")
                .expect("missing child ele")
                .text()
                .expect("missing child text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .child()
                .expect("missing child")
                .text()
                .expect("missing child text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .parent()
                .expect("missing parent")
                .text()
                .expect("missing parent text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .next()
                .expect("missing next")
                .text()
                .expect("missing next text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .before()
                .expect("missing before")
                .text()
                .expect("missing before text"),
            Some("missing".to_string())
        );
        assert_eq!(
            missing
                .after()
                .expect("missing after")
                .text()
                .expect("missing after text"),
            Some("missing".to_string())
        );
        assert_eq!(
            page.ele("#card")
                .expect("card ele after none value")
                .ele(".phone")
                .expect("missing phone ele")
                .text()
                .expect("missing phone text"),
            Some("10086".to_string())
        );
        assert_eq!(
            page.ele("#card")
                .expect("card ele after none value")
                .child_with(Some(".missing"), 1)
                .expect("missing child wrapper")
                .text()
                .expect("missing child wrapper text"),
            Some("missing".to_string())
        );
        assert_eq!(
            page.ele(".phone")
                .expect("phone ele after none value")
                .next_with(Some(".missing"), 1)
                .expect("missing next wrapper")
                .text()
                .expect("missing next wrapper text"),
            Some("missing".to_string())
        );

        page.set_raise_when_ele_not_found(true)
            .expect("set session raise when missing");
        let error = page.ele(".missing").expect_err("session ele should raise");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected session ele error: {error}"
        );
        let error = page
            .ele("#card")
            .expect("card ele after raise")
            .child_with(Some(".missing"), 1)
            .expect_err("missing child should raise after toggle");
        assert!(
            matches!(error, OpenPageError::ElementNotFound(_)),
            "unexpected session child error: {error}"
        );
    }

    #[test]
    fn cookie_assignment_includes_optional_scope_fields() {
        let cookie = cookie_assignment("foo", "bar", Some("example.com"), Some("/demo"));
        assert_eq!(cookie, "foo=bar; Domain=example.com; Path=/demo");
    }

    #[test]
    fn remove_cookie_from_header_drops_matching_cookie_names() {
        let header = remove_cookie_from_header("foo=bar; baz=qux; foo=next", "foo");
        assert_eq!(header, "baz=qux");
    }

    fn spawn_retry_server(
        statuses: &'static [&'static str],
        bodies: &'static [&'static str],
        attempts: Arc<AtomicUsize>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind retry server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let mut methods = Vec::new();
            for (index, status) in statuses.iter().enumerate() {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]);
                methods.push(
                    request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().next())
                        .unwrap_or_default()
                        .to_string(),
                );
                attempts.fetch_add(1, Ordering::SeqCst);

                let body = bodies[index];
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write response");
            }
            methods
        });
        (address, handle)
    }

    fn make_temp_file(name: &str, content: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = env::temp_dir().join(format!("openpage-{name}-{suffix}.html"));
        fs::write(&path, content).expect("write temp file");
        path
    }

    fn make_temp_bytes(name: &str, content: &[u8]) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = env::temp_dir().join(format!("openpage-{name}-{suffix}.bin"));
        fs::write(&path, content).expect("write temp bytes");
        path
    }

    fn make_temp_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        env::temp_dir().join(format!("openpage-{name}-{suffix}"))
    }

    struct RestoreFileGuard {
        path: std::path::PathBuf,
        original: Option<Vec<u8>>,
    }

    impl RestoreFileGuard {
        fn new(path: std::path::PathBuf) -> Self {
            Self {
                original: fs::read(&path).ok(),
                path,
            }
        }
    }

    impl Drop for RestoreFileGuard {
        fn drop(&mut self) {
            if let Some(original) = &self.original {
                let _ = fs::write(&self.path, original);
            } else {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    struct CurrentDirGuard {
        original: std::path::PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CurrentDirGuard {
        fn change_to(path: &std::path::Path) -> Self {
            let lock = CURRENT_DIR_TEST_LOCK.lock().expect("lock current dir");
            let original = env::current_dir().expect("read current dir");
            env::set_current_dir(path).expect("set current dir");
            Self {
                original,
                _lock: lock,
            }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original);
        }
    }

    fn spawn_capture_server(
        status: &'static str,
        body: &'static str,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind capture server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write response");
            request
        });
        (address, handle)
    }

    fn spawn_redirect_server(max_requests: usize) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind redirect server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let redirect_target = format!("{address}/final");
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for _ in 0..max_requests {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let first_line = request.lines().next().unwrap_or_default().to_string();
                requests.push(first_line.clone());

                if first_line.contains("/final ") {
                    let body = "done";
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .expect("write final response");
                } else {
                    let body = "redirect";
                    write!(
                        stream,
                        "HTTP/1.1 302 Found\r\nLocation: {redirect_target}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .expect("write redirect response");
                }
            }
            requests
        });
        (address, handle)
    }

    fn spawn_delayed_server(delay: Duration) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind delayed server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read request");
            thread::sleep(delay);
            let body = "slow";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        });
        (address, handle)
    }

    fn spawn_truncated_body_server() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind truncated server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read request");
            let body = "short";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 999\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
            );
        });
        (address, handle)
    }

    #[test]
    fn session_options_default_http_settings_match_reference_behavior() {
        let options = SessionOptions::default();

        assert_eq!(options.timeout_secs, 10);
        assert_eq!(options.timeout_secs(), 10);
        assert!(options.verify);
        assert!(options.verify());
        assert!(options.trust_env);
        assert!(options.trust_env());
        assert_eq!(options.max_redirects, Some(30));
        assert_eq!(options.max_redirects(), Some(30));
        assert_eq!(options.retry_times, 3);
        assert_eq!(options.retry_times(), 3);
        assert_eq!(options.retry_interval_millis, 2_000);
        assert_eq!(options.retry_interval_millis(), 2_000);
        assert_eq!(options.retry_interval(), 2.0);
        assert_eq!(options.download_path, std::path::PathBuf::from("."));
        assert_eq!(options.download_path(), std::path::Path::new("."));
        assert!(options.headers.is_empty());
        assert!(options.headers().is_empty());
        assert!(options.cookies.is_empty());
        assert!(options.cookies().is_empty());
        assert!(options.params.is_empty());
        assert!(options.params().is_empty());
        assert!(options.auth.is_none());
        assert!(options.auth().is_none());
        assert!(options.hooks().is_empty());
        assert!(!options.stream);
        assert!(!options.stream());
        assert!(options.http_proxy.is_none());
        assert!(options.http_proxy().is_none());
        assert!(options.https_proxy.is_none());
        assert!(options.https_proxy().is_none());
        assert!(options.user_agent().is_none());
        assert!(options.cert().is_none());
    }

    #[test]
    fn session_options_response_hooks_fire_for_page_requests() {
        let captured = Arc::new(Mutex::new(Vec::<(
            String,
            Option<String>,
            Option<u16>,
            String,
        )>::new()));
        let captured_for_hook = Arc::clone(&captured);
        let mut hooks = SessionHooks::new();
        hooks.add_response(move |event| {
            let body = String::from_utf8(event.raw_data.as_ref().clone()).expect("hook body utf8");
            captured_for_hook.lock().expect("lock hook capture").push((
                event.requested_url,
                event.response.url.clone(),
                event.response.status_code,
                body,
            ));
        });

        let mut options = SessionOptions::default();
        options.set_hooks(hooks);
        let page = Session::new(options).expect("session page");
        let (address, handle) = spawn_capture_server("200 OK", "hooked");
        let url = format!("{address}/hook");

        assert!(
            page.get(&url)
                .expect("request with response hook")
                .is_success()
        );
        handle.join().expect("server thread");

        let records = captured.lock().expect("lock hook records");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, url);
        assert_eq!(records[0].1.as_deref(), Some(records[0].0.as_str()));
        assert_eq!(records[0].2, Some(200));
        assert_eq!(records[0].3, "hooked");
    }

    #[test]
    fn session_request_options_response_hooks_extend_runtime_hooks() {
        let labels = Arc::new(Mutex::new(Vec::<String>::new()));
        let labels_for_runtime = Arc::clone(&labels);
        let labels_for_request = Arc::clone(&labels);

        let mut runtime_hooks = SessionHooks::new();
        runtime_hooks.add_response(move |_| {
            labels_for_runtime
                .lock()
                .expect("lock runtime labels")
                .push("runtime".to_string());
        });

        let mut request_hooks = SessionHooks::new();
        request_hooks.add_response(move |_| {
            labels_for_request
                .lock()
                .expect("lock request labels")
                .push("request".to_string());
        });

        let mut options = SessionOptions::default();
        options.set_hooks(runtime_hooks);
        let page = Session::new(options).expect("session page");
        let request_options = SessionRequestOptions {
            hooks: Some(request_hooks),
            ..SessionRequestOptions::default()
        };
        let (address, handle) = spawn_capture_server("200 OK", "hooks");
        let url = format!("{address}/extend");

        assert!(
            page.get_with_options(&url, &request_options)
                .expect("request with runtime + request hooks")
                .is_success()
        );
        handle.join().expect("server thread");

        let labels = labels.lock().expect("lock response hook labels");
        assert_eq!(labels.as_slice(), ["runtime", "request"]);
    }

    #[test]
    fn session_request_options_accessors_expose_request_overrides() {
        let default_options = SessionRequestOptions::default();
        assert!(default_options.timeout_secs().is_none());
        assert!(default_options.retry_times().is_none());
        assert!(default_options.retry_interval_millis().is_none());
        assert!(default_options.retry_interval().is_none());
        assert!(default_options.user_agent().is_none());
        assert!(default_options.headers().is_empty());
        assert!(default_options.header("accept").is_none());
        assert!(default_options.params().is_empty());
        assert!(default_options.param("page").is_none());
        assert!(default_options.auth().is_none());
        assert!(default_options.hooks().is_none());
        assert!(default_options.stream().is_none());

        let mut hooks = SessionHooks::new();
        hooks.add_response(|_| {});
        let request_options = SessionRequestOptions {
            timeout_secs: Some(3),
            retry_times: Some(2),
            retry_interval_millis: Some(250),
            user_agent: Some("OpenPage/Request".to_string()),
            headers: vec![("accept".to_string(), "application/json".to_string())],
            params: vec![("page".to_string(), "7".to_string())],
            auth: Some(("bob".to_string(), "secret".to_string())),
            hooks: Some(hooks),
            stream: Some(true),
        };

        assert_eq!(request_options.timeout_secs(), Some(3));
        assert_eq!(request_options.retry_times(), Some(2));
        assert_eq!(request_options.retry_interval_millis(), Some(250));
        assert_eq!(request_options.retry_interval(), Some(0.25));
        assert_eq!(request_options.user_agent(), Some("OpenPage/Request"));
        assert_eq!(request_options.header("Accept"), Some("application/json"));
        assert_eq!(
            request_options.headers(),
            &[("accept".to_string(), "application/json".to_string())]
        );
        assert_eq!(request_options.param("page"), Some("7"));
        assert_eq!(
            request_options.params(),
            &[("page".to_string(), "7".to_string())]
        );
        assert_eq!(request_options.auth(), Some(("bob", "secret")));
        assert!(request_options.hooks().is_some());
        assert_eq!(request_options.stream(), Some(true));
    }

    #[test]
    fn parse_headers_input_accepts_text_and_pairs() {
        let text = "\nconnection: keep-alive\naccept: text/html\n";
        let parsed = parse_headers_input(text).expect("parse headers text");
        assert_eq!(
            parsed,
            vec![
                ("connection".to_string(), "keep-alive".to_string()),
                ("accept".to_string(), "text/html".to_string())
            ]
        );

        let parsed = parse_headers_input([("X-Test", "1"), ("Accept", "application/json")])
            .expect("parse header pairs");
        assert_eq!(
            parsed,
            vec![
                ("X-Test".to_string(), "1".to_string()),
                ("Accept".to_string(), "application/json".to_string())
            ]
        );

        assert!(parse_headers_input("missing separator").is_err());
    }

    #[test]
    fn cookie_input_parser_accepts_text_and_json_formats() {
        let from_text = cookie_input_to_params(
            CookieInput::from("host=1; path=/shared"),
            Some("https://www.example.test/shared/page"),
        )
        .expect("parse single cookie text");
        assert_eq!(from_text.len(), 1);
        assert_eq!(from_text[0].name, "host");
        assert_eq!(from_text[0].value, "1");
        assert_eq!(
            from_text[0].url.as_deref(),
            Some("https://www.example.test/shared/page")
        );
        assert_eq!(from_text[0].path.as_deref(), Some("/shared"));

        let multi_json = json!({
            "sid": "abc",
            "token": "xyz",
            "domain": ".example.test",
            "path": "/",
            "secure": true,
            "httpOnly": true,
            "sameSite": "Strict"
        });
        let from_json =
            cookie_input_to_params(CookieInput::from(&multi_json), None).expect("parse json");
        assert_eq!(from_json.len(), 2);
        assert!(
            from_json
                .iter()
                .all(|cookie| cookie.domain.as_deref() == Some(".example.test"))
        );
        assert!(
            from_json
                .iter()
                .all(|cookie| cookie.path.as_deref() == Some("/"))
        );
        assert!(from_json.iter().all(|cookie| cookie.secure));
        assert!(from_json.iter().all(|cookie| cookie.http_only));
        assert!(
            from_json
                .iter()
                .all(|cookie| cookie.same_site.as_deref() == Some("Strict"))
        );

        let mixed_list = json!([
            "alpha=1; domain=alpha.test; path=/",
            {"name": "beta", "value": "2", "url": "https://beta.test/", "same_site": "Lax"}
        ]);
        let list_cookies = cookie_input_to_params(CookieInput::from(&mixed_list), None)
            .expect("parse mixed cookie list");
        assert_eq!(list_cookies.len(), 2);
        assert!(list_cookies.iter().any(|cookie| cookie.name == "alpha"));
        assert!(
            list_cookies.iter().any(|cookie| {
                cookie.name == "beta" && cookie.same_site.as_deref() == Some("Lax")
            })
        );
    }

    #[test]
    fn cookie_input_validation_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let missing_name = [SessionCookieParam {
            name: " ".to_string(),
            value: "1".to_string(),
            url: None,
            domain: Some("example.test".to_string()),
            path: None,
            secure: false,
            http_only: false,
            same_site: None,
        }];
        let english_empty_name =
            cookie_input_to_params(CookieInput::from(missing_name.as_slice()), None)
                .expect_err("cookie name validation should fail")
                .to_string();
        assert!(english_empty_name.contains("cookie name cannot be empty"));

        let english_scope = cookie_input_to_params(CookieInput::from("sid=1"), None)
            .expect_err("cookie scope validation should fail")
            .to_string();
        assert!(english_scope.contains("cookie `sid` requires either url or domain"));

        let english_separators = cookie_input_to_params(CookieInput::from("a=1; b=2, c=3"), None)
            .expect_err("cookie separator validation should fail")
            .to_string();
        assert!(english_separators.contains("cookie text cannot mix ';' and ',' separators"));

        let english_missing_text_value = cookie_input_to_params(
            CookieInput::from("sid; domain=example.test"),
            Some("https://example.test/"),
        )
        .expect_err("cookie text missing value validation should fail")
        .to_string();
        assert!(
            english_missing_text_value.contains("invalid cookie text: `sid` is missing a value")
        );

        let english_text_without_cookie = cookie_input_to_params(
            CookieInput::from("domain=example.test; path=/"),
            Some("https://example.test/"),
        )
        .expect_err("cookie text assignment validation should fail")
        .to_string();
        assert!(
            english_text_without_cookie
                .contains("cookie text must contain at least one cookie assignment")
        );

        let english_list_item = cookie_input_to_params(
            CookieInput::from(&json!([{"sid": "1", "token": "2", "domain": "example.test"}])),
            None,
        )
        .expect_err("cookie list item validation should fail")
        .to_string();
        assert!(
            english_list_item.contains("cookie list items must each describe exactly one cookie")
        );

        let english_object_without_cookie =
            cookie_input_to_params(CookieInput::from(&json!({"domain": "example.test"})), None)
                .expect_err("cookie object assignment validation should fail")
                .to_string();
        assert!(
            english_object_without_cookie
                .contains("cookie object must contain at least one cookie assignment")
        );

        let english_type = cookie_input_to_params(CookieInput::from(&json!(true)), None)
            .expect_err("cookie input type validation should fail")
            .to_string();
        assert!(english_type.contains("cookie input must be null, string, object, or array"));

        let english_name_value = cookie_input_to_params(
            CookieInput::from("name=sid; domain=example.test"),
            Some("https://example.test/"),
        )
        .expect_err("cookie name/value validation should fail")
        .to_string();
        assert!(english_name_value.contains("cookie text must contain `name` and `value`"));

        let english_bool = cookie_input_to_params(
            CookieInput::from("sid=1; secure=None; domain=example.test"),
            None,
        )
        .expect_err("cookie boolean validation should fail")
        .to_string();
        assert!(english_bool.contains("invalid cookie text.secure: expected boolean"));

        Settings::set_language("cn");

        let chinese_empty_name =
            cookie_input_to_params(CookieInput::from(missing_name.as_slice()), None)
                .expect_err("cookie name validation should fail in Chinese")
                .to_string();
        assert!(chinese_empty_name.contains("cookie 名称不能为空"));
        assert!(chinese_empty_name.contains("HTTP 操作失败"));

        let chinese_scope = cookie_input_to_params(CookieInput::from("sid=1"), None)
            .expect_err("cookie scope validation should fail in Chinese")
            .to_string();
        assert!(chinese_scope.contains("cookie `sid` 必须设置 url 或 domain"));

        let chinese_separators = cookie_input_to_params(CookieInput::from("a=1; b=2, c=3"), None)
            .expect_err("cookie separator validation should fail in Chinese")
            .to_string();
        assert!(chinese_separators.contains("cookie 文本不能同时混用 ';' 和 ',' 分隔符"));

        let chinese_missing_text_value = cookie_input_to_params(
            CookieInput::from("sid; domain=example.test"),
            Some("https://example.test/"),
        )
        .expect_err("cookie text missing value validation should fail in Chinese")
        .to_string();
        assert!(chinese_missing_text_value.contains("cookie 文本中的 `sid` 缺少值"));

        let chinese_text_without_cookie = cookie_input_to_params(
            CookieInput::from("domain=example.test; path=/"),
            Some("https://example.test/"),
        )
        .expect_err("cookie text assignment validation should fail in Chinese")
        .to_string();
        assert!(chinese_text_without_cookie.contains("cookie 文本必须至少包含一个 cookie 赋值"));

        let chinese_list_item = cookie_input_to_params(
            CookieInput::from(&json!([{"sid": "1", "token": "2", "domain": "example.test"}])),
            None,
        )
        .expect_err("cookie list item validation should fail in Chinese")
        .to_string();
        assert!(chinese_list_item.contains("cookie 列表中的每一项必须只描述一个 cookie"));

        let chinese_object_without_cookie =
            cookie_input_to_params(CookieInput::from(&json!({"domain": "example.test"})), None)
                .expect_err("cookie object assignment validation should fail in Chinese")
                .to_string();
        assert!(chinese_object_without_cookie.contains("cookie 对象必须至少包含一个 cookie 赋值"));

        let chinese_type = cookie_input_to_params(CookieInput::from(&json!(true)), None)
            .expect_err("cookie input type validation should fail in Chinese")
            .to_string();
        assert!(chinese_type.contains("cookie 输入必须是 null、字符串、对象或数组"));

        let chinese_name_value = cookie_input_to_params(
            CookieInput::from("name=sid; domain=example.test"),
            Some("https://example.test/"),
        )
        .expect_err("cookie name/value validation should fail in Chinese")
        .to_string();
        assert!(chinese_name_value.contains("cookie text 必须包含 `name` 和 `value`"));

        let chinese_bool = cookie_input_to_params(
            CookieInput::from("sid=1; secure=None; domain=example.test"),
            None,
        )
        .expect_err("cookie boolean validation should fail in Chinese")
        .to_string();
        assert!(chinese_bool.contains("cookie text.secure 无效: 期望 boolean"));
    }

    #[test]
    fn session_cookie_header_decode_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english = session_cookie_header_decode_error("bad header").to_string();
        assert_eq!(
            english,
            "http operation failed: failed to read session cookie header: bad header"
        );

        Settings::set_language("cn");

        let chinese = session_cookie_header_decode_error("bad header").to_string();
        assert_eq!(
            chinese,
            "HTTP 操作失败: 读取 session cookie header 失败: bad header"
        );
    }

    #[test]
    fn session_url_validation_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = Session::new(SessionOptions::default()).expect("create session page");

        let english_cookie_header = page
            .cookie_header("not a url")
            .expect_err("cookie_header() should reject invalid url")
            .to_string();
        assert!(english_cookie_header.contains("invalid url `not a url`"));

        let english_set_cookie_header = page
            .set_cookie_header("not a url", "sid=1")
            .expect_err("set_cookie_header() should reject invalid url")
            .to_string();
        assert!(english_set_cookie_header.contains("invalid url `not a url`"));

        let english_set_cookie = page
            .set_cookie("sid", "1", Some("not a url"), None, None)
            .expect_err("set_cookie() should reject invalid explicit url")
            .to_string();
        assert!(english_set_cookie.contains("invalid url `not a url`"));

        let english_cookies_from_header = cookies_from_header("not a url", "sid=1")
            .expect_err("cookies_from_header() should reject invalid url")
            .to_string();
        assert!(english_cookies_from_header.contains("invalid url `not a url`"));

        let english_referer = default_referer_header(None, "not a url")
            .expect_err("default_referer_header() should reject invalid url")
            .to_string();
        assert!(english_referer.contains("invalid url `not a url`"));

        let english_query = append_query_params("not a url", &[("q".to_string(), "1".to_string())])
            .expect_err("append_query_params() should reject invalid url")
            .to_string();
        assert!(english_query.contains("invalid url `not a url`"));

        let english_file = resolve_local_file_path("file://example.com/path")
            .expect_err("resolve_local_file_path() should reject invalid file url")
            .to_string();
        assert!(english_file.contains("invalid file url: file://example.com/path"));

        Settings::set_language("cn");

        let chinese_cookie_header = page
            .cookie_header("not a url")
            .expect_err("cookie_header() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_cookie_header.contains("无效的 url `not a url`"));
        assert!(chinese_cookie_header.contains("HTTP 操作失败"));

        let chinese_set_cookie_header = page
            .set_cookie_header("not a url", "sid=1")
            .expect_err("set_cookie_header() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_set_cookie_header.contains("无效的 url `not a url`"));

        let chinese_set_cookie = page
            .set_cookie("sid", "1", Some("not a url"), None, None)
            .expect_err("set_cookie() should reject invalid explicit url in Chinese")
            .to_string();
        assert!(chinese_set_cookie.contains("无效的 url `not a url`"));

        let chinese_cookies_from_header = cookies_from_header("not a url", "sid=1")
            .expect_err("cookies_from_header() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_cookies_from_header.contains("无效的 url `not a url`"));

        let chinese_referer = default_referer_header(None, "not a url")
            .expect_err("default_referer_header() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_referer.contains("无效的 url `not a url`"));

        let chinese_query = append_query_params("not a url", &[("q".to_string(), "1".to_string())])
            .expect_err("append_query_params() should reject invalid url in Chinese")
            .to_string();
        assert!(chinese_query.contains("无效的 url `not a url`"));

        let chinese_file = resolve_local_file_path("file://example.com/path")
            .expect_err("resolve_local_file_path() should reject invalid file url in Chinese")
            .to_string();
        assert!(chinese_file.contains("无效的 file url: file://example.com/path"));
    }

    #[test]
    fn session_host_runtime_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = Session::new(SessionOptions::default()).expect("create session page");

        let english_title = page
            .title()
            .expect_err("title() should fail before any document is loaded")
            .to_string();
        assert!(english_title.contains("session page has no loaded document"));

        let english_set_cookie = page
            .set_cookie("sid", "1", None, None, None)
            .expect_err("set_cookie() should require an explicit scope before navigation")
            .to_string();
        assert!(
            english_set_cookie.contains("session page has no current url; provide url explicitly")
        );

        Settings::set_language("cn");

        let chinese_title = page
            .title()
            .expect_err("title() should fail in Chinese before any document is loaded")
            .to_string();
        assert!(chinese_title.contains("session 页面还没有已加载文档"));
        assert!(chinese_title.contains("HTTP 操作失败"));

        let chinese_set_cookie = page
            .set_cookie("sid", "1", None, None, None)
            .expect_err("set_cookie() should require an explicit scope in Chinese")
            .to_string();
        assert!(chinese_set_cookie.contains("session 页面没有当前 url；请显式传入 url"));
        assert!(chinese_set_cookie.contains("HTTP 操作失败"));
    }

    #[test]
    fn session_cookie_param_url_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = Session::new(SessionOptions::default()).expect("create session page");
        let explicit_url = [SessionCookieParam {
            name: "sid".to_string(),
            value: "1".to_string(),
            url: Some("not a url".to_string()),
            domain: None,
            path: None,
            secure: false,
            http_only: false,
            same_site: None,
        }];
        let english_explicit_url = page
            .set_cookies(explicit_url.as_slice())
            .expect_err("set_cookies() should reject invalid explicit cookie url")
            .to_string();
        assert!(english_explicit_url.contains("invalid url `not a url`"));

        let domain_url = [SessionCookieParam {
            name: "sid".to_string(),
            value: "1".to_string(),
            url: None,
            domain: Some("[bad".to_string()),
            path: Some("/".to_string()),
            secure: false,
            http_only: false,
            same_site: None,
        }];
        let english_domain_url = page
            .set_cookies(domain_url.as_slice())
            .expect_err("set_cookies() should reject invalid domain-derived cookie url")
            .to_string();
        assert!(english_domain_url.contains("invalid url `http://[bad/`"));

        Settings::set_language("cn");

        let chinese_explicit_url = page
            .set_cookies(explicit_url.as_slice())
            .expect_err("set_cookies() should localize invalid explicit cookie url")
            .to_string();
        assert!(chinese_explicit_url.contains("无效的 url `not a url`"));
        assert!(chinese_explicit_url.contains("HTTP 操作失败"));

        let chinese_domain_url = page
            .set_cookies(domain_url.as_slice())
            .expect_err("set_cookies() should localize invalid domain-derived cookie url")
            .to_string();
        assert!(chinese_domain_url.contains("无效的 url `http://[bad/`"));
        assert!(chinese_domain_url.contains("HTTP 操作失败"));
    }

    #[test]
    fn session_lock_poisoned_runtime_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = Session::new(SessionOptions::default()).expect("create session page");
        poison_mutex(Arc::clone(&page.none_element_config));

        let english = page
            .set_none_element_value(Some("missing"), true)
            .expect_err("set_none_element_value() should surface poisoned config")
            .to_string();
        assert!(english.contains("none element runtime config lock poisoned"));

        Settings::set_language("cn");

        let chinese = page
            .set_raise_when_ele_not_found(true)
            .expect_err("set_raise_when_ele_not_found() should localize poisoned config")
            .to_string();
        assert!(chinese.contains("未找到元素运行时配置锁已损坏"));
    }

    #[test]
    fn session_options_set_cookies_accepts_multi_format_inputs() {
        let mut options = SessionOptions::default();
        options
            .set_cookies("sid=abc; domain=.example.test; path=/shared; secure; httpOnly")
            .expect("set cookies from text");
        assert_eq!(options.cookies.len(), 1);
        assert_eq!(options.cookies[0].name, "sid");
        assert_eq!(options.cookies[0].domain.as_deref(), Some(".example.test"));
        assert!(options.cookies[0].secure);
        assert!(options.cookies[0].http_only);

        let cookies = json!({
            "api": "1",
            "domain": "api.example.test",
            "path": "/"
        });
        options
            .set_cookies(&cookies)
            .expect("replace cookies from json");
        assert_eq!(options.cookies.len(), 1);
        assert_eq!(options.cookies[0].name, "api");
        assert_eq!(
            options.cookies[0].domain.as_deref(),
            Some("api.example.test")
        );
        assert_eq!(options.cookies[0].path.as_deref(), Some("/"));

        options.clear_cookies();
        assert!(options.cookies.is_empty());
    }

    #[test]
    fn session_options_from_ini_loads_reference_drissionpage_configs_file() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .expect("repository root")
            .to_path_buf();
        let config_path = repo_root
            .join("参考项目")
            .join("DrissionPage-master")
            .join("DrissionPage")
            .join("_configs")
            .join("configs.ini");

        let options = SessionOptions::from_ini(Some(config_path.as_path()))
            .expect("load DrissionPage reference session options ini");

        assert_eq!(options.timeout_secs, 10);
        assert_eq!(options.download_path, std::path::PathBuf::from("."));
        assert_eq!(options.retry_times, 3);
        assert_eq!(options.retry_interval_millis, 2_000);
        assert!(options.http_proxy.is_none());
        assert!(options.https_proxy.is_none());
        assert!(options.verify);
        assert!(options.trust_env);
        assert_eq!(options.max_redirects, Some(30));
        assert_eq!(options.source_ini_path(), Some(config_path.as_path()));
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value.contains("Mozilla/5.0"))
        );
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "accept" && value.contains("text/html"))
        );
    }

    #[test]
    fn session_options_from_ini_none_loads_default_configs_file() {
        let options = SessionOptions::from_ini(None).expect("load default session options ini");

        assert_eq!(options.timeout_secs, 10);
        assert_eq!(options.download_path, std::path::PathBuf::from("."));
        assert_eq!(options.retry_times, 3);
        assert_eq!(options.retry_interval_millis, 2_000);
        assert!(options.source_ini_path.is_some());
        assert!(options.source_ini_path().is_some());
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value.contains("Mozilla/5.0"))
        );
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "accept-charset" && value.contains("utf-8"))
        );
    }

    #[test]
    fn session_options_from_ini_none_prefers_project_dp_configs_file() {
        let dir = make_temp_dir("session-options-project-config");
        fs::create_dir_all(&dir).expect("create temp dir");
        let project_ini = dir.join("dp_configs.ini");
        fs::write(
            &project_ini,
            "[session_options]\nheaders = {'user-agent': 'ProjectSession/1.0'}\n\n[timeouts]\nbase = 7\n",
        )
        .expect("write project session configs ini");
        let _guard = CurrentDirGuard::change_to(dir.as_path());

        let resolved = resolve_session_options_ini_path(None).expect("resolve project session ini");
        let options = SessionOptions::from_ini(None).expect("load project session configs ini");

        assert_eq!(
            fs::canonicalize(&resolved).expect("canonicalize resolved project session ini"),
            fs::canonicalize(&project_ini).expect("canonicalize expected project session ini")
        );
        assert_eq!(options.timeout_secs, 7);
        assert!(
            options
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value == "ProjectSession/1.0")
        );
        assert_eq!(
            options
                .source_ini_path()
                .and_then(|path| fs::canonicalize(path).ok()),
            Some(fs::canonicalize(&project_ini).expect("canonicalize source project session ini"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_from_ini_parse_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let dir = make_temp_dir("session-options-parse-error");
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("session.ini");
        fs::write(&config_path, "[session_options]\nheaders = {bad}\n")
            .expect("write invalid session ini");

        let english = SessionOptions::from_ini(Some(config_path.as_path()))
            .expect_err("invalid session ini should fail")
            .to_string();
        assert!(english.contains("invalid headers in session options ini:"));

        Settings::set_language("cn");

        let chinese = SessionOptions::from_ini(Some(config_path.as_path()))
            .expect_err("invalid session ini should fail in Chinese")
            .to_string();
        assert!(chinese.contains("session options ini 中的 headers 无效"));
        assert!(chinese.contains("HTTP 操作失败"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_from_ini_options_false_returns_defaults_without_reading_file() {
        let dir = make_temp_dir("session-options-read-file-false");
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("session.ini");
        let mut options = SessionOptions::default();
        options
            .set_user_agent(Some("OpenPage/ReadFalse".to_string()))
            .set_auth(Some(("alice".to_string(), "secret".to_string())));
        options
            .save(Some(config_path.as_path()))
            .expect("write session options ini");

        let loaded = SessionOptions::from_ini_options(false, Some(config_path.as_path()))
            .expect("create default session options");

        assert_eq!(loaded.timeout_secs, 10);
        assert_eq!(loaded.download_path, std::path::PathBuf::from("."));
        assert_eq!(loaded.retry_times, 3);
        assert_eq!(loaded.retry_interval_millis, 2_000);
        assert!(
            loaded
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value.contains("Mozilla/5.0"))
        );
        assert!(loaded.user_agent.is_none());
        assert!(loaded.auth.is_none());
        assert!(loaded.source_ini_path.is_none());
        assert_eq!(loaded.source_ini_path(), None);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_new_matches_from_ini_options_semantics() {
        let dir = make_temp_dir("session-options-new-wrapper");
        let config_path = dir.join("session.ini");
        let mut options = SessionOptions::default();
        options
            .set_user_agent(Some("OpenPage/SessionNew".to_string()))
            .set_auth(Some(("alice".to_string(), "secret".to_string())));
        options
            .save(Some(config_path.as_path()))
            .expect("write wrapped session ini");

        let loaded = SessionOptions::new(true, Some(config_path.as_path()))
            .expect("load session options via new()");
        let defaults = SessionOptions::new(false, Some(config_path.as_path()))
            .expect("default session options");

        assert_eq!(loaded.user_agent.as_deref(), Some("OpenPage/SessionNew"));
        assert_eq!(
            loaded.auth.as_ref(),
            Some(&("alice".to_string(), "secret".to_string()))
        );
        assert!(defaults.user_agent.is_none());
        assert!(defaults.auth.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_save_preserves_browser_sections_from_template_ini() {
        let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .expect("repository root")
            .to_path_buf();
        let source_path = repo_root
            .join("参考项目")
            .join("DrissionPage-master")
            .join("DrissionPage")
            .join("_configs")
            .join("configs.ini");
        let dir = make_temp_dir("session-options-save-template");
        fs::create_dir_all(&dir).expect("create temp dir");
        let target_path = dir.join("session.ini");
        let cookies = vec![SessionCookieParam {
            name: "sid".to_string(),
            value: "abc".to_string(),
            url: Some("https://example.test/".to_string()),
            domain: None,
            path: Some("/".to_string()),
            secure: true,
            http_only: true,
            same_site: Some("Lax".to_string()),
        }];

        let mut options = SessionOptions::from_ini(Some(source_path.as_path()))
            .expect("load DrissionPage reference session options ini");
        options
            .set_download_path("downloads")
            .set_user_agent(Some("OpenPage/SessionIni".to_string()));
        options
            .set_cookies(&cookies)
            .expect("set ini session cookies");
        options
            .set_auth(Some(("alice".to_string(), "secret".to_string())))
            .set_params(&[("page".to_string(), "2".to_string())])
            .set_cert(Some(SessionCert::PemPair {
                cert: std::path::PathBuf::from("client.pem"),
                key: std::path::PathBuf::from("client.key"),
            }))
            .set_verify(false)
            .set_trust_env(false)
            .set_max_redirects(None)
            .set_proxies(Some("http://127.0.0.1:7890".to_string()), None)
            .set_retry(Some(4), Some(250));

        let saved_path = options
            .save(Some(target_path.as_path()))
            .expect("save session options ini");
        let saved = fs::read_to_string(&saved_path).expect("read saved session ini");
        let loaded = SessionOptions::from_ini(Some(saved_path.as_path()))
            .expect("reload saved session options ini");

        assert_eq!(saved_path, target_path);
        assert!(saved.contains("[chromium_options]"));
        assert!(saved.contains("address = 127.0.0.1:9222"));
        assert!(saved.contains("[session_options]"));
        assert!(saved.contains("OpenPage/SessionIni"));
        assert_eq!(loaded.download_path, std::path::PathBuf::from("downloads"));
        assert_eq!(loaded.user_agent.as_deref(), Some("OpenPage/SessionIni"));
        assert_eq!(
            loaded.auth,
            Some(("alice".to_string(), "secret".to_string()))
        );
        assert_eq!(loaded.params, vec![("page".to_string(), "2".to_string())]);
        assert_eq!(loaded.cookies, cookies);
        assert_eq!(
            loaded.cert,
            Some(SessionCert::PemPair {
                cert: std::path::PathBuf::from("client.pem"),
                key: std::path::PathBuf::from("client.key"),
            })
        );
        assert!(!loaded.verify);
        assert!(!loaded.trust_env);
        assert_eq!(loaded.max_redirects, None);
        assert_eq!(loaded.http_proxy.as_deref(), Some("http://127.0.0.1:7890"));
        assert!(loaded.https_proxy.is_none());
        assert_eq!(loaded.retry_times, 4);
        assert_eq!(loaded.retry_interval_millis, 250);
        assert!(
            loaded
                .headers
                .iter()
                .any(|(name, value)| name == "user-agent" && value.contains("Mozilla/5.0"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_options_save_to_default_writes_default_configs_ini() {
        let saved_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("configs.ini");
        let _guard = RestoreFileGuard::new(saved_path.clone());
        let mut options = SessionOptions::default();
        options.set_user_agent(Some("OpenPageDefaultSession/1.0".to_string()));

        let returned = options
            .save_to_default()
            .expect("save session options to default ini");

        assert_eq!(returned, saved_path);

        let saved = fs::read_to_string(&saved_path).expect("read saved default session ini");
        assert!(saved.contains("[session_options]"));
        assert!(saved.contains("OpenPageDefaultSession/1.0"));
    }

    #[test]
    fn session_runtime_timeout_retry_and_close_match_reference_behavior() {
        let page = Session::new(SessionOptions::default()).expect("session page");

        assert_eq!(page.timeout_secs().expect("default timeout"), 10);
        assert_eq!(page.retry_times().expect("default retry times"), 3);
        assert_eq!(
            page.download_path().expect("default download path"),
            env::current_dir()
                .expect("current dir")
                .display()
                .to_string()
        );
        assert_eq!(
            page.retry_interval_millis()
                .expect("default retry interval"),
            2_000
        );
        assert_eq!(page.retry_interval().expect("default retry interval"), 2.0);

        page.set_timeout(7).expect("set timeout");
        page.set_retry(5, 0.25).expect("set retry");
        page.close().expect("close session");

        assert_eq!(page.timeout_secs().expect("updated timeout"), 7);
        assert_eq!(page.retry_times().expect("updated retry times"), 5);
        assert_eq!(
            page.retry_interval_millis()
                .expect("updated retry interval"),
            250
        );
        assert_eq!(page.retry_interval().expect("updated retry interval"), 0.25);
        assert_eq!(page.forced_encoding().expect("forced encoding"), None);
    }

    #[test]
    fn session_download_path_resolution_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let dir = make_temp_dir("session-download-path-cwd");
        fs::create_dir_all(&dir).expect("create cwd temp dir");
        let guard = CurrentDirGuard::change_to(&dir);
        fs::remove_dir_all(&dir).expect("remove cwd temp dir");

        let english = super::normalize_session_download_path(std::path::Path::new("downloads"))
            .expect_err("unresolvable cwd should fail")
            .to_string();
        assert!(english.contains("failed to resolve session download path downloads"));
        assert!(english.contains("io error"));

        Settings::set_language("cn");

        let chinese = super::normalize_session_download_path(std::path::Path::new("downloads"))
            .expect_err("unresolvable cwd should localize")
            .to_string();
        assert!(chinese.contains("解析 session 下载路径 downloads 失败"));
        assert!(chinese.contains("IO 错误"));

        drop(guard);
    }

    #[test]
    fn session_download_uses_runtime_download_path_and_tracks_last_download() {
        let download_dir = make_temp_dir("session-download");
        let page = Session::new(SessionOptions::default()).expect("session page");
        page.set_download_path(&download_dir)
            .expect("set download path");
        assert_eq!(
            page.download_path().expect("download path"),
            download_dir.display().to_string()
        );

        let body = "downloaded body";
        let (address, handle) = spawn_capture_server("200 OK", body);
        let url = format!("{address}/files/openpage.txt");

        let saved_path = page.download(&url).expect("download file");
        assert_eq!(
            saved_path,
            download_dir.join("openpage.txt").display().to_string()
        );
        assert_eq!(
            fs::read_to_string(&saved_path).expect("downloaded contents"),
            body
        );

        let last_download = page
            .last_download()
            .expect("last download result")
            .expect("download record");
        assert_eq!(last_download.url, url);
        assert_eq!(last_download.final_url, url);
        assert_eq!(last_download.path, saved_path);
        assert_eq!(last_download.filename, "openpage.txt".to_string());
        assert_eq!(last_download.status_code, 200);
        assert_eq!(last_download.total_bytes, body.len() as u64);
        assert_eq!(
            last_download.content_type.as_deref(),
            Some("text/plain; charset=utf-8")
        );

        let _ = handle.join().expect("server thread");
        let _ = fs::remove_file(&saved_path);
        let _ = fs::remove_dir_all(&download_dir);
    }

    #[test]
    fn session_download_to_writes_explicit_target_path() {
        let target_dir = make_temp_dir("session-download-explicit");
        let target_path = target_dir.join("custom.bin");
        let body = "explicit target";
        let (address, handle) = spawn_capture_server("200 OK", body);
        let url = format!("{address}/payload.bin");
        let page = Session::new(SessionOptions::default()).expect("session page");

        let saved_path = page
            .download_to(&url, &target_path)
            .expect("download explicit path");
        assert_eq!(saved_path, target_path.display().to_string());
        assert_eq!(
            fs::read_to_string(&target_path).expect("downloaded contents"),
            body
        );
        assert_eq!(
            page.last_download()
                .expect("last download")
                .expect("record")
                .filename,
            "custom.bin".to_string()
        );

        let _ = handle.join().expect("server thread");
        let _ = fs::remove_file(&target_path);
        let _ = fs::remove_dir_all(&target_dir);
    }

    #[test]
    fn session_download_file_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let page = Session::new(SessionOptions {
            retry_times: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let blocker = make_temp_file("session-download-blocker", "not a directory");
        let target_path = blocker.join("payload.bin");

        let (english_address, english_handle) = spawn_capture_server("200 OK", "payload");
        let english_url = format!("{english_address}/payload.bin");
        let english = page
            .download_to(&english_url, &target_path)
            .expect_err("download parent directory creation should fail")
            .to_string();
        assert!(english.contains("failed to create parent directory session download file"));
        assert!(english.contains(&blocker.display().to_string()));
        let _ = english_handle.join();

        Settings::set_language("cn");

        let (chinese_address, chinese_handle) = spawn_capture_server("200 OK", "payload");
        let chinese_url = format!("{chinese_address}/payload.bin");
        let chinese = page
            .download_to(&chinese_url, &target_path)
            .expect_err("download parent directory creation should fail in Chinese")
            .to_string();
        assert!(chinese.contains("session 下载文件 create parent directory 失败"));
        assert!(chinese.contains(&blocker.display().to_string()));
        let _ = chinese_handle.join();

        let _ = fs::remove_file(&blocker);
    }

    #[test]
    fn session_download_status_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let page = Session::new(SessionOptions {
            retry_times: 0,
            retry_interval_millis: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let (english_address, english_handle) = spawn_capture_server("404 Not Found", "missing");
        let english_url = format!("{english_address}/missing.bin");
        let english_error = page
            .download(&english_url)
            .expect_err("download status validation should fail")
            .to_string();
        assert!(english_error.contains(&format!(
            "download request returned status 404 for {english_url}"
        )));
        let _ = english_handle.join().expect("english server thread");

        Settings::set_language("cn");

        let (chinese_address, chinese_handle) = spawn_capture_server("404 Not Found", "missing");
        let chinese_url = format!("{chinese_address}/missing.bin");
        let chinese_error = page
            .download(&chinese_url)
            .expect_err("download status validation should fail in Chinese")
            .to_string();
        assert!(chinese_error.contains(&format!("下载请求 {chinese_url} 返回状态码 404")));
        assert!(chinese_error.contains("HTTP 操作失败"));
        let _ = chinese_handle.join().expect("chinese server thread");
    }

    #[test]
    fn session_cookie_queries_cover_all_domains_and_metadata() {
        let page = Session::new(SessionOptions::default()).expect("session page");
        let current_url =
            Url::parse("https://www.example.test/shared/page").expect("current cookie url");
        let other_url = Url::parse("https://other.test/").expect("other cookie url");
        {
            let mut state = page.lock_state().expect("lock state");
            state.url = Some(current_url.as_str().to_string());
        }

        let cookie_jar = page.lock_state().expect("lock state").cookie_jar.clone();
        cookie_jar.add_cookie_str("host=1; Path=/shared", &current_url);
        cookie_jar.add_cookie_str(
            "shared=2; Domain=example.test; Path=/shared; Secure; HttpOnly; SameSite=Strict",
            &current_url,
        );
        cookie_jar.add_cookie_str("other=3; Domain=other.test; Path=/", &other_url);

        let current = page.cookies().expect("current cookies");
        assert_eq!(current.len(), 2);
        assert!(current.iter().any(|cookie| {
            cookie.name == "host" && cookie.domain.as_deref() == Some("www.example.test")
        }));
        assert!(current.iter().any(|cookie| {
            cookie.name == "shared" && cookie.domain.as_deref() == Some("example.test")
        }));

        let all = page.cookies_all_domains().expect("all-domain cookies");
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|cookie| cookie.name == "other"));

        let detailed_current = page
            .cookies_detailed(false)
            .expect("current detailed cookies");
        assert_eq!(detailed_current.len(), 2);
        let host = detailed_current
            .iter()
            .find(|cookie| cookie.name == "host")
            .expect("host cookie");
        assert!(host.host_only);
        assert_eq!(host.path.as_deref(), Some("/shared"));
        assert!(!host.secure);
        assert!(!host.http_only);
        assert_eq!(host.same_site, None);
        assert!(!host.persistent);

        let shared = detailed_current
            .iter()
            .find(|cookie| cookie.name == "shared")
            .expect("shared cookie");
        assert!(!shared.host_only);
        assert_eq!(shared.path.as_deref(), Some("/shared"));
        assert!(shared.secure);
        assert!(shared.http_only);
        assert_eq!(shared.same_site.as_deref(), Some("Strict"));
        assert!(!shared.persistent);

        let detailed_all = page.cookies_detailed(true).expect("all detailed cookies");
        assert_eq!(detailed_all.len(), 3);
        assert!(detailed_all.iter().any(|cookie| cookie.name == "other"));
    }

    #[test]
    fn session_page_set_cookies_accepts_text_and_json_inputs() {
        let page = Session::new(SessionOptions::default()).expect("session page");
        {
            let mut state = page.lock_state().expect("lock state");
            state.url = Some("https://www.example.test/shared/page".to_string());
        }

        page.set_cookies("host=1; path=/shared")
            .expect("set host cookie from text");
        let shared = json!({
            "shared": "2",
            "domain": "example.test",
            "path": "/shared",
            "secure": true,
            "httpOnly": true,
            "sameSite": "Strict"
        });
        page.set_cookies(&shared)
            .expect("set shared cookie from json");

        let cookies = page.cookies().expect("current cookies");
        assert_eq!(cookies.len(), 2);
        assert!(cookies.iter().any(|cookie| {
            cookie.name == "host" && cookie.domain.as_deref() == Some("www.example.test")
        }));
        assert!(cookies.iter().any(|cookie| {
            cookie.name == "shared" && cookie.domain.as_deref() == Some("example.test")
        }));

        let detailed = page.cookies_detailed(false).expect("detailed cookies");
        let host = detailed
            .iter()
            .find(|cookie| cookie.name == "host")
            .expect("host cookie");
        assert_eq!(host.path.as_deref(), Some("/shared"));
        assert!(!host.secure);
        assert!(!host.http_only);

        let shared = detailed
            .iter()
            .find(|cookie| cookie.name == "shared")
            .expect("shared cookie");
        assert_eq!(shared.path.as_deref(), Some("/shared"));
        assert!(shared.secure);
        assert!(shared.http_only);
        assert_eq!(shared.same_site.as_deref(), Some("Strict"));
    }

    #[test]
    fn session_get_retries_failed_responses_until_success() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let (url, handle) = spawn_retry_server(
            &["500 Internal Server Error", "200 OK"],
            &["retry", "done"],
            Arc::clone(&attempts),
        );
        let page = Session::new(SessionOptions {
            retry_times: 1,
            retry_interval_millis: 0,
            ..SessionOptions::default()
        })
        .expect("session page should initialize");

        assert!(
            page.get(&url)
                .expect("get should retry then succeed")
                .is_success()
        );
        assert_eq!(page.status_code().expect("status code"), Some(200));
        assert_eq!(page.html().expect("html body"), "done".to_string());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(handle.join().expect("server thread"), vec!["GET", "GET"]);
    }

    #[test]
    fn session_post_retries_failed_responses_until_success() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let (url, handle) = spawn_retry_server(
            &["502 Bad Gateway", "200 OK"],
            &["retry", "posted"],
            Arc::clone(&attempts),
        );
        let page = Session::new(SessionOptions {
            retry_times: 1,
            retry_interval_millis: 0,
            ..SessionOptions::default()
        })
        .expect("session page should initialize");

        assert!(
            page.post(&url)
                .expect("post should retry then succeed")
                .is_success()
        );
        assert_eq!(page.status_code().expect("status code"), Some(200));
        assert_eq!(page.html().expect("html body"), "posted".to_string());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(handle.join().expect("server thread"), vec!["POST", "POST"]);
    }

    #[test]
    fn session_get_loads_local_file_paths() {
        let path = make_temp_file(
            "session-local",
            "<html><head><title>Local File</title></head><body>openpage</body></html>",
        );
        let page = Session::new(SessionOptions::default()).expect("session page");
        let file_url = Url::from_file_path(&path)
            .expect("build local file url")
            .to_string();

        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load file")
                .is_success()
        );
        assert_eq!(page.status_code().expect("status"), Some(200));
        assert!(page.url_available().expect("url available"));
        assert_eq!(page.title().expect("title"), Some("Local File".to_string()));
        assert!(page.html().expect("html").contains("openpage"));
        assert!(
            page.url()
                .expect("url")
                .expect("current url")
                .starts_with("file://")
        );
        assert!(page.get(&file_url).expect("load file url").is_success());
        let current_file_url = page.url().expect("url").expect("current url");
        let current_path = Url::parse(&current_file_url)
            .expect("parse current file url")
            .to_file_path()
            .expect("current file url path");
        assert_eq!(
            fs::canonicalize(current_path).expect("canonical current file path"),
            fs::canonicalize(&path).expect("canonical expected file path")
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn session_local_file_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let page = Session::new(SessionOptions::default()).expect("session page");
        let missing = env::temp_dir().join(format!(
            "openpage-missing-local-{}.html",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let file_url = Url::from_file_path(&missing)
            .expect("build missing file url")
            .to_string();

        let english = page
            .get(&file_url)
            .expect_err("missing local file should fail")
            .to_string();
        assert!(english.contains("failed to resolve session local file"));
        assert!(english.contains(&missing.display().to_string()));

        Settings::set_language("cn");

        let chinese = page
            .get(&file_url)
            .expect_err("missing local file should fail in Chinese")
            .to_string();
        assert!(chinese.contains("session 本地文件 resolve 失败"));
        assert!(chinese.contains(&missing.display().to_string()));
    }

    #[test]
    fn session_runtime_snapshot_exposes_current_configuration_and_cookies() {
        let page = Session::new(SessionOptions {
            timeout_secs: 21,
            user_agent: Some("OpenPage/TestAgent".to_string()),
            headers: vec![
                ("x-one".to_string(), "1".to_string()),
                ("accept".to_string(), "text/html".to_string()),
            ],
            cookies: vec![SessionCookieParam {
                name: "sid".to_string(),
                value: "abc".to_string(),
                url: Some("https://example.test/".to_string()),
                domain: None,
                path: Some("/".to_string()),
                secure: true,
                http_only: true,
                same_site: Some("Lax".to_string()),
            }],
            download_path: std::path::PathBuf::from("downloads"),
            retry_times: 4,
            retry_interval_millis: 250,
            http_proxy: Some("http://127.0.0.1:7890".to_string()),
            https_proxy: Some("http://127.0.0.1:7891".to_string()),
            params: vec![("page".to_string(), "2".to_string())],
            verify: false,
            auth: Some(("alice".to_string(), "secret".to_string())),
            stream: true,
            trust_env: false,
            max_redirects: Some(5),
            ..SessionOptions::default()
        })
        .expect("session page");

        let snapshot = page.session().expect("session runtime snapshot");
        let expected_download_path = std::env::current_dir()
            .expect("current dir")
            .join("downloads")
            .display()
            .to_string();

        assert_eq!(snapshot.timeout_secs, 21);
        assert_eq!(snapshot.timeout_secs(), 21);
        assert_eq!(snapshot.user_agent.as_deref(), Some("OpenPage/TestAgent"));
        assert_eq!(snapshot.user_agent(), Some("OpenPage/TestAgent"));
        assert_eq!(snapshot.download_path, expected_download_path);
        assert_eq!(snapshot.download_path(), expected_download_path);
        assert_eq!(snapshot.retry_times, 4);
        assert_eq!(snapshot.retry_times(), 4);
        assert_eq!(snapshot.retry_interval_millis, 250);
        assert_eq!(snapshot.retry_interval_millis(), 250);
        assert_eq!(
            snapshot.http_proxy.as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(snapshot.http_proxy(), Some("http://127.0.0.1:7890"));
        assert_eq!(
            snapshot.https_proxy.as_deref(),
            Some("http://127.0.0.1:7891")
        );
        assert_eq!(snapshot.https_proxy(), Some("http://127.0.0.1:7891"));
        assert_eq!(snapshot.params, vec![("page".to_string(), "2".to_string())]);
        assert_eq!(snapshot.params(), &[("page".to_string(), "2".to_string())]);
        assert_eq!(snapshot.param("page"), Some("2"));
        assert!(!snapshot.verify);
        assert!(!snapshot.verify());
        assert_eq!(
            snapshot.auth,
            Some(("alice".to_string(), "secret".to_string()))
        );
        assert_eq!(snapshot.auth(), Some(("alice", "secret")));
        assert!(snapshot.stream);
        assert!(snapshot.stream());
        assert!(snapshot.cert.is_none());
        assert!(snapshot.cert().is_none());
        assert!(!snapshot.trust_env);
        assert!(!snapshot.trust_env());
        assert_eq!(snapshot.max_redirects, Some(5));
        assert_eq!(snapshot.max_redirects(), Some(5));
        assert!(snapshot.current_url.is_none());
        assert!(snapshot.current_url().is_none());
        assert!(
            snapshot
                .headers
                .contains(&("accept".to_string(), "text/html".to_string()))
        );
        assert!(
            snapshot
                .headers
                .contains(&("x-one".to_string(), "1".to_string()))
        );
        assert_eq!(snapshot.header("Accept"), Some("text/html"));
        assert_eq!(snapshot.headers().len(), snapshot.headers.len());
        assert_eq!(snapshot.cookies.len(), 1);
        assert_eq!(snapshot.cookies().len(), 1);
        assert_eq!(snapshot.cookies[0].name, "sid".to_string());
        assert_eq!(snapshot.cookies[0].value, "abc".to_string());
        assert_eq!(snapshot.cookies[0].same_site.as_deref(), Some("Lax"));
    }

    #[test]
    fn session_set_params_and_auth_apply_to_requests() {
        let (address, handle) = spawn_capture_server("200 OK", "secured");
        let page = Session::new(SessionOptions::default()).expect("session page");
        let url = format!("{address}/items");

        page.set_params(&[
            ("foo".to_string(), "bar baz".to_string()),
            ("x".to_string(), "1".to_string()),
        ])
        .expect("set params");
        page.set_auth(Some(("alice".to_string(), "secret".to_string())))
            .expect("set auth");

        assert!(
            page.get(&url)
                .expect("request with params and auth")
                .is_success()
        );
        assert_eq!(page.html().expect("html body"), "secured".to_string());

        let request = handle.join().expect("server thread");
        assert!(
            request
                .lines()
                .next()
                .expect("request line")
                .contains("/items?foo=bar+baz&x=1 HTTP/1.1")
        );
        let auth_header = request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
            .expect("authorization header");
        let (_, value) = auth_header
            .split_once(':')
            .expect("authorization separator");
        let expected = BASE64_STANDARD.encode("alice:secret");
        assert_eq!(value.trim(), format!("Basic {expected}"));
    }

    #[test]
    fn session_stream_option_defers_body_loading_until_needed() {
        let (address, handle) = spawn_capture_server("200 OK", "streamed");
        let page = Session::new(SessionOptions {
            stream: true,
            ..SessionOptions::default()
        })
        .expect("session page");

        let response = page.get(&address).expect("streaming request");
        assert!(response.is_success());
        {
            let state = page.lock_state().expect("lock session state");
            assert_eq!(state.status_code, Some(200));
            assert!(state.pending_response.is_some());
            assert!(state.raw_data.is_none());
            assert!(state.body.is_none());
            assert_eq!(state.encoding.as_deref(), Some("utf-8"));
        }
        assert_eq!(
            response.encoding().map(str::to_string),
            Some("utf-8".to_string())
        );
        assert_eq!(
            page.html().expect("load streamed body"),
            "streamed".to_string()
        );
        {
            let state = page.lock_state().expect("lock session state");
            assert!(state.pending_response.is_none());
            assert!(state.raw_data.is_some());
            assert!(state.body.is_some());
        }

        handle.join().expect("server thread");
    }

    #[test]
    fn session_request_options_stream_override_disables_lazy_loading() {
        let (address, handle) = spawn_capture_server("200 OK", "override");
        let page = Session::new(SessionOptions {
            stream: true,
            ..SessionOptions::default()
        })
        .expect("session page");
        let request_options = SessionRequestOptions {
            stream: Some(false),
            ..SessionRequestOptions::default()
        };

        assert!(
            page.get_with_options(&address, &request_options)
                .expect("request with stream override")
                .is_success()
        );
        {
            let state = page.lock_state().expect("lock session state");
            assert!(state.pending_response.is_none());
            assert!(state.raw_data.is_some());
            assert_eq!(state.body.as_deref().map(AsRef::as_ref), Some("override"));
        }

        handle.join().expect("server thread");
    }

    #[test]
    fn session_page_set_stream_updates_runtime_stream_behavior() {
        let (address, handle) = spawn_capture_server("200 OK", "runtime-stream");
        let page = Session::new(SessionOptions::default()).expect("session page");

        page.set_stream(true).expect("enable runtime stream");
        assert!(page.stream().expect("runtime stream getter"));
        assert!(
            page.get(&address)
                .expect("runtime streaming request")
                .is_success()
        );
        assert!(
            page.lock_state()
                .expect("lock session state")
                .pending_response
                .is_some()
        );

        handle.join().expect("server thread");
    }

    #[test]
    fn session_options_initial_headers_apply_to_requests() {
        let (address, handle) = spawn_capture_server("200 OK", "init");
        let page = Session::new(SessionOptions {
            headers: vec![
                ("X-Init".to_string(), "present".to_string()),
                ("Referer".to_string(), "".to_string()),
            ],
            ..SessionOptions::default()
        })
        .expect("session page");
        let url = format!("{address}/headers");

        assert!(
            page.get(&url)
                .expect("request with initial headers")
                .is_success()
        );

        let request = handle.join().expect("server thread");
        assert!(
            request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-init: present"))
        );
        assert!(
            !request
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("referer:"))
        );
    }

    #[test]
    fn session_page_set_header_replaces_existing_header_case_insensitively() {
        let page = Session::new(SessionOptions::default()).expect("session page");
        page.set_headers(&[
            ("Accept".to_string(), "text/html".to_string()),
            ("Referer".to_string(), "".to_string()),
        ])
        .expect("set initial headers");
        page.set_header("accept", "application/json")
            .expect("replace accept header");
        page.set_header("X-Test", "1").expect("set x-test");
        page.set_header("x-test", "2").expect("replace x-test");

        let (address, handle) = spawn_capture_server("200 OK", "headers");
        let url = format!("{address}/headers");
        assert!(
            page.get(&url)
                .expect("request with updated headers")
                .is_success()
        );

        let request = handle.join().expect("server thread");
        assert!(
            request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("accept: application/json"))
        );
        assert!(
            !request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("accept: text/html"))
        );
        assert!(
            request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-test: 2"))
        );
        assert!(
            !request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-test: 1"))
        );
        assert!(
            !request
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("referer:"))
        );
    }

    #[test]
    fn session_options_initial_cookies_seed_session_cookie_store() {
        let current_url =
            Url::parse("https://www.example.test/shared/page").expect("current cookie url");
        let page = Session::new(SessionOptions {
            cookies: vec![
                SessionCookieParam {
                    name: "host".to_string(),
                    value: "1".to_string(),
                    url: Some(current_url.as_str().to_string()),
                    domain: None,
                    path: Some("/shared".to_string()),
                    secure: false,
                    http_only: false,
                    same_site: None,
                },
                SessionCookieParam {
                    name: "shared".to_string(),
                    value: "2".to_string(),
                    url: None,
                    domain: Some("example.test".to_string()),
                    path: Some("/shared".to_string()),
                    secure: true,
                    http_only: true,
                    same_site: Some("Lax".to_string()),
                },
            ],
            ..SessionOptions::default()
        })
        .expect("session page");
        {
            let mut state = page.lock_state().expect("lock state");
            state.url = Some(current_url.as_str().to_string());
        }

        let cookies = page.cookies().expect("current cookies");
        assert_eq!(cookies.len(), 2);
        assert!(cookies.iter().any(|cookie| cookie.name == "host"));
        assert!(cookies.iter().any(|cookie| cookie.name == "shared"));

        let detailed = page.cookies_detailed(false).expect("detailed cookies");
        let shared = detailed
            .iter()
            .find(|cookie| cookie.name == "shared")
            .expect("shared cookie");
        assert!(shared.secure);
        assert!(shared.http_only);
        assert_eq!(shared.same_site.as_deref(), Some("Lax"));
    }

    #[test]
    fn session_request_options_override_runtime_request_settings() {
        let page = Session::new(SessionOptions {
            retry_times: 0,
            retry_interval_millis: 999,
            ..SessionOptions::default()
        })
        .expect("session page");
        page.set_params(&[("base".to_string(), "1".to_string())])
            .expect("set base params");
        page.set_headers(&[
            ("X-Base".to_string(), "base".to_string()),
            ("Referer".to_string(), "http://default.test/".to_string()),
        ])
        .expect("set base headers");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind request options server");
        let address = format!("http://{}", listener.local_addr().expect("server addr"));
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (index, status) in ["500 Internal Server Error", "200 OK"].iter().enumerate() {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                requests.push(request);
                let body = if index == 0 { "retry" } else { "done" };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("write response");
            }
            requests
        });
        let url = format!("{address}/items");
        let request_options = SessionRequestOptions {
            retry_times: Some(1),
            retry_interval_millis: Some(0),
            headers: vec![
                ("X-Base".to_string(), "override".to_string()),
                ("Referer".to_string(), "".to_string()),
                ("X-Req".to_string(), "request".to_string()),
            ],
            params: vec![("req".to_string(), "2".to_string())],
            ..SessionRequestOptions::default()
        };

        assert!(
            page.get_with_options(&url, &request_options)
                .expect("request with overrides")
                .is_success()
        );
        let requests = handle.join().expect("server thread");
        assert_eq!(requests.len(), 2);
        let second_request = &requests[1];
        assert!(
            second_request
                .lines()
                .next()
                .expect("request line")
                .contains("/items?base=1&req=2 HTTP/1.1")
        );
        assert!(
            second_request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-base: override"))
        );
        assert!(
            second_request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-req: request"))
        );
        assert!(
            !second_request
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("referer:"))
        );
    }

    #[test]
    fn session_request_options_timeout_overrides_session_default() {
        let page = Session::new(SessionOptions {
            timeout_secs: 5,
            retry_times: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let (url, handle) = spawn_delayed_server(Duration::from_millis(1200));
        let request_options = SessionRequestOptions {
            timeout_secs: Some(1),
            ..SessionRequestOptions::default()
        };

        let result = page.get_with_options(&url, &request_options);
        assert!(result.is_err());
        handle.join().expect("server thread");
    }

    #[test]
    fn session_request_send_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let page = Session::new(SessionOptions {
            retry_times: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let request_options = SessionRequestOptions {
            timeout_secs: Some(1),
            ..SessionRequestOptions::default()
        };

        let (english_url, english_handle) = spawn_delayed_server(Duration::from_millis(1200));
        let english = page
            .get_with_options(&english_url, &request_options)
            .expect_err("timeout should fail")
            .to_string();
        assert!(english.contains(&format!("session GET request failed for {english_url}")));
        let _ = english_handle.join();

        Settings::set_language("cn");

        let (chinese_url, chinese_handle) = spawn_delayed_server(Duration::from_millis(1200));
        let chinese = page
            .get_with_options(&chinese_url, &request_options)
            .expect_err("timeout should fail in Chinese")
            .to_string();
        assert!(chinese.contains(&format!("session GET 请求 {chinese_url} 失败")));
        assert!(chinese.contains("HTTP 操作失败"));
        let _ = chinese_handle.join();
    }

    #[test]
    fn session_response_body_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let page = Session::new(SessionOptions {
            retry_times: 0,
            ..SessionOptions::default()
        })
        .expect("session page");

        let (english_url, english_handle) = spawn_truncated_body_server();
        let english = page
            .get(&english_url)
            .expect_err("truncated response body should fail")
            .to_string();
        assert!(english.contains(&format!(
            "failed to read session response body for {english_url}"
        )));
        let _ = english_handle.join();

        let stream_page = Session::new(SessionOptions {
            retry_times: 0,
            stream: true,
            ..SessionOptions::default()
        })
        .expect("stream session page");
        let (english_stream_url, english_stream_handle) = spawn_truncated_body_server();
        assert!(
            stream_page
                .get(&english_stream_url)
                .expect("stream get should store pending response")
                .is_success()
        );
        let english_stream = stream_page
            .html()
            .expect_err("streamed truncated response body should fail")
            .to_string();
        assert!(english_stream.contains(&format!(
            "failed to read session response body for {english_stream_url}"
        )));
        let _ = english_stream_handle.join();

        Settings::set_language("cn");

        let (chinese_url, chinese_handle) = spawn_truncated_body_server();
        let chinese = page
            .get(&chinese_url)
            .expect_err("truncated response body should fail in Chinese")
            .to_string();
        assert!(chinese.contains(&format!("读取 session 响应体 {chinese_url} 失败")));
        assert!(chinese.contains("HTTP 操作失败"));
        let _ = chinese_handle.join();

        let chinese_stream_page = Session::new(SessionOptions {
            retry_times: 0,
            stream: true,
            ..SessionOptions::default()
        })
        .expect("Chinese stream session page");
        let (chinese_stream_url, chinese_stream_handle) = spawn_truncated_body_server();
        assert!(
            chinese_stream_page
                .get(&chinese_stream_url)
                .expect("stream get should store pending response in Chinese")
                .is_success()
        );
        let chinese_stream = chinese_stream_page
            .html()
            .expect_err("streamed truncated response body should fail in Chinese")
            .to_string();
        assert!(chinese_stream.contains(&format!("读取 session 响应体 {chinese_stream_url} 失败")));
        assert!(chinese_stream.contains("HTTP 操作失败"));
        let _ = chinese_stream_handle.join();
    }

    #[test]
    fn session_requests_set_default_referer_from_current_url() {
        let (first_address, first_handle) = spawn_capture_server("200 OK", "first");
        let page = Session::new(SessionOptions::default()).expect("session page");
        let first_url = format!("{first_address}/first");

        assert!(page.get(&first_url).expect("first request").is_success());
        let first_request = first_handle.join().expect("server thread");
        let first_referer = first_request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("referer:"))
            .expect("first referer header");
        assert_eq!(
            first_referer.trim().to_ascii_lowercase(),
            format!("referer: {first_address}")
        );

        let (second_address, second_handle) = spawn_capture_server("200 OK", "second");
        let second_url = format!("{second_address}/next");
        assert!(page.get(&second_url).expect("second request").is_success());
        let second_request = second_handle.join().expect("server thread");
        let second_referer = second_request
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("referer:"))
            .expect("second referer header");
        assert_eq!(
            second_referer.trim().to_ascii_lowercase(),
            format!("referer: {first_url}")
        );
    }

    #[test]
    fn session_set_encoding_updates_current_and_future_documents() {
        let path = make_temp_bytes("session-encoding", b"caf\xe9");
        let page = Session::new(SessionOptions::default()).expect("session page");

        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load bytes file")
                .is_success()
        );
        assert_eq!(
            page.html().expect("default html"),
            "caf\u{fffd}".to_string()
        );

        page.set_encoding(Some("windows-1252".to_string()))
            .expect("set encoding");
        assert_eq!(
            page.forced_encoding().expect("forced encoding"),
            Some("windows-1252".to_string())
        );
        assert_eq!(page.html().expect("decoded html"), "caf\u{e9}".to_string());
        assert_eq!(
            page.encoding().expect("current encoding"),
            Some("windows-1252".to_string())
        );

        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("reload bytes file")
                .is_success()
        );
        assert_eq!(page.html().expect("reloaded html"), "caf\u{e9}".to_string());

        page.set_encoding(None).expect("clear encoding");
        assert_eq!(page.forced_encoding().expect("forced encoding"), None);
        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("reload bytes file after clear")
                .is_success()
        );
        assert_eq!(
            page.html().expect("auto decoded html"),
            "caf\u{fffd}".to_string()
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn session_set_proxies_routes_http_requests_through_proxy() {
        let (proxy_url, handle) = spawn_capture_server("200 OK", "proxied");
        let page = Session::new(SessionOptions::default()).expect("session page");

        page.set_proxies(Some(proxy_url.clone()), None)
            .expect("set proxy");
        assert!(
            page.get("http://example.test/proxy-path")
                .expect("request through proxy")
                .is_success()
        );
        assert_eq!(page.html().expect("html body"), "proxied".to_string());

        let request = handle.join().expect("server thread");
        assert_eq!(
            request.lines().next().expect("request line"),
            "GET http://example.test/proxy-path HTTP/1.1"
        );
    }

    #[test]
    fn session_set_max_redirects_controls_follow_behavior() {
        let page = Session::new(SessionOptions::default()).expect("session page");

        page.set_max_redirects(Some(0)).expect("disable redirects");
        let (first_address, first_handle) = spawn_redirect_server(1);
        assert!(
            page.get(&format!("{first_address}/first"))
                .expect("request without redirects")
                .is_success()
        );
        assert_eq!(page.status_code().expect("status"), Some(302));
        assert!(page.url_available().expect("url available"));
        assert_eq!(
            page.url().expect("url").expect("current url"),
            format!("{first_address}/first")
        );
        assert_eq!(
            first_handle.join().expect("server thread"),
            vec!["GET /first HTTP/1.1".to_string()]
        );

        page.set_max_redirects(Some(1)).expect("allow one redirect");
        let (second_address, second_handle) = spawn_redirect_server(2);
        assert!(
            page.get(&format!("{second_address}/first"))
                .expect("request with redirect")
                .is_success()
        );
        assert_eq!(page.status_code().expect("status"), Some(200));
        assert!(page.url_available().expect("url available"));
        assert_eq!(page.html().expect("html body"), "done".to_string());
        assert_eq!(
            page.url().expect("url").expect("current url"),
            format!("{second_address}/final")
        );
        assert_eq!(
            second_handle.join().expect("server thread"),
            vec![
                "GET /first HTTP/1.1".to_string(),
                "GET /final HTTP/1.1".to_string()
            ]
        );
    }

    #[test]
    fn session_url_available_is_false_for_unsuccessful_status() {
        let page = Session::new(SessionOptions {
            retry_times: 0,
            retry_interval_millis: 0,
            ..SessionOptions::default()
        })
        .expect("session page");
        let (address, handle) = spawn_capture_server("404 Not Found", "missing");

        assert!(!page.get(&address).expect("request 404").is_success());
        assert_eq!(page.status_code().expect("status"), Some(404));
        assert!(!page.url_available().expect("url available"));

        let _ = handle.join().expect("server thread");
    }

    #[test]
    fn session_new_reports_missing_client_cert_files() {
        let missing = env::temp_dir().join(format!(
            "openpage-missing-cert-{}.pem",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let error = Session::new(SessionOptions {
            cert: Some(SessionCert::Pem(missing.clone())),
            ..SessionOptions::default()
        })
        .expect_err("missing cert should fail");

        match error {
            OpenPageError::Io(message) => {
                assert!(message.contains("failed to read cert"));
                assert!(message.contains(&missing.display().to_string()));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn session_new_proxy_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let english_error = Session::new(SessionOptions {
            http_proxy: Some("://bad-proxy".to_string()),
            ..SessionOptions::default()
        })
        .expect_err("invalid proxy should fail")
        .to_string();
        assert!(english_error.contains("invalid session http proxy `://bad-proxy`"));

        Settings::set_language("cn");

        let chinese_error = Session::new(SessionOptions {
            https_proxy: Some("://bad-proxy".to_string()),
            ..SessionOptions::default()
        })
        .expect_err("invalid proxy should fail in Chinese")
        .to_string();
        assert!(chinese_error.contains("session https 代理 `://bad-proxy` 无效"));
        assert!(chinese_error.contains("HTTP 操作失败"));
    }

    #[test]
    fn session_new_identity_parse_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();
        let dir = make_temp_dir("session-invalid-cert");
        fs::create_dir_all(&dir).expect("create invalid cert dir");
        let cert_path = dir.join("client.pem");
        fs::write(&cert_path, "not a pem").expect("write invalid cert");

        let english_error = Session::new(SessionOptions {
            cert: Some(SessionCert::Pem(cert_path.clone())),
            ..SessionOptions::default()
        })
        .expect_err("invalid cert should fail")
        .to_string();
        assert!(english_error.contains("failed to parse session identity"));

        Settings::set_language("cn");

        let chinese_error = Session::new(SessionOptions {
            cert: Some(SessionCert::Pem(cert_path.clone())),
            ..SessionOptions::default()
        })
        .expect_err("invalid cert should fail in Chinese")
        .to_string();
        assert!(chinese_error.contains("解析 session identity 失败"));
        assert!(chinese_error.contains("HTTP 操作失败"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_find_supports_xpath_queries() {
        let root = snapshot_find(HTML, "xpath://section[@id='root']").expect("root should exist");
        let second_item = root
            .find("xpath:./ul/li[@data-kind='b']")
            .expect("second item should exist");
        let all_items = root
            .find_all("xpath:.//li")
            .expect("all xpath items should exist");

        assert_eq!(second_item.tag().expect("tag"), "li".to_string());
        assert_eq!(
            second_item.text().expect("second item text"),
            Some("beta".to_string())
        );
        assert_eq!(all_items.len(), 2);
    }

    #[test]
    fn snapshot_xpath_errors_follow_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let english = snapshot_find(HTML, "xpath://[")
            .expect_err("invalid xpath should fail")
            .to_string();
        assert!(english.contains("unsupported locator"));
        assert!(english.contains("invalid xpath `//[`"));

        Settings::set_language("cn");

        let chinese = snapshot_find(HTML, "xpath://[")
            .expect_err("invalid xpath should localize")
            .to_string();
        assert!(chinese.contains("定位符语法不受支持"));
        assert!(chinese.contains("无效的 xpath `//[`"));
    }

    #[test]
    fn snapshot_css_selector_errors_follow_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let english = snapshot_find(HTML, "css:[")
            .expect_err("invalid css selector should fail")
            .to_string();
        assert!(english.contains("element not found"));
        assert!(english.contains("invalid css selector `[`"));

        Settings::set_language("cn");

        let chinese = snapshot_find(HTML, "css:[")
            .expect_err("invalid css selector should localize")
            .to_string();
        assert!(chinese.contains("没有找到元素"));
        assert!(chinese.contains("无效的 css selector `[`"));
    }

    #[test]
    fn snapshot_css_filtering_rejects_xpath_locator_with_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let english = parse_optional_selector(Some("xpath://div"))
            .expect_err("xpath locator should be rejected for CSS filtering")
            .to_string();
        assert!(english.contains("unsupported locator syntax"));
        assert!(english.contains("xpath locator is not valid for CSS filtering"));

        Settings::set_language("cn");

        let chinese = parse_optional_selector(Some("xpath://div"))
            .expect_err("xpath css filtering rejection should localize")
            .to_string();
        assert!(chinese.contains("定位符语法不受支持"));
        assert!(chinese.contains("CSS 过滤不支持 xpath locator"));
    }

    #[test]
    fn snapshot_node_query_css_rejection_follows_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let root = snapshot_fragment_root(
            r#"<div id="root">alpha<span id="a">a</span><span id="b">b</span></div>"#,
        )
        .expect("fragment root should exist");

        let english = root
            .children_nodes_with(Some("span"))
            .expect_err("css locators should be rejected for node queries")
            .to_string();
        assert!(english.contains("unsupported locator syntax"));
        assert!(english.contains("css locator is not supported for node queries"));

        Settings::set_language("cn");

        let chinese = root
            .children_nodes_with(Some("span"))
            .expect_err("css node query rejection should localize")
            .to_string();
        assert!(chinese.contains("定位符语法不受支持"));
        assert!(chinese.contains("node 查询不支持 css locator"));
    }

    #[test]
    fn xpath_path_errors_follow_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let english = parse_xpath_path("broken")
            .expect_err("unsupported xpath path should fail")
            .to_string();
        assert!(english.contains("element not found"));
        assert!(english.contains("unsupported xpath path `broken`"));

        let parsed = Html::parse_document("<html><body><div></div></body></html>");
        let missing = nth_scraper_child_by_tag(parsed.tree.root(), "section", 2)
            .expect_err("missing xpath segment should fail")
            .to_string();
        assert!(missing.contains("xpath segment `section[2]` not found"));

        Settings::set_language("cn");

        let chinese = parse_xpath_path("broken")
            .expect_err("unsupported xpath path should localize")
            .to_string();
        assert!(chinese.contains("没有找到元素"));
        assert!(chinese.contains("不支持的 xpath 路径 `broken`"));

        let invalid_index = nth_scraper_child_by_tag(parsed.tree.root(), "section", 0)
            .expect_err("invalid xpath segment index should localize")
            .to_string();
        assert!(invalid_index.contains("xpath 片段 `section` 的序号无效"));

        let missing = nth_scraper_child_by_tag(parsed.tree.root(), "section", 2)
            .expect_err("missing xpath segment should localize")
            .to_string();
        assert!(missing.contains("没有找到 xpath 片段 `section[2]`"));
    }

    #[test]
    fn session_element_find_by_supports_by_mappings() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let item = root
            .find_by("class name", "item")
            .expect("first class match should exist");
        let items = root
            .find_all_by("tag name", "li")
            .expect("tag matches should exist");

        assert_eq!(item.tag().expect("item tag"), "li".to_string());
        assert_eq!(item.text().expect("item text"), Some("alpha".to_string()));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn session_element_find_accepts_by_locator_tuples() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let item = root
            .find((By::CLASS_NAME, "item"))
            .expect("first class match should exist");
        let items = root
            .find_all((By::TAG_NAME, "li"))
            .expect("tag matches should exist");

        assert_eq!(item.tag().expect("item tag"), "li".to_string());
        assert_eq!(item.text().expect("item text"), Some("alpha".to_string()));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn session_page_find_by_supports_by_mappings() {
        let path = make_temp_file("session-find-by", HTML);
        let page = Session::new(SessionOptions::default()).expect("session page");
        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load file")
                .is_success()
        );

        let submit = page.find_by("id", "submit").expect("submit button");
        let items = page
            .find_all_by("class name", "item")
            .expect("item matches");

        assert_eq!(submit.tag().expect("submit tag"), "button".to_string());
        assert_eq!(submit.text().expect("submit text"), Some("Go".to_string()));
        assert_eq!(items.len(), 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn session_page_find_accepts_by_locator_tuples() {
        let path = make_temp_file("session-find", HTML);
        let page = Session::new(SessionOptions::default()).expect("session page");
        assert!(
            page.get(path.to_str().expect("path str"))
                .expect("load file")
                .is_success()
        );

        let submit = page.find((By::ID, "submit")).expect("submit button");
        let items = page
            .find_all((By::CLASS_NAME, "item"))
            .expect("item matches");

        assert_eq!(submit.tag().expect("submit tag"), "button".to_string());
        assert_eq!(submit.text().expect("submit text"), Some("Go".to_string()));
        assert_eq!(items.len(), 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn session_page_and_element_find_locators_accept_by_locator_inputs() {
        fn assert_calls(page: &Session, element: &DocumentElement) {
            let locators = vec!["#root".to_string(), ".item".to_string()];
            let tuple_locators = [(By::ID, "root"), (By::CLASS_NAME, "item")];
            let mixed_locators = [
                LocatorInput::from("#root"),
                LocatorInput::from((By::CLASS_NAME, "item")),
            ];

            let _ = page.find_locators((By::ID, "root"), true, true);
            let _ = page.find_locators(&locators, false, false);
            let _ = page.find_locators(&tuple_locators, false, false);
            let _ = page.find_locators(&mixed_locators, false, false);
            let _ = element.find_locators((By::CLASS_NAME, "item"), true, true);
            let _ = element.find_locators(&locators, false, false);
            let _ = element.find_locators(&tuple_locators, false, false);
            let _ = element.find_locators(&mixed_locators, false, false);
        }

        let _ = assert_calls as fn(&Session, &DocumentElement);
    }

    #[test]
    fn session_settings_accept_supported_values() {
        fn assert_calls(page: &Session) {
            let settings = page.settings();
            let headers = [("Accept".to_string(), "text/html".to_string())];
            let params = [("q".to_string(), "openpage".to_string())];
            let mut param_map = std::collections::HashMap::new();
            param_map.insert("q".to_string(), "openpage".to_string());
            let cookie = SessionCookieParam {
                name: "sid".to_string(),
                value: "1".to_string(),
                url: Some("https://example.test/".to_string()),
                domain: None,
                path: Some("/".to_string()),
                secure: false,
                http_only: false,
                same_site: None,
            };

            let _ = page.set_user_agent("OpenPage/Test");
            let _ = settings.user_agent("OpenPage/Test");
            let _ = settings.user_agent(None);
            let _ = settings.headers(&headers);
            let _ = settings.header("Accept", "application/json");
            let _ = settings.timeout(10);
            let _ = settings.retry(Some(3), Some(250));
            let _ = settings.retry(3, 0.25);
            let _ = settings.retry(None, None);
            let _ = settings.retry_times(4);
            let _ = settings.retry_interval(500);
            let _ = settings.retry_interval(0.5);
            let _ = settings.download_path(std::path::Path::new("/tmp/openpage-downloads"));
            let _ = page.set_encoding("utf-8");
            let _ = settings.encoding("utf-8");
            let _ = settings.encoding(None);
            let _ = page.set_params([("q", "openpage"), ("page", "1")]);
            let _ = settings.params(&params);
            let _ = settings.params([("q", "openpage"), ("page", "1")]);
            let _ = settings.params(&param_map);
            let _ = page.set_auth(("user", "pass"));
            let _ = settings.auth(("user", "pass"));
            let _ = settings.auth(None);
            let _ = settings.hooks(SessionHooks::default());
            let _ = settings.stream(true);
            let _ = page.set_proxies("http://127.0.0.1:8080", None);
            let _ = settings.proxies("http://127.0.0.1:8080", None);
            let _ = settings.verify(false);
            let _ = page.set_cert("client.pem");
            let _ = page.set_cert(("client.pem", "client.key"));
            let _ = settings.cert(None);
            let _ = settings.cert("client.pem");
            let _ = settings.cert(("client.pem", "client.key"));
            let _ = settings.trust_env(false);
            let _ = page.set_max_redirects(5);
            let _ = settings.max_redirects(Some(5));
            let _ = settings.max_redirects(5);
            let _ = settings.cookies("sid=1; domain=example.test; path=/");
            let _ = settings.cookies(&cookie);
            let _ = settings.cookie("sid", "2", Some("https://example.test/"), None, Some("/"));
            let _ = settings.clear_cookies();
            let _ = settings.remove_cookie("sid", Some("https://example.test/"));
        }

        let _ = assert_calls as fn(&Session);
    }

    #[test]
    fn snapshot_traversal_supports_root_parent_siblings_and_children() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let items = root.find(".items").expect("items list should exist");

        assert_eq!(root.tag().expect("root tag"), "html".to_string());
        assert_eq!(
            submit
                .parent()
                .expect("submit parent")
                .tag()
                .expect("parent tag"),
            "section"
        );
        assert_eq!(
            submit
                .next()
                .expect("next sibling")
                .attr("id")
                .expect("next id"),
            Some("out".to_string())
        );
        assert_eq!(
            submit
                .prev()
                .expect("prev sibling")
                .attr("id")
                .expect("prev id"),
            Some("name".to_string())
        );

        let children = items.children().expect("direct children");
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[0].text().expect("first child text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            children[1].text().expect("second child text"),
            Some("beta".to_string())
        );
        assert_eq!(
            submit.raw_text().expect("submit raw text"),
            Some("Go".to_string())
        );
    }

    #[test]
    fn snapshot_relative_lists_cover_before_after_and_filtered_siblings() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        let prevs = submit.prevs().expect("previous siblings");
        assert_eq!(prevs.len(), 2);
        assert_eq!(prevs[0].tag().expect("first prev tag"), "h1".to_string());
        assert_eq!(
            prevs[1].attr("id").expect("second prev id"),
            Some("name".to_string())
        );

        let nexts = submit.nexts().expect("next siblings");
        assert_eq!(nexts.len(), 2);
        assert_eq!(
            nexts[0].attr("id").expect("first next id"),
            Some("out".to_string())
        );
        assert_eq!(nexts[1].tag().expect("second next tag"), "ul".to_string());

        let afters = submit.afters_with(Some(".item")).expect("following items");
        assert_eq!(afters.len(), 2);
        assert_eq!(
            afters[0].text().expect("first after text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            submit
                .after_with(Some(".item"), 2)
                .expect("second matching after")
                .text()
                .expect("second matching after text"),
            Some("beta".to_string())
        );

        let befores = second_item
            .befores_with(Some(".item"))
            .expect("preceding items");
        assert_eq!(befores.len(), 1);
        assert_eq!(
            befores[0].text().expect("preceding item text"),
            Some("alpha".to_string())
        );
        assert_eq!(
            second_item
                .before()
                .expect("nearest preceding element")
                .text()
                .expect("nearest preceding text"),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn snapshot_relative_element_not_found_errors_follow_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let root = snapshot_fragment_root(r#"<div id="root"><span id="only">one</span></div>"#)
            .expect("fragment root should exist");
        let only = root.find("#only").expect("only child should exist");

        let english_child = root
            .child_with(Some(".missing"), 1)
            .expect_err("missing child should fail")
            .to_string();
        assert!(english_child.contains("child element not found"));

        let english_child_index = root
            .child_with(None::<&str>, 0)
            .expect_err("zero child index should fail")
            .to_string();
        assert!(english_child_index.contains("child element not found: index must be >= 1"));

        let english_prev = only
            .prev_with(None::<&str>, 1)
            .expect_err("missing previous sibling should fail")
            .to_string();
        assert!(english_prev.contains("previous element not found"));

        let english_next = only
            .next_with(None::<&str>, 1)
            .expect_err("missing next sibling should fail")
            .to_string();
        assert!(english_next.contains("next element not found"));

        let english_before = root
            .before_with(Some(".missing"), 1)
            .expect_err("missing preceding element should fail")
            .to_string();
        assert!(english_before.contains("preceding element not found"));

        let english_after = root
            .after_with(Some(".missing"), 1)
            .expect_err("missing following element should fail")
            .to_string();
        assert!(english_after.contains("following element not found"));

        Settings::set_language("cn");

        let chinese_child = root
            .child_with(Some(".missing"), 1)
            .expect_err("missing child should localize")
            .to_string();
        assert!(chinese_child.contains("没有找到子元素"));

        let chinese_child_index = root
            .child_with(None::<&str>, 0)
            .expect_err("zero child index should localize")
            .to_string();
        assert!(chinese_child_index.contains("没有找到子元素: index 必须 >= 1"));

        let chinese_prev = only
            .prev_with(None::<&str>, 1)
            .expect_err("missing previous sibling should localize")
            .to_string();
        assert!(chinese_prev.contains("没有找到前一个元素"));

        let chinese_next = only
            .next_with(None::<&str>, 1)
            .expect_err("missing next sibling should localize")
            .to_string();
        assert!(chinese_next.contains("没有找到后一个元素"));

        let chinese_before = root
            .before_with(Some(".missing"), 1)
            .expect_err("missing preceding element should localize")
            .to_string();
        assert!(chinese_before.contains("没有找到前方元素"));

        let chinese_after = root
            .after_with(Some(".missing"), 1)
            .expect_err("missing following element should localize")
            .to_string();
        assert!(chinese_after.contains("没有找到后方元素"));
    }

    #[test]
    fn snapshot_relative_node_not_found_errors_follow_language_setting() {
        let _guard = scoped_test_settings();
        Settings::reset();

        let root = snapshot_fragment_root(r#"<div id="root"><span id="only"></span></div>"#)
            .expect("fragment root should exist");
        let only = root.find("#only").expect("only child should exist");

        let english_child = only
            .child_node_with(None::<&str>, 1)
            .expect_err("missing child node should fail")
            .to_string();
        assert!(english_child.contains("child node not found"));

        let english_child_index = only
            .child_node_with(None::<&str>, 0)
            .expect_err("zero child node index should fail")
            .to_string();
        assert!(english_child_index.contains("child node not found: index must be >= 1"));

        let english_prev = only
            .prev_node_with(None::<&str>, 1)
            .expect_err("missing previous node should fail")
            .to_string();
        assert!(english_prev.contains("previous node not found"));

        let english_next = only
            .next_node_with(None::<&str>, 1)
            .expect_err("missing next node should fail")
            .to_string();
        assert!(english_next.contains("next node not found"));

        let english_before = root
            .before_node_with(Some("xpath://missing"), 1)
            .expect_err("missing preceding node should fail")
            .to_string();
        assert!(english_before.contains("preceding node not found"));

        let english_after = root
            .after_node_with(Some("xpath://missing"), 1)
            .expect_err("missing following node should fail")
            .to_string();
        assert!(english_after.contains("following node not found"));

        Settings::set_language("cn");

        let chinese_child = only
            .child_node_with(None::<&str>, 1)
            .expect_err("missing child node should localize")
            .to_string();
        assert!(chinese_child.contains("没有找到子节点"));

        let chinese_child_index = only
            .child_node_with(None::<&str>, 0)
            .expect_err("zero child node index should localize")
            .to_string();
        assert!(chinese_child_index.contains("没有找到子节点: index 必须 >= 1"));

        let chinese_prev = only
            .prev_node_with(None::<&str>, 1)
            .expect_err("missing previous node should localize")
            .to_string();
        assert!(chinese_prev.contains("没有找到前一个节点"));

        let chinese_next = only
            .next_node_with(None::<&str>, 1)
            .expect_err("missing next node should localize")
            .to_string();
        assert!(chinese_next.contains("没有找到后一个节点"));

        let chinese_before = root
            .before_node_with(Some("xpath://missing"), 1)
            .expect_err("missing preceding node should localize")
            .to_string();
        assert!(chinese_before.contains("没有找到前方节点"));

        let chinese_after = root
            .after_node_with(Some("xpath://missing"), 1)
            .expect_err("missing following node should localize")
            .to_string();
        assert!(chinese_after.contains("没有找到后方节点"));
    }

    #[test]
    fn snapshot_parent_supports_level_and_xpath_filter() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        assert_eq!(
            second_item
                .parent_level(1)
                .expect("list parent")
                .tag()
                .expect("list parent tag"),
            "ul".to_string()
        );
        assert_eq!(
            second_item
                .parent_level(3)
                .expect("document parent")
                .tag()
                .expect("document parent tag"),
            "body".to_string()
        );
        assert_eq!(
            second_item
                .parent_with("xpath:section[@id='root']", 1)
                .expect("section parent")
                .attr("id")
                .expect("section id"),
            Some("root".to_string())
        );
        assert_eq!(
            second_item
                .parent_with("section#root", 1)
                .expect("css section parent")
                .attr("id")
                .expect("css section id"),
            Some("root".to_string())
        );
    }

    #[test]
    fn snapshot_parent_and_child_accept_by_locator_tuples() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let items = root.find(".items").expect("items should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        assert_eq!(
            second_item
                .parent_with((By::XPATH, "section[@id='root']"), 1)
                .expect("xpath section parent")
                .attr("id")
                .expect("section id"),
            Some("root".to_string())
        );
        assert_eq!(
            second_item
                .parent_with((By::CSS_SELECTOR, "section#root"), 1)
                .expect("css section parent")
                .attr("id")
                .expect("section id"),
            Some("root".to_string())
        );

        let direct_child = items
            .child_with(Some((By::XPATH, "li[@data-kind='b']")), 1)
            .expect("matching child");
        assert_eq!(
            direct_child.text().expect("child text"),
            Some("beta".to_string())
        );

        let tag_children = items
            .children_with(Some((By::TAG_NAME, "li")))
            .expect("matching tag children");
        assert_eq!(tag_children.len(), 2);
    }

    #[test]
    fn snapshot_prev_and_next_accept_by_locator_tuples() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");

        let prev_input = submit
            .prev_with(Some((By::TAG_NAME, "input")), 1)
            .expect("matching previous input");
        assert_eq!(
            prev_input.attr("id").expect("input id"),
            Some("name".to_string())
        );

        let next_div = submit
            .next_with(Some((By::XPATH, "div[@id='out']")), 1)
            .expect("matching next div");
        assert_eq!(
            next_div.attr("id").expect("div id"),
            Some("out".to_string())
        );

        let next_divs = submit
            .nexts_with(Some((By::TAG_NAME, "div")))
            .expect("matching next divs");
        assert_eq!(next_divs.len(), 1);

        let prev_inputs = submit
            .prevs_with(Some((By::XPATH, "input")))
            .expect("matching previous inputs");
        assert_eq!(prev_inputs.len(), 1);
    }

    #[test]
    fn snapshot_before_and_after_accept_by_locator_tuples() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        let after_item = submit
            .after_with(Some((By::CLASS_NAME, "item")), 2)
            .expect("second matching after");
        assert_eq!(
            after_item.text().expect("after item text"),
            Some("beta".to_string())
        );

        let after_items = submit
            .afters_with(Some((By::TAG_NAME, "li")))
            .expect("matching after items");
        assert_eq!(after_items.len(), 2);

        let before_item = second_item
            .before_with(Some((By::XPATH, "li")), 1)
            .expect("matching before item");
        assert_eq!(
            before_item.text().expect("before item text"),
            Some("alpha".to_string())
        );

        let before_items = second_item
            .befores_with(Some((By::CLASS_NAME, "item")))
            .expect("matching before items");
        assert_eq!(before_items.len(), 1);
    }

    #[test]
    fn snapshot_relative_lists_support_xpath_filters() {
        let root = snapshot_root(HTML).expect("document root should exist");
        let submit = root.find("#submit").expect("submit should exist");
        let items = root.find(".items").expect("items should exist");
        let second_item = root
            .find(".item[data-kind='b']")
            .expect("second item should exist");

        let direct_child = items
            .children_with(Some("xpath:li[@data-kind='b']"))
            .expect("matching child");
        assert_eq!(direct_child.len(), 1);
        assert_eq!(
            direct_child[0].text().expect("child text"),
            Some("beta".to_string())
        );

        let prev_inputs = submit
            .prevs_with(Some("xpath:input"))
            .expect("previous matching inputs");
        assert_eq!(prev_inputs.len(), 1);
        assert_eq!(
            prev_inputs[0].attr("id").expect("input id"),
            Some("name".to_string())
        );

        let before_items = second_item
            .befores_with(Some("xpath:li"))
            .expect("preceding items");
        assert_eq!(before_items.len(), 1);
        assert_eq!(
            before_items[0].text().expect("preceding item text"),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn snapshot_fragment_root_uses_first_element_in_fragment() {
        let root = snapshot_fragment_root(r#"<section id="root"><span>demo</span></section>"#)
            .expect("fragment root should exist");
        assert_eq!(
            root.tag().expect("fragment root tag"),
            "section".to_string()
        );
        assert_eq!(
            root.attr("id").expect("fragment root id"),
            Some("root".to_string())
        );
    }

    #[test]
    fn snapshot_fragment_find_supports_xpath_queries() {
        let span = snapshot_fragment_find(
            r#"<section id="root"><span class="value">demo</span></section>"#,
            "xpath:/section/span",
        )
        .expect("fragment span should exist");
        let root = snapshot_fragment_root(
            r#"<section id="root"><span class="value">demo</span></section>"#,
        )
        .expect("fragment root should exist");
        let nested = root
            .find("xpath:/span")
            .expect("root-relative xpath child should exist");

        assert_eq!(
            span.attr("class").expect("span class"),
            Some("value".to_string())
        );
        assert_eq!(
            nested.text().expect("nested text"),
            Some("demo".to_string())
        );
    }

    #[test]
    fn snapshot_fragment_paths_ignore_internal_wrapper() {
        let root = snapshot_fragment_root(r#"<section id="root"><span>demo</span></section>"#)
            .expect("fragment root should exist");
        assert_eq!(
            root.xpath().expect("fragment xpath"),
            "/section[1]".to_string()
        );
        assert_eq!(
            root.css_path().expect("fragment css path"),
            "section#root".to_string()
        );
    }

    #[test]
    fn snapshot_helpers_resolve_special_attrs_with_base_url() {
        let root = snapshot_fragment_root_with_base_url(
            r#"<div><a id="doc" href="/docs">Docs</a><img id="logo" src="img/logo.png" /></div>"#,
            Some("https://example.com/start"),
        )
        .expect("fragment root should exist");
        let link = root.find("#doc").expect("link should exist");
        let image = root.find("#logo").expect("image should exist");

        assert_eq!(
            link.attr("href").expect("href attr"),
            Some("https://example.com/docs".to_string())
        );
        assert_eq!(
            link.attr("text").expect("text attr"),
            Some("Docs".to_string())
        );
        assert_eq!(
            image.attr("src").expect("src attr"),
            Some("https://example.com/img/logo.png".to_string())
        );
        assert_eq!(
            image.link().expect("image link"),
            Some("https://example.com/img/logo.png".to_string())
        );
    }

    #[test]
    fn snapshot_helpers_cover_comments_texts_and_child_count() {
        let root = snapshot_fragment_root(
            r#"<div id="root"> alpha <!--note--><span>beta</span> <em>gamma</em></div>"#,
        )
        .expect("fragment root should exist");

        assert_eq!(root.child_count().expect("child count"), 2);
        assert_eq!(root.comments().expect("comments"), vec!["note".to_string()]);
        assert_eq!(
            root.texts(true).expect("direct texts"),
            vec!["alpha".to_string()]
        );
        assert_eq!(
            root.texts(false).expect("texts"),
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
        );
    }

    #[test]
    fn snapshot_query_xpath_supports_non_element_results() {
        let root = snapshot_fragment_root_with_base_url(
            r#"<div id="root"> alpha <!--note--><a href="/docs">Docs</a><span>beta</span></div>"#,
            Some("https://example.com/start"),
        )
        .expect("fragment root should exist");

        let text_and_element = root
            .query_xpath("./text() | *")
            .expect("text and element query");
        assert!(matches!(text_and_element[0], SessionXPathResult::Text(_)));
        assert!(matches!(
            text_and_element[1],
            SessionXPathResult::Element(_)
        ));
        assert!(matches!(
            text_and_element[2],
            SessionXPathResult::Element(_)
        ));

        let comments = root.query_xpath(".//comment()").expect("comments query");
        match &comments[0] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment result, got {other:?}"),
        }

        let attrs = root.query_xpath("./a/@href").expect("attribute query");
        match &attrs[0] {
            SessionXPathResult::Attribute { name, value } => {
                assert_eq!(name, "href");
                assert_eq!(value, "/docs");
            }
            other => panic!("expected attribute result, got {other:?}"),
        }

        let counts = root.query_xpath("count(./*)").expect("count query");
        match counts[0] {
            SessionXPathResult::Integer(value) => assert_eq!(value, 2),
            SessionXPathResult::Number(value) => assert_eq!(value, 2.0),
            ref other => panic!("expected numeric result, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_relative_node_navigation_supports_non_element_nodes() {
        let root = snapshot_fragment_root(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
        )
        .expect("fragment root should exist");
        let first = root.find("#a").expect("first span should exist");
        let second = root.find("#b").expect("second span should exist");

        let children = root.children_nodes().expect("child nodes");
        assert_eq!(children.len(), 6);
        match &children[0] {
            SessionXPathResult::Text(value) => assert_eq!(value, "alpha"),
            other => panic!("expected text child, got {other:?}"),
        }
        match &children[1] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment child, got {other:?}"),
        }
        assert!(matches!(children[2], SessionXPathResult::Element(_)));
        assert!(matches!(children[3], SessionXPathResult::Element(_)));
        assert!(matches!(children[4], SessionXPathResult::Element(_)));
        match &children[5] {
            SessionXPathResult::Text(value) => assert_eq!(value, "tail"),
            other => panic!("expected trailing text child, got {other:?}"),
        }

        match first
            .next_node_with(None::<&str>, 1)
            .expect("next sibling node should exist")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("next id"), Some("b".to_string()))
            }
            other => panic!("expected next element node, got {other:?}"),
        }
        match second
            .prev_node_with(None::<&str>, 1)
            .expect("previous sibling node should exist")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("prev id"), Some("a".to_string()))
            }
            other => panic!("expected previous element node, got {other:?}"),
        }
        match second
            .before_node_with(None::<&str>, 1)
            .expect("preceding node should exist")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("before id"), Some("a".to_string()))
            }
            other => panic!("expected preceding element node, got {other:?}"),
        }
        match second
            .after_node_with(None::<&str>, 1)
            .expect("following node should exist")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("after id"), Some("c".to_string()))
            }
            other => panic!("expected following element node, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_relative_node_navigation_supports_xpath_filters_and_rejects_css() {
        let root = snapshot_fragment_root(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
        )
        .expect("fragment root should exist");
        let second = root.find("#b").expect("second span should exist");

        let text_nodes = root
            .children_nodes_with(Some("xpath:text()"))
            .expect("text child nodes");
        assert_eq!(text_nodes.len(), 2);
        match &text_nodes[0] {
            SessionXPathResult::Text(value) => assert_eq!(value, "alpha"),
            other => panic!("expected first text child, got {other:?}"),
        }
        match &text_nodes[1] {
            SessionXPathResult::Text(value) => assert_eq!(value, "tail"),
            other => panic!("expected second text child, got {other:?}"),
        }

        let comments = second
            .prev_nodes_with(Some("xpath:comment()"))
            .expect("comment siblings");
        assert_eq!(comments.len(), 1);
        match &comments[0] {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment sibling, got {other:?}"),
        }

        match root.children_nodes_with(Some("span")) {
            Err(OpenPageError::UnsupportedLocator(message)) => {
                assert_eq!(message, "css locator is not supported for node queries")
            }
            other => panic!("expected css node query rejection, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_relative_node_navigation_accepts_by_locator_tuples() {
        let root = snapshot_fragment_root(
            r#"<div id="root">alpha<!--note--><span id="a">a</span><span id="b">b</span><strong id="c">c</strong>tail</div>"#,
        )
        .expect("fragment root should exist");
        let second = root.find("#b").expect("second span should exist");

        assert_eq!(
            root.children_nodes_with(Some((By::XPATH, "span")))
                .expect("span child nodes")
                .len(),
            2
        );

        match root
            .child_node_with(Some((By::XPATH, "comment()")), 1)
            .expect("comment child node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected comment child node, got {other:?}"),
        }

        match second
            .prev_node_with(Some((By::XPATH, "comment()")), 1)
            .expect("previous comment node")
        {
            SessionXPathResult::Comment(value) => assert_eq!(value, "note"),
            other => panic!("expected previous comment node, got {other:?}"),
        }

        match second
            .next_node_with(Some((By::XPATH, "strong[@id='c']")), 1)
            .expect("next strong node")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("next id"), Some("c".to_string()))
            }
            other => panic!("expected next strong node, got {other:?}"),
        }

        match second
            .before_node_with(Some((By::XPATH, "span[@id='a']")), 1)
            .expect("preceding span node")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("before id"), Some("a".to_string()))
            }
            other => panic!("expected preceding span node, got {other:?}"),
        }

        match second
            .after_node_with(Some((By::XPATH, "strong[@id='c']")), 1)
            .expect("following strong node")
        {
            SessionXPathResult::Element(node) => {
                assert_eq!(node.attr("id").expect("after id"), Some("c".to_string()))
            }
            other => panic!("expected following strong node, got {other:?}"),
        }

        match second.prev_nodes_with(Some((By::CSS_SELECTOR, "span"))) {
            Err(OpenPageError::UnsupportedLocator(message)) => {
                assert_eq!(message, "css locator is not supported for node queries")
            }
            other => panic!("expected css tuple node query rejection, got {other:?}"),
        }

        match root.children_nodes_with(Some((By::CSS_SELECTOR, "span"))) {
            Err(OpenPageError::UnsupportedLocator(message)) => {
                assert_eq!(message, "css locator is not supported for node queries")
            }
            other => panic!("expected css tuple child node rejection, got {other:?}"),
        }
    }

    #[test]
    fn session_find_locators_supports_any_one_and_first_match_only() {
        let root = snapshot_find(HTML, "#root").expect("root should exist");
        let locators = vec![".missing".to_string(), ".item".to_string()];
        let tuple_locators = [(By::ID, "submit"), (By::CLASS_NAME, "item")];
        let mixed_locators = [
            LocatorInput::from(".missing"),
            LocatorInput::from((By::CLASS_NAME, "item")),
        ];
        let single = root
            .find_locators((By::CLASS_NAME, "item"), true, true)
            .expect("find single by locator");

        assert_eq!(single.len(), 1);
        assert_eq!(single[0].locator, "@class=item".to_string());
        assert_eq!(single[0].elements.len(), 1);

        let any_one = root
            .find_locators(&locators, true, true)
            .expect("find any locator");
        assert_eq!(any_one.len(), 1);
        assert_eq!(any_one[0].locator, ".item".to_string());
        assert_eq!(any_one[0].elements.len(), 1);
        assert_eq!(
            any_one[0].elements[0].text().expect("matched text"),
            Some("alpha".to_string())
        );

        let all = root
            .find_locators(&locators, false, false)
            .expect("find all locators");
        assert_eq!(all.len(), 2);
        assert!(all[0].elements.is_empty());
        assert_eq!(all[1].locator, ".item".to_string());
        assert_eq!(all[1].elements.len(), 2);

        let tuples = root
            .find_locators(&tuple_locators, false, true)
            .expect("find tuple locator list");
        assert_eq!(tuples.len(), 2);
        assert_eq!(tuples[0].locator, "@id=submit".to_string());
        assert_eq!(tuples[0].elements.len(), 1);
        assert_eq!(tuples[1].locator, "@class=item".to_string());
        assert_eq!(tuples[1].elements.len(), 1);

        let mixed = root
            .find_locators(&mixed_locators, true, true)
            .expect("find mixed locator list");
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].locator, "@class=item".to_string());
        assert_eq!(mixed[0].elements.len(), 1);
    }

    #[test]
    fn session_request_returns_owned_response_with_document() {
        let (address, handle) = spawn_capture_server(
            "200 OK",
            "<html><head><title>OpenPage</title></head><body><h1 id=\"title\">Hello</h1></body></html>",
        );
        let session = Session::new(SessionOptions::default()).expect("session");
        let response = session.get(&address).expect("response");
        handle.join().expect("server thread");

        assert_eq!(response.status_code(), Some(200));
        assert!(response.is_success());
        assert_eq!(
            response
                .document()
                .find("#title")
                .expect("document element")
                .text()
                .expect("text"),
            Some("Hello".to_string())
        );
    }
}
