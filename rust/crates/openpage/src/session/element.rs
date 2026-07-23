use super::*;

impl DocumentElement {
    pub(crate) fn none_element_runtime_config_handle(
        &self,
    ) -> Option<&ElementsOneRuntimeConfigHandle> {
        self.none_element_config.as_ref()
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        find_in_scope(
            Arc::clone(&self.html),
            self.node_id,
            locator.raw(),
            self.base_url.clone(),
            self.none_element_config.as_ref(),
        )
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.find(locator.raw()) {
            Ok(element) => Ok(ElementsOneOwned::some_with_config(
                element,
                self.none_element_config.clone(),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(self.none_element_config.as_ref())? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(
                    self.none_element_config.clone(),
                ))
            }
            Err(err) => Err(err),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        find_all_in_scope(
            Arc::clone(&self.html),
            self.node_id,
            locator.raw(),
            self.base_url.clone(),
            self.none_element_config.as_ref(),
        )
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find_by(&self, by: &str, value: &str) -> OpenPageResult<DocumentElement> {
        self.find((by, value))
    }

    pub fn find_all_by(&self, by: &str, value: &str) -> OpenPageResult<Vec<DocumentElement>> {
        self.find_all((by, value))
    }

    pub fn query_xpath(&self, expression: &str) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.with_element(|element| {
            xpath_query_from_scope_element(
                &self.html,
                self.base_url.as_ref(),
                element,
                expression,
                self.none_element_config.as_ref(),
            )
        })
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<DocumentElement>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        collect_locator_matches(&locators, any_one, first_match_only, |locator| {
            self.find_all(locator)
        })
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        self.with_element(|element| Ok(element.value().name().to_string()))
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| {
            let text = element.text().collect::<String>().trim().to_string();
            Ok(Some(text).filter(|value| !value.is_empty()))
        })
    }

    pub fn html(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| Ok(Some(element.html())))
    }

    pub fn inner_html(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| Ok(Some(element.inner_html())))
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        self.with_element(|element| {
            let text = element.text().collect::<String>();
            Ok(Some(text).filter(|value| !value.is_empty()))
        })
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.with_element(|element| {
            Ok(element
                .value()
                .attrs
                .iter()
                .map(|(name, value)| (name.local.to_string(), value.to_string()))
                .collect())
        })
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.with_element(|element| {
            let raw = element.attr(name).map(ToString::to_string);
            match name {
                "href" => {
                    Ok(raw.and_then(|value| resolve_href_attr(&value, self.base_url.as_deref())))
                }
                "src" => {
                    Ok(raw.and_then(|value| resolve_src_attr(&value, self.base_url.as_deref())))
                }
                "text" => self.text(),
                "innerText" => self.raw_text(),
                "html" | "outerHTML" => self.html(),
                "innerHTML" => self.inner_html(),
                _ => Ok(element
                    .attr(&name.to_ascii_lowercase())
                    .map(ToString::to_string)),
            }
        })
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        let href = self.attr("href")?;
        if href.as_deref().is_some_and(|value| !value.is_empty()) {
            return Ok(href);
        }
        self.attr("src")
    }

    pub fn child_count(&self) -> OpenPageResult<usize> {
        self.with_element(|element| Ok(element.child_elements().count()))
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        self.with_element(|element| Ok(css_path_for_element(element)))
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        self.with_element(|element| Ok(xpath_for_element(element)))
    }

    pub fn comments(&self) -> OpenPageResult<Vec<String>> {
        self.with_element(|element| {
            Ok(element
                .descendants()
                .filter_map(|node| {
                    node.value()
                        .as_comment()
                        .and_then(|comment| normalize_text_item(comment))
                })
                .collect())
        })
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Vec<String>> {
        self.with_element(|element| {
            let mut values = Vec::new();
            for child in element.children() {
                match child.value() {
                    Node::Text(text) => {
                        if let Some(value) = normalize_text_item(text) {
                            values.push(value);
                        }
                    }
                    Node::Element(_) if !text_node_only => {
                        if let Some(child_element) = ElementRef::wrap(child) {
                            let text = child_element.text().collect::<String>();
                            if let Some(value) = normalize_text_item(text.as_str()) {
                                values.push(value);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(values)
        })
    }

    pub fn parent(&self) -> OpenPageResult<DocumentElement> {
        self.parent_level(1)
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<DocumentElement> {
        if level == 0 {
            return Err(OpenPageError::ElementNotFound(
                parent_element_level_must_start_message(),
            ));
        }
        self.with_element(|element| {
            let mut current = Some(element);
            for _ in 0..level {
                current = current.and_then(nearest_parent_element);
            }
            current
                .map(|parent| {
                    session_element_from_ref(
                        &self.html,
                        self.base_url.as_ref(),
                        parent,
                        self.none_element_config.as_ref(),
                    )
                })
                .ok_or_else(|| OpenPageError::ElementNotFound(parent_element_not_found_message()))
        })
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                parent_element_index_must_start_message(),
            ));
        }
        let locator = Locator::from_input(locator)?;
        match locator.kind() {
            LocatorKind::Css => self.with_element(|element| {
                let selector = parse_selector_query(locator.query())?;
                nth_from_start(
                    element
                        .ancestors()
                        .skip(1)
                        .filter_map(ElementRef::wrap)
                        .filter(|candidate| selector.matches(candidate))
                        .map(|candidate| {
                            session_element_from_ref(
                                &self.html,
                                self.base_url.as_ref(),
                                candidate,
                                self.none_element_config.as_ref(),
                            )
                        })
                        .collect(),
                    index,
                    &parent_element_not_found_message(),
                )
            }),
            LocatorKind::XPath => self.with_element(|element| {
                nth_from_start(
                    xpath_find_all_from_scope_element(
                        &self.html,
                        self.base_url.as_ref(),
                        element,
                        &format!(
                            "{}[{index}]",
                            relative_axis_xpath_query("ancestor", locator.query())
                        ),
                        self.none_element_config.as_ref(),
                    )?,
                    1,
                    &parent_element_not_found_message(),
                )
            }),
        }
    }

    pub fn child(&self) -> OpenPageResult<DocumentElement> {
        self.child_with(None::<&str>, 1)
    }

    pub fn child_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.child_node_with(None::<&str>, 1)
    }

    pub fn child_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.children_with(locator)?,
            index,
            &child_element_not_found_message(),
        )
    }

    pub fn child_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.children_nodes_with(locator)?,
            index,
            &child_node_not_found_message(),
        )
    }

    pub fn children(&self) -> OpenPageResult<Vec<DocumentElement>> {
        self.children_with(None::<&str>)
    }

    pub fn children_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.children_nodes_with(None::<&str>)
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &direct_child_xpath_query(locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => collect_matching_elements(
                &self.html,
                self.base_url.as_ref(),
                element.child_elements(),
                locator.as_ref().map(|locator| locator.raw()),
                self.none_element_config.as_ref(),
            ),
        })
    }

    pub fn children_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        self.with_element(|element| {
            relative_node_xpath_query_with_locator(
                &self.html,
                self.base_url.as_ref(),
                element,
                locator.as_ref(),
                "./node()",
                direct_child_xpath_query,
                self.none_element_config.as_ref(),
            )
        })
    }

    pub fn prev(&self) -> OpenPageResult<DocumentElement> {
        self.prev_with(None::<&str>, 1)
    }

    pub fn prev_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.prev_node_with(None::<&str>, 1)
    }

    pub fn prev_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_end(
            self.prevs_with(locator)?,
            index,
            &previous_element_not_found_message(),
        )
    }

    pub fn prev_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_end(
            self.prev_nodes_with(locator)?,
            index,
            &previous_node_not_found_message(),
        )
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<DocumentElement>> {
        self.prevs_with(None::<&str>)
    }

    pub fn prev_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.prev_nodes_with(None::<&str>)
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &relative_axis_xpath_query("preceding-sibling", locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => {
                let mut items = collect_matching_elements(
                    &self.html,
                    self.base_url.as_ref(),
                    element.prev_siblings().filter_map(ElementRef::wrap),
                    locator.as_ref().map(|locator| locator.raw()),
                    self.none_element_config.as_ref(),
                )?;
                items.reverse();
                Ok(items)
            }
        })
    }

    pub fn prev_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        self.with_element(|element| {
            relative_node_xpath_query_with_locator(
                &self.html,
                self.base_url.as_ref(),
                element,
                locator.as_ref(),
                "./preceding-sibling::node()",
                |query| relative_axis_xpath_query("preceding-sibling", query),
                self.none_element_config.as_ref(),
            )
        })
    }

    pub fn next(&self) -> OpenPageResult<DocumentElement> {
        self.next_with(None::<&str>, 1)
    }

    pub fn next_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.next_node_with(None::<&str>, 1)
    }

    pub fn next_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.nexts_with(locator)?,
            index,
            &next_element_not_found_message(),
        )
    }

    pub fn next_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.next_nodes_with(locator)?,
            index,
            &next_node_not_found_message(),
        )
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<DocumentElement>> {
        self.nexts_with(None::<&str>)
    }

    pub fn next_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.next_nodes_with(None::<&str>)
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &relative_axis_xpath_query("following-sibling", locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => collect_matching_elements(
                &self.html,
                self.base_url.as_ref(),
                element.next_siblings().filter_map(ElementRef::wrap),
                locator.as_ref().map(|locator| locator.raw()),
                self.none_element_config.as_ref(),
            ),
        })
    }

    pub fn next_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        self.with_element(|element| {
            relative_node_xpath_query_with_locator(
                &self.html,
                self.base_url.as_ref(),
                element,
                locator.as_ref(),
                "./following-sibling::node()",
                |query| relative_axis_xpath_query("following-sibling", query),
                self.none_element_config.as_ref(),
            )
        })
    }

    pub fn before(&self) -> OpenPageResult<DocumentElement> {
        self.before_with(None::<&str>, 1)
    }

    pub fn before_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.before_node_with(None::<&str>, 1)
    }

    pub fn before_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_end(
            self.befores_with(locator)?,
            index,
            &preceding_element_not_found_message(),
        )
    }

    pub fn before_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_end(
            self.before_nodes_with(locator)?,
            index,
            &preceding_node_not_found_message(),
        )
    }

    pub fn befores(&self) -> OpenPageResult<Vec<DocumentElement>> {
        self.befores_with(None::<&str>)
    }

    pub fn before_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.before_nodes_with(None::<&str>)
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &relative_axis_xpath_query("preceding", locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => document_relatives(
                &self.html,
                self.base_url.as_ref(),
                element,
                RelativeDirection::Before,
                locator.as_ref().map(|locator| locator.raw()),
                self.none_element_config.as_ref(),
            ),
        })
    }

    pub fn before_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        match locator.as_ref() {
            Some(locator) => self.with_element(|element| {
                filter_relative_node_results(
                    xpath_query_from_scope_element(
                        &self.html,
                        self.base_url.as_ref(),
                        element,
                        &relative_axis_xpath_query("preceding", locator.query()),
                        self.none_element_config.as_ref(),
                    )?,
                    xpath_query_requests_attributes(locator.query()),
                )
            }),
            None => self.with_element(|element| {
                collect_document_relative_nodes(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    RelativeDirection::Before,
                    self.none_element_config.as_ref(),
                )
            }),
        }
    }

    pub fn after(&self) -> OpenPageResult<DocumentElement> {
        self.after_with(None::<&str>, 1)
    }

    pub fn after_node(&self) -> OpenPageResult<SessionXPathResult> {
        self.after_node_with(None::<&str>, 1)
    }

    pub fn after_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.afters_with(locator)?,
            index,
            &following_element_not_found_message(),
        )
    }

    pub fn after_node_with<'a, L>(
        &self,
        locator: Option<L>,
        index: usize,
    ) -> OpenPageResult<SessionXPathResult>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_from_start(
            self.after_nodes_with(locator)?,
            index,
            &following_node_not_found_message(),
        )
    }

    pub fn afters(&self) -> OpenPageResult<Vec<DocumentElement>> {
        self.afters_with(None::<&str>)
    }

    pub fn after_nodes(&self) -> OpenPageResult<Vec<SessionXPathResult>> {
        self.after_nodes_with(None::<&str>)
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        self.with_element(|element| match locator.as_ref() {
            Some(locator) if locator.kind() == LocatorKind::XPath => {
                xpath_find_all_from_scope_element(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    &relative_axis_xpath_query("following", locator.query()),
                    self.none_element_config.as_ref(),
                )
            }
            _ => document_relatives(
                &self.html,
                self.base_url.as_ref(),
                element,
                RelativeDirection::After,
                locator.as_ref().map(|locator| locator.raw()),
                self.none_element_config.as_ref(),
            ),
        })
    }

    pub fn after_nodes_with<'a, L>(
        &self,
        locator: Option<L>,
    ) -> OpenPageResult<Vec<SessionXPathResult>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_xpath_locator_input(locator)?;
        match locator.as_ref() {
            Some(locator) => self.with_element(|element| {
                filter_relative_node_results(
                    xpath_query_from_scope_element(
                        &self.html,
                        self.base_url.as_ref(),
                        element,
                        &relative_axis_xpath_query("following", locator.query()),
                        self.none_element_config.as_ref(),
                    )?,
                    xpath_query_requests_attributes(locator.query()),
                )
            }),
            None => self.with_element(|element| {
                collect_document_relative_nodes(
                    &self.html,
                    self.base_url.as_ref(),
                    element,
                    RelativeDirection::After,
                    self.none_element_config.as_ref(),
                )
            }),
        }
    }

    fn with_element<T, F>(&self, f: F) -> OpenPageResult<T>
    where
        F: FnOnce(ElementRef<'_>) -> OpenPageResult<T>,
    {
        let document = Html::parse_document(self.html.as_ref());
        let element = element_from_document(&document, self.node_id)?;
        f(element)
    }
}
