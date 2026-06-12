use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chromiumoxide::cdp::browser_protocol::dom::{
    BackendNodeId, NodeId, QuerySelectorAllParams, QuerySelectorParams, RemoveAttributeParams,
    RequestNodeParams, ResolveNodeParams, SetAttributeValueParams,
};
use chromiumoxide::cdp::js_protocol::runtime::{CallFunctionOnParams, RemoteObjectId};
use chromiumoxide::page::Page as OxPage;
use serde_json::Value;
use tokio::runtime::Runtime;
use tokio::time::timeout as tokio_timeout;

use crate::element::Element;
use crate::element_list::{
    ElementsOneOwned, ElementsOneRuntimeConfigHandle, elements_one_should_raise_when_missing,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::locator::{
    Locator, LocatorBatchInput, LocatorInput, LocatorKind, LocatorMatch, collect_locator_matches,
    parse_locator_batch_input, parse_optional_locator_input,
};
use crate::page::{
    FrameCacheHandle, FrameNoneElementConfigCacheHandle, execute_page_command_async,
};
use crate::session::{
    SessionElement, SessionXPathResult, snapshot_fragment_find_all_with_base_url,
    snapshot_fragment_find_with_base_url, snapshot_fragment_query_xpath_with_base_url,
    snapshot_fragment_root_with_base_url,
};
use crate::settings::{
    cdp_timeout_duration, element_index_must_start_message, javascript_execution_timed_out_message,
    shadow_root_child_element_not_found_message, shadow_root_following_element_not_found_message,
    shadow_root_host_element_not_found_message, shadow_root_next_element_not_found_message,
    shadow_root_object_id_unavailable_message, shadow_root_operation_failed_message,
    shadow_root_parent_element_index_must_start_message,
    shadow_root_parent_element_level_must_start_message,
    shadow_root_parent_element_not_found_message, shadow_root_preceding_element_not_found_message,
    shadow_root_xpath_css_path_unresolved_message, timeout_duration_millis, timeout_error,
    value_bool_required_message, value_string_compatible_required_message,
    value_string_required_message, value_unavailable_message,
};

const MARKER_ATTRIBUTE: &str = "data-openpage-marker";

async fn run_shadow_root_lookup_future_with_cdp_timeout<Fut, T, E>(
    future: Fut,
    operation: &str,
) -> OpenPageResult<T>
where
    Fut: Future<Output = Result<T, E>>,
    E: ToString,
{
    let timeout = cdp_timeout_duration();
    let timeout_ms = timeout_duration_millis(timeout);
    tokio_timeout(timeout, future)
        .await
        .map_err(|_| timeout_error(operation, timeout_ms))?
        .map_err(|err| {
            OpenPageError::ElementNotFound(shadow_root_operation_failed_message(
                operation,
                &err.to_string(),
            ))
        })
}

fn shadow_root_selector_error(err: OpenPageError) -> OpenPageError {
    match err {
        OpenPageError::Timeout(message) => OpenPageError::Timeout(message),
        err => OpenPageError::ElementNotFound(shadow_root_operation_failed_message(
            "selector lookup",
            &err.to_string(),
        )),
    }
}

#[derive(Debug, Clone)]
pub struct ShadowRoot {
    runtime: Arc<Runtime>,
    page: OxPage,
    backend_node_id: BackendNodeId,
    remote_object_id: RemoteObjectId,
    host_node_id: NodeId,
    javascript_timeout_ms: u64,
    none_element_config: ElementsOneRuntimeConfigHandle,
    frame_cache: FrameCacheHandle,
    frame_none_element_configs: FrameNoneElementConfigCacheHandle,
}

impl ShadowRoot {
    pub(crate) fn new(
        runtime: Arc<Runtime>,
        page: OxPage,
        backend_node_id: BackendNodeId,
        remote_object_id: RemoteObjectId,
        host_node_id: NodeId,
        javascript_timeout_ms: u64,
        none_element_config: ElementsOneRuntimeConfigHandle,
        frame_cache: FrameCacheHandle,
        frame_none_element_configs: FrameNoneElementConfigCacheHandle,
    ) -> Self {
        Self {
            runtime,
            page,
            backend_node_id,
            remote_object_id,
            host_node_id,
            javascript_timeout_ms,
            none_element_config,
            frame_cache,
            frame_none_element_configs,
        }
    }

    pub fn tag(&self) -> String {
        "shadow-root".to_string()
    }

    pub fn html(&self) -> OpenPageResult<String> {
        Ok(format!("<shadow_root>{}</shadow_root>", self.inner_html()?))
    }

    pub fn inner_html(&self) -> OpenPageResult<String> {
        value_as_string(
            self.run_js("return this.innerHTML;")?,
            "shadow root innerHTML",
        )
    }

    pub fn host(&self) -> OpenPageResult<Element> {
        nth_element_from_start(
            self.resolve_node_ids(&[self.host_node_id])?,
            1,
            &shadow_root_host_element_not_found_message(),
        )
    }

    pub fn backend_node_id(&self) -> BackendNodeId {
        self.backend_node_id
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        self.run_js_with_args(script, &[], false)
    }

    pub fn run_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.run_js_with_options(script, args, as_expr, None)
    }

    pub fn run_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        let js = build_js_invocation(script, args, as_expr)?;
        let timeout_ms = Some(resolve_javascript_timeout_ms(
            timeout_ms,
            self.javascript_timeout_ms,
        ));
        self.runtime.block_on(async {
            let response = self.call_js_fn_with_timeout(js, true, timeout_ms).await?;
            Ok(response.result.value.unwrap_or(Value::Null))
        })
    }

    pub fn run_async_js(&self, script: &str) -> OpenPageResult<()> {
        self.run_async_js_with_args(script, &[], false)
    }

    pub fn run_async_js_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<()> {
        self.run_async_js_with_options(script, args, as_expr, None)
    }

    pub fn run_async_js_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<()> {
        let js = build_js_invocation(script, args, as_expr)?;
        let timeout_ms = Some(resolve_javascript_timeout_ms(
            timeout_ms,
            self.javascript_timeout_ms,
        ));
        self.runtime.block_on(async {
            self.call_js_fn_with_timeout(js, false, timeout_ms)
                .await
                .map(|_| ())
        })
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        let html = self.inner_html()?;
        let base_url = value_as_optional_string(
            self.run_js("return this.baseURI || (this.host && this.host.baseURI) || document.baseURI || null;")?,
            "baseURI",
        )?;
        snapshot_fragment_root_with_base_url(&html, base_url.as_deref())
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let html = self.inner_html()?;
        let base_url = value_as_optional_string(
            self.run_js("return this.baseURI || (this.host && this.host.baseURI) || document.baseURI || null;")?,
            "baseURI",
        )?;
        snapshot_fragment_find_with_base_url(&html, locator.raw(), base_url.as_deref())
    }

    pub fn snapshot_find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let html = self.inner_html()?;
        let base_url = value_as_optional_string(
            self.run_js("return this.baseURI || (this.host && this.host.baseURI) || document.baseURI || null;")?,
            "baseURI",
        )?;
        snapshot_fragment_find_all_with_base_url(&html, locator.raw(), base_url.as_deref())
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find(locator.raw())
    }

    pub fn s_eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<SessionElement> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<SessionElement>> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        let html = self.inner_html()?;
        let base_url = value_as_optional_string(
            self.run_js("return this.baseURI || (this.host && this.host.baseURI) || document.baseURI || null;")?,
            "baseURI",
        )?;
        snapshot_fragment_query_xpath_with_base_url(&html, expression, base_url.as_deref())
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        value_as_bool(self.run_js("return !this.disabled;")?, "enabled")
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        match self.run_js("return !!this.isConnected;") {
            Ok(value) => value_as_bool(value, "alive"),
            Err(_) => Ok(false),
        }
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match locator.kind() {
            LocatorKind::Css => {
                let node_id = self.query_selector(locator.query())?;
                self.resolve_node(node_id, "shadow root element not found")
            }
            LocatorKind::XPath => nth_element_from_start(
                self.find_all_by_xpath(locator.query())?,
                1,
                "shadow root element not found",
            ),
        }
    }

    pub fn ele<'a, L>(&self, locator: L) -> OpenPageResult<ElementsOneOwned<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match self.find(locator.raw()) {
            Ok(element) => Ok(ElementsOneOwned::some_with_config(
                element,
                Some(Arc::clone(&self.none_element_config)),
            )),
            Err(err @ OpenPageError::ElementNotFound(_)) => {
                if elements_one_should_raise_when_missing(Some(&self.none_element_config))? {
                    return Err(err);
                }
                Ok(ElementsOneOwned::none_with_config(Some(Arc::clone(
                    &self.none_element_config,
                ))))
            }
            Err(err) => Err(err),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        match locator.kind() {
            LocatorKind::Css => self.query_selector_all(locator.query()),
            LocatorKind::XPath => self.find_all_by_xpath(locator.query()),
        }
    }

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find_locators<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        first_match_only: bool,
    ) -> OpenPageResult<Vec<LocatorMatch<Element>>>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        collect_locator_matches(&locators, any_one, first_match_only, |locator| {
            self.find_all(locator)
        })
    }

    pub fn child(&self) -> OpenPageResult<Element> {
        self.child_with(None::<&str>, 1)
    }

    pub fn child_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_start(
            self.children_with(locator)?,
            index,
            &shadow_root_child_element_not_found_message(),
        )
    }

    pub fn children(&self) -> OpenPageResult<Vec<Element>> {
        self.children_with(None::<&str>)
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        match parse_optional_locator_input(locator)? {
            None => self.query_selector_all(":scope > *"),
            Some(locator) => match locator.kind() {
                LocatorKind::Css => {
                    let selector = direct_child_selector(Some(locator.query()))?;
                    self.query_selector_all(&selector)
                }
                LocatorKind::XPath => {
                    self.find_all_by_xpath(&normalize_child_xpath(locator.query()))
                }
            },
        }
    }

    pub fn parent(&self) -> OpenPageResult<Element> {
        self.parent_level(1)
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<Element> {
        if level == 0 {
            return Err(OpenPageError::ElementNotFound(
                shadow_root_parent_element_level_must_start_message(),
            ));
        }
        let host = self.host()?;
        nth_element_from_start(
            host.find_all(&format!("xpath:./ancestor-or-self::*[{level}]"))?,
            1,
            &shadow_root_parent_element_not_found_message(),
        )
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                shadow_root_parent_element_index_must_start_message(),
            ));
        }
        let locator = Locator::from_input(locator)?;
        let host = self.host()?;
        match locator.kind() {
            LocatorKind::Css => {
                if host.matches_locator(&locator)? {
                    if index == 1 {
                        return Ok(host);
                    }
                    return host.parent_with(locator.raw(), index - 1);
                }
                host.parent_with(locator.raw(), index)
            }
            LocatorKind::XPath => nth_element_from_start(
                host.find_all(&format!(
                    "xpath:{}[{index}]",
                    normalize_axis_xpath("ancestor-or-self", locator.query())
                ))?,
                1,
                &shadow_root_parent_element_not_found_message(),
            ),
        }
    }

    pub fn next(&self) -> OpenPageResult<Element> {
        self.next_with(None::<&str>, 1)
    }

    pub fn next_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_start(
            self.nexts_with(locator)?,
            index,
            &shadow_root_next_element_not_found_message(),
        )
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<Element>> {
        self.nexts_with(None::<&str>)
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.host()?.children_with(locator)
    }

    pub fn before(&self) -> OpenPageResult<Element> {
        self.before_with(None::<&str>, 1)
    }

    pub fn before_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_end(
            self.befores_with(locator)?,
            index,
            &shadow_root_preceding_element_not_found_message(),
        )
    }

    pub fn befores(&self) -> OpenPageResult<Vec<Element>> {
        self.befores_with(None::<&str>)
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.host()?.befores_with(locator)
    }

    pub fn after(&self) -> OpenPageResult<Element> {
        self.after_with(None::<&str>, 1)
    }

    pub fn after_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        nth_element_from_start(
            self.afters_with(locator)?,
            index,
            &shadow_root_following_element_not_found_message(),
        )
    }

    pub fn afters(&self) -> OpenPageResult<Vec<Element>> {
        self.afters_with(None::<&str>)
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = parse_optional_locator_input(locator)?;
        let host = self.host()?;
        let mut elements = host.children_with(locator.as_ref().map(|locator| locator.raw()))?;
        elements.extend(host.afters_with(locator.as_ref().map(|locator| locator.raw()))?);
        Ok(elements)
    }

    fn query_selector(&self, selector: &str) -> OpenPageResult<NodeId> {
        let root_node_id = self.current_node_id()?;
        let node_id = self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.page,
                QuerySelectorParams::new(root_node_id, selector.to_string()),
                "ShadowRoot::query_selector()",
            )
            .await
            .map_err(shadow_root_selector_error)?;
            Ok::<NodeId, OpenPageError>(response.node_id)
        })?;
        if *node_id.inner() == 0 {
            Err(OpenPageError::ElementNotFound(selector.to_string()))
        } else {
            Ok(node_id)
        }
    }

    fn query_selector_all(&self, selector: &str) -> OpenPageResult<Vec<Element>> {
        let root_node_id = self.current_node_id()?;
        let node_ids = self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.page,
                QuerySelectorAllParams::new(root_node_id, selector.to_string()),
                "ShadowRoot::query_selector_all()",
            )
            .await
            .map_err(shadow_root_selector_error)?;
            Ok::<Vec<NodeId>, OpenPageError>(response.node_ids)
        })?;
        self.resolve_node_ids(&node_ids)
    }

    fn current_node_id(&self) -> OpenPageResult<NodeId> {
        self.runtime.block_on(async {
            let resolved = execute_page_command_async(
                &self.page,
                ResolveNodeParams::builder()
                    .backend_node_id(self.backend_node_id)
                    .build(),
                "ShadowRoot::current_node_id()",
            )
            .await?;
            let object_id = resolved.object.object_id.ok_or_else(|| {
                OpenPageError::PageOperation(shadow_root_object_id_unavailable_message())
            })?;
            let requested = execute_page_command_async(
                &self.page,
                RequestNodeParams::new(object_id),
                "ShadowRoot::current_node_id()",
            )
            .await?;
            Ok::<NodeId, OpenPageError>(requested.node_id)
        })
    }

    fn resolve_node(&self, node_id: NodeId, error_message: &str) -> OpenPageResult<Element> {
        nth_element_from_start(self.resolve_node_ids(&[node_id])?, 1, error_message)
    }

    fn find_all_by_xpath(&self, xpath: &str) -> OpenPageResult<Vec<Element>> {
        let xpath = normalize_relative_xpath(xpath);
        let html = self.inner_html()?;
        let base_url = value_as_optional_string(
            self.run_js(
                "return this.baseURI || (this.host && this.host.baseURI) || document.baseURI || null;",
            )?,
            "baseURI",
        )?;
        let results =
            snapshot_fragment_query_xpath_with_base_url(&html, &xpath, base_url.as_deref())?;
        let mut node_ids = Vec::new();
        for result in results {
            let SessionXPathResult::Element(element) = result else {
                continue;
            };
            let css_path = element.css_path()?;
            if css_path.is_empty() {
                continue;
            }
            node_ids.push(self.query_selector(&css_path).map_err(|err| {
                OpenPageError::ElementNotFound(shadow_root_xpath_css_path_unresolved_message(
                    &css_path,
                    &err.to_string(),
                ))
            })?);
        }
        self.resolve_node_ids(&node_ids)
    }

    async fn call_js_fn_with_timeout(
        &self,
        js: String,
        await_promise: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<chromiumoxide::cdp::js_protocol::runtime::CallFunctionOnReturns> {
        let params = CallFunctionOnParams::builder()
            .function_declaration(js)
            .object_id(self.remote_object_id.clone())
            .await_promise(await_promise)
            .return_by_value(true)
            .build()
            .map_err(OpenPageError::JavaScript)?;
        let future = self.page.execute(params);
        let response = match timeout_ms {
            Some(timeout_ms) => {
                tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), future)
                    .await
                    .map_err(|_| OpenPageError::Timeout(javascript_execution_timed_out_message()))?
            }
            None => future.await,
        }
        .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
        let response = response.result;
        if let Some(details) = response.exception_details {
            return Err(OpenPageError::JavaScript(format!("{details:?}")));
        }
        Ok(response)
    }

    fn resolve_node_ids(&self, node_ids: &[NodeId]) -> OpenPageResult<Vec<Element>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        let batch = next_marker_batch();
        let markers: Vec<_> = node_ids
            .iter()
            .enumerate()
            .map(|(index, node_id)| (*node_id, format!("{batch}-{index}")))
            .collect();

        self.runtime.block_on(async {
            for (node_id, marker) in &markers {
                execute_page_command_async(
                    &self.page,
                    SetAttributeValueParams::new(*node_id, MARKER_ATTRIBUTE, marker.clone()),
                    "ShadowRoot::resolve_node_ids()",
                )
                .await?;
            }
            Ok::<(), OpenPageError>(())
        })?;

        let elements = self.runtime.block_on(async {
            let mut elements = Vec::with_capacity(markers.len());
            for (_, marker) in &markers {
                let query = marker_search_query(marker);
                let element = run_shadow_root_lookup_future_with_cdp_timeout(
                    self.page.find_xpath(query),
                    "resolve shadow root element",
                )
                .await?;
                elements.push(Element::new(
                    Arc::clone(&self.runtime),
                    self.page.clone(),
                    None,
                    None,
                    element,
                    self.javascript_timeout_ms,
                    Arc::clone(&self.none_element_config),
                    Arc::clone(&self.frame_cache),
                    Arc::clone(&self.frame_none_element_configs),
                ));
            }
            Ok::<Vec<Element>, OpenPageError>(elements)
        });

        let cleanup = self.runtime.block_on(async {
            for (node_id, _) in &markers {
                let _ = execute_page_command_async(
                    &self.page,
                    RemoveAttributeParams::new(*node_id, MARKER_ATTRIBUTE),
                    "ShadowRoot::resolve_node_ids()",
                )
                .await;
            }
            Ok::<(), OpenPageError>(())
        });

        match (elements, cleanup) {
            (Ok(elements), Ok(())) => Ok(elements),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }
}

fn direct_child_selector(locator: Option<&str>) -> OpenPageResult<String> {
    let Some(locator) = locator.map(str::trim).filter(|locator| !locator.is_empty()) else {
        return Ok(":scope > *".to_string());
    };
    Ok(format!(":scope > {locator}"))
}

fn normalize_relative_xpath(xpath: &str) -> String {
    let xpath = xpath.trim();
    if xpath.starts_with('/') {
        format!(".{xpath}")
    } else {
        xpath.to_string()
    }
}

fn normalize_child_xpath(xpath: &str) -> String {
    let xpath = xpath.trim().trim_start_matches(['.', '/']);
    format!("./{xpath}")
}

fn normalize_axis_xpath(axis: &str, xpath: &str) -> String {
    let xpath = xpath.trim().trim_start_matches(['.', '/']);
    format!("./{axis}::{xpath}")
}

fn marker_search_query(marker: &str) -> String {
    format!(r#"[{MARKER_ATTRIBUTE}="{marker}"]"#)
}

fn build_js_invocation(script: &str, args: &[Value], as_expr: bool) -> OpenPageResult<String> {
    let args_json =
        serde_json::to_string(args).map_err(|err| OpenPageError::Serialization(err.to_string()))?;
    if as_expr {
        Ok(format!(
            "function() {{ const __args = {args_json}; return ((...args) => ({script}))(...__args); }}"
        ))
    } else {
        Ok(format!(
            "function() {{ const __args = {args_json}; return (function(...args) {{ {script} }}).apply(this, __args); }}"
        ))
    }
}

fn resolve_javascript_timeout_ms(requested: Option<u64>, default_timeout_ms: u64) -> u64 {
    requested.unwrap_or(default_timeout_ms).max(1)
}

fn value_as_bool(value: Value, name: &str) -> OpenPageResult<bool> {
    match value {
        Value::Bool(value) => Ok(value),
        Value::Null => Err(OpenPageError::ElementNotFound(value_unavailable_message(
            name,
        ))),
        other => Err(OpenPageError::JavaScript(value_bool_required_message(
            name,
            &other.to_string(),
        ))),
    }
}

fn value_as_optional_string(value: Value, name: &str) -> OpenPageResult<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value)),
        Value::Bool(value) => Ok(Some(value.to_string())),
        Value::Number(value) => Ok(Some(value.to_string())),
        other => Err(OpenPageError::JavaScript(
            value_string_compatible_required_message(name, &other.to_string()),
        )),
    }
}

fn value_as_string(value: Value, name: &str) -> OpenPageResult<String> {
    match value {
        Value::String(value) => Ok(value),
        Value::Null => Err(OpenPageError::ElementNotFound(value_unavailable_message(
            name,
        ))),
        other => Err(OpenPageError::JavaScript(value_string_required_message(
            name,
            &other.to_string(),
        ))),
    }
}

fn nth_element_from_start(
    elements: Vec<Element>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<Element> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            element_index_must_start_message(error_message),
        ));
    }
    elements
        .into_iter()
        .nth(index - 1)
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

fn nth_element_from_end(
    elements: Vec<Element>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<Element> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(
            element_index_must_start_message(error_message),
        ));
    }
    let len = elements.len();
    elements
        .into_iter()
        .nth(len.saturating_sub(index))
        .ok_or_else(|| OpenPageError::ElementNotFound(error_message.to_string()))
}

fn next_marker_batch() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_MARKER_BATCH: AtomicU64 = AtomicU64::new(1);
    format!(
        "openpage-shadow-{}",
        NEXT_MARKER_BATCH.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ShadowRoot, build_js_invocation, direct_child_selector, normalize_axis_xpath,
        nth_element_from_end, nth_element_from_start, resolve_javascript_timeout_ms,
        run_shadow_root_lookup_future_with_cdp_timeout, shadow_root_selector_error,
    };
    use crate::{
        By, Element, LocatorInput, LocatorMatch, OpenPageError, OpenPageResult, SessionElement,
        SessionXPathResult, Settings,
    };
    use serde_json::json;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    #[test]
    fn resolve_javascript_timeout_ms_prefers_explicit_value() {
        assert_eq!(resolve_javascript_timeout_ms(Some(250), 30_000), 250);
        assert_eq!(resolve_javascript_timeout_ms(Some(0), 30_000), 1);
        assert_eq!(resolve_javascript_timeout_ms(None, 30_000), 30_000);
    }

    #[test]
    fn shadow_root_lookup_operations_respect_global_timeout_setting() {
        let _guard = crate::settings::scoped_test_settings();
        Settings::reset();
        Settings::set_cdp_timeout(0.01);

        let runtime = Runtime::new().expect("runtime");

        let lookup_error = runtime
            .block_on(run_shadow_root_lookup_future_with_cdp_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    Ok::<(), &'static str>(())
                },
                "resolve shadow root element",
            ))
            .expect_err("shadow root lookup should time out");
        assert!(
            matches!(lookup_error, OpenPageError::Timeout(ref message) if message.contains("resolve shadow root element")),
            "unexpected shadow root lookup timeout error: {lookup_error}"
        );

        Settings::reset();

        let lookup_error = runtime
            .block_on(run_shadow_root_lookup_future_with_cdp_timeout(
                async { Err::<(), &'static str>("missing") },
                "resolve shadow root element",
            ))
            .expect_err("shadow root lookup failure should remain ElementNotFound");
        assert!(
            matches!(lookup_error, OpenPageError::ElementNotFound(ref message) if message == "ShadowRoot operation resolve shadow root element failed: missing"),
            "unexpected shadow root lookup error: {lookup_error}"
        );

        Settings::set_language("cn");

        let lookup_error = runtime
            .block_on(run_shadow_root_lookup_future_with_cdp_timeout(
                async { Err::<(), &'static str>("missing") },
                "resolve shadow root element",
            ))
            .expect_err("shadow root lookup failure should localize");
        assert!(
            matches!(lookup_error, OpenPageError::ElementNotFound(ref message) if message == "ShadowRoot 操作 resolve shadow root element 失败: missing"),
            "unexpected localized shadow root lookup error: {lookup_error}"
        );
    }

    #[test]
    fn shadow_root_selector_errors_preserve_timeouts() {
        let timeout = shadow_root_selector_error(OpenPageError::Timeout("slow".to_string()));
        assert!(
            matches!(timeout, OpenPageError::Timeout(ref message) if message == "slow"),
            "unexpected timeout conversion: {timeout}"
        );

        let missing = shadow_root_selector_error(OpenPageError::PageOperation("boom".to_string()));
        assert!(
            matches!(missing, OpenPageError::ElementNotFound(ref message) if message == "ShadowRoot operation selector lookup failed: page operation failed: boom"),
            "unexpected selector conversion: {missing}"
        );

        Settings::set_language("cn");

        let missing = shadow_root_selector_error(OpenPageError::PageOperation("boom".to_string()));
        assert!(
            matches!(missing, OpenPageError::ElementNotFound(ref message) if message == "ShadowRoot 操作 selector lookup 失败: 页面操作失败: boom"),
            "unexpected localized selector conversion: {missing}"
        );
    }

    #[test]
    fn shadow_root_relative_index_errors_follow_language_setting() {
        let _guard = crate::settings::scoped_test_settings();
        Settings::reset();

        let english = nth_element_from_start(
            Vec::<Element>::new(),
            0,
            &crate::settings::shadow_root_child_element_not_found_message(),
        )
        .expect_err("zero child index should fail")
        .to_string();
        assert!(english.contains("shadow root child element not found: index must be >= 1"));

        Settings::set_language("cn");

        let chinese = nth_element_from_end(
            Vec::<Element>::new(),
            0,
            &crate::settings::shadow_root_preceding_element_not_found_message(),
        )
        .expect_err("zero preceding index should localize")
        .to_string();
        assert!(chinese.contains("没有找到 ShadowRoot 前方元素: index 必须 >= 1"));
    }

    #[test]
    fn shadow_root_xpath_css_path_errors_follow_language_setting() {
        let _guard = crate::settings::scoped_test_settings();
        Settings::reset();

        assert_eq!(
            crate::settings::shadow_root_xpath_css_path_unresolved_message(
                "body > span",
                "missing"
            ),
            "shadow root xpath css path `body > span` could not be resolved: missing"
        );

        Settings::set_language("cn");

        assert_eq!(
            crate::settings::shadow_root_xpath_css_path_unresolved_message(
                "body > span",
                "missing"
            ),
            "ShadowRoot xpath css path `body > span` 无法解析: missing"
        );
    }

    #[test]
    fn shadow_root_static_query_aliases_are_typechecked() {
        fn assert_methods(root: &ShadowRoot) -> OpenPageResult<()> {
            let _: SessionElement = root.s_ele("css:.item")?;
            let _: Vec<SessionElement> = root.s_eles("css:.item")?;
            let _: Vec<SessionXPathResult> = root.snapshot_query_xpath(".//*")?;
            Ok(())
        }

        let _ = assert_methods as fn(&ShadowRoot) -> OpenPageResult<()>;
    }

    #[test]
    fn shadow_root_find_locators_accepts_batch_inputs() {
        fn assert_methods(root: &ShadowRoot) -> OpenPageResult<()> {
            let tuple_locators = [("id", "root"), (By::CLASS_NAME, "item")];
            let mixed_locators = [
                LocatorInput::from("#root"),
                LocatorInput::from((By::CLASS_NAME, "item")),
            ];

            let _: Vec<LocatorMatch<Element>> = root.find_locators("#root", true, true)?;
            let _: Vec<LocatorMatch<Element>> = root.find_locators((By::ID, "root"), true, true)?;
            let _: Vec<LocatorMatch<Element>> =
                root.find_locators(&tuple_locators, false, false)?;
            let _: Vec<LocatorMatch<Element>> =
                root.find_locators(&mixed_locators, false, false)?;
            Ok(())
        }

        let _ = assert_methods as fn(&ShadowRoot) -> OpenPageResult<()>;
    }

    #[test]
    fn direct_child_selector_defaults_to_scope_children() {
        assert_eq!(
            direct_child_selector(None).expect("selector should build"),
            ":scope > *"
        );
    }

    #[test]
    fn direct_child_selector_prefixes_scope() {
        assert_eq!(
            direct_child_selector(Some(".item")).expect("selector should build"),
            ":scope > .item"
        );
    }

    #[test]
    fn normalize_axis_xpath_prefixes_requested_axis() {
        assert_eq!(
            normalize_axis_xpath("ancestor-or-self", "./div[@x='1']"),
            "./ancestor-or-self::div[@x='1']"
        );
        assert_eq!(normalize_axis_xpath("preceding", "//a"), "./preceding::a");
    }

    #[test]
    fn build_js_invocation_wraps_statement_body_with_args() {
        let js = build_js_invocation("return args[0] + args[1];", &[json!(1), json!(2)], false)
            .expect("invocation should build");
        assert!(js.contains("const __args = [1,2];"));
        assert!(js.contains("function(...args)"));
    }

    #[test]
    fn build_js_invocation_wraps_expression_body_with_args() {
        let js = build_js_invocation("args[0] + args[1]", &[json!(1), json!(2)], true)
            .expect("invocation should build");
        assert!(js.contains("const __args = [1,2];"));
        assert!(js.contains("=> (args[0] + args[1])"));
    }
}
