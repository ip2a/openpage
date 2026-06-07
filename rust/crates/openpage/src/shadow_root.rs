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

use crate::element::Element;
use crate::element_list::{
    ElementsOneOwned, ElementsOneRuntimeConfigHandle, elements_one_should_raise_when_missing,
};
use crate::error::{OpenPageError, OpenPageResult};
use crate::locator::{Locator, LocatorInput, LocatorKind, parse_optional_locator_input};
use crate::page::execute_page_command_async;
use crate::session::{
    SessionElement, SessionXPathResult, snapshot_fragment_find_all_with_base_url,
    snapshot_fragment_find_with_base_url, snapshot_fragment_query_xpath_with_base_url,
    snapshot_fragment_root_with_base_url,
};
use crate::settings::{
    javascript_execution_timed_out_message, shadow_root_object_id_unavailable_message,
};

const MARKER_ATTRIBUTE: &str = "data-openpage-marker";

#[derive(Debug, Clone)]
pub struct ShadowRoot {
    runtime: Arc<Runtime>,
    page: OxPage,
    backend_node_id: BackendNodeId,
    remote_object_id: RemoteObjectId,
    host_node_id: NodeId,
    javascript_timeout_ms: u64,
    none_element_config: ElementsOneRuntimeConfigHandle,
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
    ) -> Self {
        Self {
            runtime,
            page,
            backend_node_id,
            remote_object_id,
            host_node_id,
            javascript_timeout_ms,
            none_element_config,
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
            "shadow root host element not found",
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

    pub fn snapshot_find(&self, locator: &str) -> OpenPageResult<SessionElement> {
        let html = self.inner_html()?;
        let base_url = value_as_optional_string(
            self.run_js("return this.baseURI || (this.host && this.host.baseURI) || document.baseURI || null;")?,
            "baseURI",
        )?;
        snapshot_fragment_find_with_base_url(&html, locator, base_url.as_deref())
    }

    pub fn snapshot_find_all(&self, locator: &str) -> OpenPageResult<Vec<SessionElement>> {
        let html = self.inner_html()?;
        let base_url = value_as_optional_string(
            self.run_js("return this.baseURI || (this.host && this.host.baseURI) || document.baseURI || null;")?,
            "baseURI",
        )?;
        snapshot_fragment_find_all_with_base_url(&html, locator, base_url.as_deref())
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
            "shadow root child element not found",
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
                "shadow root parent element not found: level must be >= 1".to_string(),
            ));
        }
        let host = self.host()?;
        nth_element_from_start(
            host.find_all(&format!("xpath:./ancestor-or-self::*[{level}]"))?,
            1,
            "shadow root parent element not found",
        )
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                "shadow root parent element not found: index must be >= 1".to_string(),
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
                "shadow root parent element not found",
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
            "shadow root next element not found",
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
            "shadow root preceding element not found",
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
            "shadow root following element not found",
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
            .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
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
            .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
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
                OpenPageError::ElementNotFound(format!(
                    "shadow root xpath css path `{css_path}` could not be resolved: {err}"
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
                let element = self
                    .page
                    .find_xpath(query)
                    .await
                    .map_err(|err| OpenPageError::ElementNotFound(err.to_string()))?;
                elements.push(Element::new(
                    Arc::clone(&self.runtime),
                    self.page.clone(),
                    None,
                    None,
                    element,
                    self.javascript_timeout_ms,
                    Arc::clone(&self.none_element_config),
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
        Value::Null => Err(OpenPageError::ElementNotFound(format!(
            "{name} is unavailable"
        ))),
        other => Err(OpenPageError::JavaScript(format!(
            "{name} did not return a bool: {other}"
        ))),
    }
}

fn value_as_optional_string(value: Value, name: &str) -> OpenPageResult<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value)),
        Value::Bool(value) => Ok(Some(value.to_string())),
        Value::Number(value) => Ok(Some(value.to_string())),
        other => Err(OpenPageError::JavaScript(format!(
            "{name} did not return a string-compatible value: {other}"
        ))),
    }
}

fn value_as_string(value: Value, name: &str) -> OpenPageResult<String> {
    match value {
        Value::String(value) => Ok(value),
        Value::Null => Err(OpenPageError::ElementNotFound(format!(
            "{name} is unavailable"
        ))),
        other => Err(OpenPageError::JavaScript(format!(
            "{name} did not return a string: {other}"
        ))),
    }
}

fn nth_element_from_start(
    elements: Vec<Element>,
    index: usize,
    error_message: &str,
) -> OpenPageResult<Element> {
    if index == 0 {
        return Err(OpenPageError::ElementNotFound(format!(
            "{error_message}: index must be >= 1"
        )));
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
        return Err(OpenPageError::ElementNotFound(format!(
            "{error_message}: index must be >= 1"
        )));
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
        build_js_invocation, direct_child_selector, normalize_axis_xpath,
        resolve_javascript_timeout_ms,
    };
    use serde_json::json;

    #[test]
    fn resolve_javascript_timeout_ms_prefers_explicit_value() {
        assert_eq!(resolve_javascript_timeout_ms(Some(250), 30_000), 250);
        assert_eq!(resolve_javascript_timeout_ms(Some(0), 30_000), 1);
        assert_eq!(resolve_javascript_timeout_ms(None, 30_000), 30_000);
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
