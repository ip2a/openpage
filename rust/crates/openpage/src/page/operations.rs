use super::*;

impl Page {
    pub(crate) fn apply_load_mode(&self, load_mode: LoadMode) -> OpenPageResult<()> {
        self.load_mode
            .lock()
            .map(|mut mode| *mode = load_mode)
            .map_err(|_| {
                OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                    "page load mode",
                    "页面加载模式",
                ))
            })
    }

    pub(super) fn browser_backed_ref(&self, method_name: &str) -> OpenPageResult<&Browser> {
        self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_method_message(method_name))
        })
    }

    fn cloned_none_element_config(
        &self,
        handle: &ElementsOneConfigHandle,
    ) -> OpenPageResult<ElementsOneConfigHandle> {
        handle
            .lock()
            .map(|config| Arc::new(std::sync::Mutex::new(config.clone())))
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element config",
                    "未找到元素配置",
                ))
            })
    }

    fn frame_none_element_config_from(
        &self,
        frame_id: &str,
        source: &ElementsOneConfigHandle,
    ) -> OpenPageResult<ElementsOneConfigHandle> {
        let fresh_config = self.cloned_none_element_config(source)?;
        if !singleton_tab_obj_enabled() {
            return Ok(fresh_config);
        }

        self.prune_frame_none_element_configs()?;
        let mut configs = self.frame_none_element_configs.lock().map_err(|_| {
            OpenPageError::PageOperation(component_state_lock_poisoned_message(
                "frame none element config cache",
                "frame 未找到元素配置缓存",
            ))
        })?;
        if let Some(config) = configs.get(frame_id) {
            return Ok(Arc::clone(config));
        }
        configs.insert(frame_id.to_string(), Arc::clone(&fresh_config));
        Ok(fresh_config)
    }

    fn cached_frame(&self, frame_id: &str) -> OpenPageResult<Option<Frame>> {
        if !singleton_tab_obj_enabled() {
            return Ok(None);
        }

        self.prune_frame_caches()?;
        self.frame_cache
            .lock()
            .map(|cache| cache.get(frame_id).cloned())
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "frame object cache",
                    "frame 对象缓存",
                ))
            })
    }

    fn cache_frame(&self, frame: &Frame) -> OpenPageResult<()> {
        if !singleton_tab_obj_enabled() {
            return Ok(());
        }

        self.frame_cache
            .lock()
            .map(|mut cache| {
                cache.insert(frame.id().to_string(), frame.clone());
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "frame object cache",
                    "frame 对象缓存",
                ))
            })
    }

    fn prune_frame_none_element_configs(&self) -> OpenPageResult<()> {
        let live_frame_ids: HashSet<String> =
            self.download_scope_frame_ids()?.into_iter().collect();
        self.prune_frame_caches_for_live_ids(&live_frame_ids)
    }

    fn prune_frame_caches(&self) -> OpenPageResult<()> {
        let live_frame_ids: HashSet<String> =
            self.download_scope_frame_ids()?.into_iter().collect();
        self.prune_frame_caches_for_live_ids(&live_frame_ids)
    }

    fn prune_frame_caches_for_live_ids(
        &self,
        live_frame_ids: &HashSet<String>,
    ) -> OpenPageResult<()> {
        let mut frames = self.frame_cache.lock().map_err(|_| {
            OpenPageError::PageOperation(component_state_lock_poisoned_message(
                "frame object cache",
                "frame 对象缓存",
            ))
        })?;
        frames.retain(|frame_id, _| live_frame_ids.contains(frame_id));
        drop(frames);

        let mut configs = self.frame_none_element_configs.lock().map_err(|_| {
            OpenPageError::PageOperation(component_state_lock_poisoned_message(
                "frame none element config cache",
                "frame 未找到元素配置缓存",
            ))
        })?;
        configs.retain(|frame_id, _| live_frame_ids.contains(frame_id));
        Ok(())
    }

    pub fn set_none_element_value(&self, value: Option<&str>, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.return_value = value.map(str::to_string);
                config.return_value_enabled = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element config",
                    "未找到元素配置",
                ))
            })
    }

    pub fn set_raise_when_ele_not_found(&self, on_off: bool) -> OpenPageResult<()> {
        self.none_element_config
            .lock()
            .map(|mut config| {
                config.raise_when_not_found = on_off;
            })
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "none element config",
                    "未找到元素配置",
                ))
            })
    }

    fn javascript_timeout_ms(&self) -> OpenPageResult<u64> {
        match &self.browser {
            Some(browser) => Ok(browser.timeouts()?.script),
            None => Ok(DEFAULT_SCRIPT_TIMEOUT_MS),
        }
    }

    pub(super) fn implicit_wait_timeout_ms(&self) -> OpenPageResult<u64> {
        match &self.browser {
            Some(browser) => Ok(resolve_implicit_wait_timeout_ms(Some(
                browser.timeouts()?.implicit_wait,
            ))),
            None => Ok(resolve_implicit_wait_timeout_ms(None)),
        }
    }

    pub fn url(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            Ok(
                run_page_future_with_cdp_timeout(self.inner.url(), "read url")
                    .await?
                    .unwrap_or_default(),
            )
        })
    }

    pub fn title(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            Ok(
                run_page_future_with_cdp_timeout(self.inner.get_title(), "read title")
                    .await?
                    .unwrap_or_default(),
            )
        })
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        match &self.browser {
            Some(browser) => browser.page_download_path(&self.target_id()),
            None => Ok(None),
        }
    }

    pub fn download_file_exists_mode(&self) -> OpenPageResult<String> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "download_file_exists_mode()",
            ))
        })?;
        browser.page_download_file_exists_mode(&self.target_id())
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        let scope_url = self.url()?;
        self.download_with_cookie_scope(url, Some(scope_url.as_str()))
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        let scope_url = self.url()?;
        self.download_to_with_cookie_scope(url, path, Some(scope_url.as_str()))
    }

    pub fn html(&self) -> OpenPageResult<String> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.content(), "read html").await
        })
    }

    pub fn evaluate(&self, expression: &str) -> OpenPageResult<Value> {
        let timeout_ms = self.javascript_timeout_ms()?;
        self.runtime.block_on(run_with_timeout(
            async {
                let result = self
                    .inner
                    .evaluate(expression)
                    .await
                    .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
                result
                    .into_value::<Value>()
                    .map_err(|err| OpenPageError::JavaScript(err.to_string()))
            },
            timeout_ms,
            javascript_execution_timed_out_message(),
        ))
    }

    fn evaluate_with_options(
        &self,
        expression: &str,
        timeout_ms: Option<u64>,
        await_promise: bool,
    ) -> OpenPageResult<Value> {
        let timeout_ms = resolve_javascript_timeout_ms(timeout_ms, self.javascript_timeout_ms()?);
        let params = EvaluateParams::builder()
            .expression(expression)
            .await_promise(await_promise)
            .build()
            .map_err(OpenPageError::PageOperation)?;
        self.evaluate_params_with_timeout(params, timeout_ms)
    }

    fn evaluate_params_with_timeout(
        &self,
        params: EvaluateParams,
        timeout_ms: u64,
    ) -> OpenPageResult<Value> {
        self.runtime.block_on(run_with_timeout(
            async {
                let result = self
                    .inner
                    .evaluate(params)
                    .await
                    .map_err(|err| OpenPageError::JavaScript(err.to_string()))?;
                let default_value = result.value().cloned();
                match result.into_value::<Value>() {
                    Ok(value) => Ok(value),
                    Err(_) => Ok(default_value.unwrap_or(Value::Null)),
                }
            },
            timeout_ms,
            javascript_execution_timed_out_message(),
        ))
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

    pub fn eles<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.find_all(locator)
    }

    pub fn find<'a, L>(&self, locator: L) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let javascript_timeout_ms = self.javascript_timeout_ms()?;
        self.runtime.block_on(async {
            let element = match locator.kind() {
                LocatorKind::Css => {
                    run_page_lookup_future_with_cdp_timeout(
                        self.inner.find_element(locator.query().to_string()),
                        "find element",
                    )
                    .await?
                }
                LocatorKind::XPath => {
                    run_page_lookup_future_with_cdp_timeout(
                        self.inner.find_xpath(locator.query().to_string()),
                        "find element by xpath",
                    )
                    .await?
                }
            };
            Ok(Element::new(
                Arc::clone(&self.runtime),
                self.inner.clone(),
                self.browser.clone(),
                Some(self.uploader.clone()),
                element,
                javascript_timeout_ms,
                Arc::clone(&self.none_element_config),
                Arc::clone(&self.frame_cache),
                Arc::clone(&self.frame_none_element_configs),
            ))
        })
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let javascript_timeout_ms = self.javascript_timeout_ms()?;
        self.runtime.block_on(async {
            let elements = match locator.kind() {
                LocatorKind::Css => {
                    run_page_lookup_future_with_cdp_timeout(
                        self.inner.find_elements(locator.query().to_string()),
                        "find elements",
                    )
                    .await?
                }
                LocatorKind::XPath => {
                    run_page_lookup_future_with_cdp_timeout(
                        self.inner.find_xpaths(locator.query().to_string()),
                        "find elements by xpath",
                    )
                    .await?
                }
            };
            Ok(elements
                .into_iter()
                .map(|element| {
                    Element::new(
                        Arc::clone(&self.runtime),
                        self.inner.clone(),
                        self.browser.clone(),
                        Some(self.uploader.clone()),
                        element,
                        javascript_timeout_ms,
                        Arc::clone(&self.none_element_config),
                        Arc::clone(&self.frame_cache),
                        Arc::clone(&self.frame_none_element_configs),
                    )
                })
                .collect())
        })
    }

    pub fn wait_for<'a, L>(&self, locator: L, timeout_ms: u64) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.wait_for_raw(locator.raw(), timeout_ms)
    }

    fn wait_for_raw(&self, locator: &str, timeout_ms: u64) -> OpenPageResult<Element> {
        let start = Instant::now();
        loop {
            match self.find(locator) {
                Ok(element) => return Ok(element),
                Err(err) => {
                    if start.elapsed() >= Duration::from_millis(timeout_ms) {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            locator,
                            &err.to_string(),
                        )));
                    }
                    sleep(Duration::from_millis(100));
                }
            }
        }
    }

    pub fn wait_for_elements_loaded<'a, L>(
        &self,
        locators: L,
        any_one: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorBatchInput<'a>>,
    {
        let locators = parse_locator_batch_input(locators)?;
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            let mut matched = 0usize;
            for locator in &locators {
                if !self.find_all(locator)?.is_empty() {
                    matched += 1;
                }
            }
            if (!any_one && matched == locators.len()) || (any_one && matched > 0) {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("Page::wait_for_elements_loaded()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for_ele_displayed<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_displayed(remaining)
        })
    }

    pub fn wait_for_ele_hidden<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_hidden(remaining)
        })
    }

    pub fn wait_for_ele_enabled<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_enabled(remaining)
        })
    }

    pub fn wait_for_ele_deleted<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_deleted(remaining)
        })
    }

    pub fn wait_for_ele_clickable<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.wait_for_ele_state(target, timeout_ms, |ele, remaining| {
            ele.wait_until_clickable(remaining)
        })
    }

    fn wait_for_ele_state<'a, L, F>(
        &self,
        target: L,
        timeout_ms: u64,
        wait_fn: F,
    ) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
        F: FnOnce(&Element, u64) -> OpenPageResult<bool>,
    {
        match target.into() {
            PageElementTarget::Locator(locator) => {
                let locator = Locator::from_input(locator)?;
                self.wait_for_ele_state_raw(locator.raw(), timeout_ms, wait_fn)
            }
            target => wait_fn(
                resolve_page_element_target(self, target)?.element(),
                timeout_ms,
            ),
        }
    }

    fn wait_for_ele_state_raw<F>(
        &self,
        locator: &str,
        timeout_ms: u64,
        wait_fn: F,
    ) -> OpenPageResult<bool>
    where
        F: FnOnce(&Element, u64) -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        let element = loop {
            match self.find(locator) {
                Ok(ele) => break ele,
                Err(_) => {
                    sleep(Duration::from_millis(50));
                    if Instant::now() >= deadline {
                        return wait_timeout_result("Page::wait_for_ele_state()", timeout_ms);
                    }
                }
            }
        };
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        wait_fn(&element, remaining.max(1))
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        match &target {
            PageFrameTarget::Frame(frame) => {
                resolve_page_frame_target(self, target.clone())?;
                return Ok((*frame).clone());
            }
            PageFrameTarget::OwnedFrame(frame) => {
                resolve_page_frame_target(self, target.clone())?;
                return Ok(frame.clone());
            }
            _ => {}
        }
        self.frame_from_element(self.get_frame_ele(target)?)
    }

    pub fn get_frame_with_timeout<'a, L>(&self, target: L, timeout_ms: u64) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame(target.clone()) {
                Ok(frame) => return Ok(frame),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            "frame",
                            &err.to_string(),
                        )));
                    }
                }
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_by_index<I>(&self, index: I) -> OpenPageResult<Frame>
    where
        I: FrameIndexInput,
    {
        self.get_frame(index.into_frame_index())
    }

    pub fn get_frame_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<Frame>
    where
        I: FrameIndexInput,
    {
        self.get_frame_with_timeout(index.into_frame_index(), timeout_ms)
    }

    pub fn get_frame_ele<'a, L>(&self, target: L) -> OpenPageResult<Element>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        resolve_page_frame_target(self, target.into())
    }

    pub fn get_frame_ele_with_timeout<'a, L>(
        &self,
        target: L,
        timeout_ms: u64,
    ) -> OpenPageResult<Element>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame_ele(target.clone()) {
                Ok(element) => return Ok(element),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            "frame element",
                            &err.to_string(),
                        )));
                    }
                }
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_ele_by_index<I>(&self, index: I) -> OpenPageResult<Element>
    where
        I: FrameIndexInput,
    {
        self.get_frame_ele(index.into_frame_index())
    }

    pub fn get_frame_ele_by_index_with_timeout<I>(
        &self,
        index: I,
        timeout_ms: u64,
    ) -> OpenPageResult<Element>
    where
        I: FrameIndexInput,
    {
        self.get_frame_ele_with_timeout(index.into_frame_index(), timeout_ms)
    }

    pub fn get_frames<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.get_frame_eles(locator)?
            .into_iter()
            .map(|element| self.frame_from_element(element))
            .collect()
    }

    pub fn get_frames_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = optional_frame_locator_input(locator)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frames(Some(locator.as_str())) {
                Ok(frames) if !frames.is_empty() => return Ok(frames),
                Ok(_) => {}
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            &locator,
                            &err.to_string(),
                        )));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                    &locator,
                    "no matching frames",
                )));
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_eles<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(optional_frame_locator_input(locator)?.as_str())?;
        let batch = next_page_marker();
        let script = frame_find_all_script(&locator, &batch)?;
        let markers = value_as_string_vec(self.run_js(&script)?, "page get_frame_eles() result")?;
        let mut elements = Vec::with_capacity(markers.len());
        for marker in markers {
            let element = self.find(&marker_xpath(&marker))?;
            let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
            elements.push(element);
        }
        Ok(elements)
    }

    pub fn get_frame_eles_with_timeout<'a, L>(
        &self,
        locator: Option<L>,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = optional_frame_locator_input(locator)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            match self.get_frame_eles(Some(locator.as_str())) {
                Ok(elements) if !elements.is_empty() => return Ok(elements),
                Ok(_) => {}
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                            &locator,
                            &err.to_string(),
                        )));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(OpenPageError::Timeout(wait_for_locator_timed_out_message(
                    &locator,
                    "no matching frame elements",
                )));
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn get_frame_context<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        self.get_frame(target)
    }

    pub fn get_frame_context_by_index<I>(&self, index: I) -> OpenPageResult<Frame>
    where
        I: FrameIndexInput,
    {
        self.get_frame_by_index(index)
    }

    pub fn get_frame_contexts<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Frame>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.get_frames(locator)
    }

    pub fn set_blocked_urls<'a, I>(&self, patterns: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        let patterns = actions_input_values(patterns.into());
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            NetworkEnableParams::default(),
            "Page::set_blocked_urls()",
        )?;
        let params = SetBlockedUrLsParams::builder()
            .url_patterns(
                patterns
                    .iter()
                    .cloned()
                    .map(|pattern| BlockPattern::new(pattern, true)),
            )
            .build();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::set_blocked_urls()",
        )?;
        Ok(())
    }

    pub fn run_js(&self, script: &str) -> OpenPageResult<Value> {
        let script = load_javascript_source(script)?;
        match script {
            Cow::Borrowed(script) => self.evaluate(script),
            Cow::Owned(script) => self.run_js_with_options(&script, &[], false, None),
        }
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
        let script = load_javascript_source(script)?;
        let expression = build_page_js_expression(script.as_ref(), args, as_expr)?;
        self.evaluate_with_options(&expression, timeout_ms, true)
    }

    pub fn run_js_loaded(&self, script: &str) -> OpenPageResult<Value> {
        self.run_js_loaded_with_args(script, &[], false)
    }

    pub fn run_js_loaded_with_args(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
    ) -> OpenPageResult<Value> {
        self.run_js_loaded_with_options(script, args, as_expr, None)
    }

    pub fn run_js_loaded_with_options(
        &self,
        script: &str,
        args: &[Value],
        as_expr: bool,
        timeout_ms: Option<u64>,
    ) -> OpenPageResult<Value> {
        let _ = self.wait_for_doc_loaded(self.navigation_page_load_timeout_ms()?);
        self.run_js_with_options(script, args, as_expr, timeout_ms)
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
        let script = load_javascript_source(script)?;
        let expression = build_page_js_expression(script.as_ref(), args, as_expr)?;
        self.evaluate_with_options(&expression, timeout_ms, false)
            .map(|_| ())
    }

    pub fn set(&self) -> PageSetter<'_> {
        PageSetter { page: self }
    }

    pub fn execute_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            command,
            "Page::execute_cdp()",
        )
    }

    pub fn run_cdp<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        self.execute_cdp(command)
    }

    pub fn execute_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        self.wait_for_doc_loaded(self.navigation_page_load_timeout_ms()?)?;
        self.execute_cdp(command)
    }

    pub fn run_cdp_loaded<T>(&self, command: T) -> OpenPageResult<T::Response>
    where
        T: Command,
    {
        self.execute_cdp_loaded(command)
    }

    pub fn window_hide(&self) -> OpenPageResult<()> {
        let Some(browser_pid) = self.browser_pid else {
            return Err(OpenPageError::UnsupportedOperation(
                launched_browser_only_message("window hide()"),
            ));
        };
        set_app_visibility(browser_pid, false)
    }

    pub fn window_show(&self) -> OpenPageResult<()> {
        let Some(browser_pid) = self.browser_pid else {
            return Err(OpenPageError::UnsupportedOperation(
                launched_browser_only_message("window show()"),
            ));
        };
        set_app_visibility(browser_pid, true)
    }

    pub fn set_tab_download_path(&self, path: &str) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "set_tab_download_path()",
            ))
        })?;
        browser.set_page_download_path(&self.target_id(), path)
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.set_tab_download_path(path)
    }

    pub fn set_tab_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "set_tab_download_file_exists_mode()",
            ))
        })?;
        browser.set_page_download_file_exists_mode(&self.target_id(), mode)
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.set_tab_download_file_exists_mode(mode)
    }

    pub fn set_tab_when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.set_tab_download_file_exists_mode(DownloadFileExistsMode::parse(mode)?)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.set_tab_when_download_file_exists(mode)
    }

    pub fn set_tab_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "set_tab_download_filename()",
            ))
        })?;
        browser.set_page_download_filename(&self.target_id(), rename, suffix, suffix_specified)
    }

    pub fn set_tab_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_download_filename(rename, suffix, suffix_specified)
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        self.browser
            .as_ref()
            .ok_or_else(|| {
                OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                    "retry_times()",
                ))
            })?
            .retry_times()
    }

    pub fn retry_interval(&self) -> OpenPageResult<f64> {
        self.browser
            .as_ref()
            .ok_or_else(|| {
                OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                    "retry_interval()",
                ))
            })?
            .retry_interval_millis()
            .map(|millis| millis as f64 / 1000.0)
    }

    pub fn set_retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message("set_retry()"))
        })?;
        browser.set_retry(
            retry_times,
            retry_interval_secs
                .map(timeout_seconds_to_millis)
                .transpose()?,
        )
    }

    pub fn timeouts(&self) -> OpenPageResult<HashMap<&'static str, f64>> {
        let timeouts = self
            .browser
            .as_ref()
            .ok_or_else(|| {
                OpenPageError::UnsupportedOperation(browser_backed_page_only_message("timeouts()"))
            })?
            .timeouts()?;
        Ok(HashMap::from([
            ("base", timeouts.implicit_wait as f64 / 1000.0),
            ("page_load", timeouts.page_load as f64 / 1000.0),
            ("script", timeouts.script as f64 / 1000.0),
        ]))
    }

    pub fn set_timeouts(
        &self,
        base_secs: Option<f64>,
        page_load_secs: Option<f64>,
        script_secs: Option<f64>,
    ) -> OpenPageResult<()> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message("set_timeouts()"))
        })?;
        let mut timeouts = browser.timeouts()?;
        if let Some(base_secs) = base_secs {
            timeouts.implicit_wait = timeout_seconds_to_millis(base_secs)?;
        }
        if let Some(page_load_secs) = page_load_secs {
            timeouts.page_load = timeout_seconds_to_millis(page_load_secs)?;
        }
        if let Some(script_secs) = script_secs {
            timeouts.script = timeout_seconds_to_millis(script_secs)?;
        }
        browser.set_timeouts(timeouts)
    }

    pub fn load_mode(&self) -> OpenPageResult<String> {
        Ok(self.load_mode_value()?.as_str().to_string())
    }

    pub fn set_load_mode(&self, mode: LoadMode) -> OpenPageResult<()> {
        *self.load_mode.lock().map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "page load mode",
                "页面加载模式",
            ))
        })? = mode;
        Ok(())
    }

    pub fn window_state(&self) -> OpenPageResult<String> {
        let info = self.window_info()?;
        Ok(info
            .bounds
            .window_state
            .map(|state| state.as_ref().to_string())
            .unwrap_or_else(|| "normal".to_string()))
    }

    pub fn window_size(&self) -> OpenPageResult<(i64, i64)> {
        let info = self.window_info()?;
        Ok((
            info.bounds.width.unwrap_or_default(),
            info.bounds.height.unwrap_or_default(),
        ))
    }

    pub fn window_location(&self) -> OpenPageResult<(i64, i64)> {
        let info = self.window_info()?;
        Ok((
            info.bounds.left.unwrap_or_default(),
            info.bounds.top.unwrap_or_default(),
        ))
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        value_as_f64_pair(
            self.run_js(
                "[document.scrollingElement ? document.scrollingElement.scrollLeft : 0, \
                  document.scrollingElement ? document.scrollingElement.scrollTop : 0]",
            )?,
            "page scroll position",
        )
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js(
                "(() => { \
                    const doc = document.documentElement; \
                    const width = Number(window.innerWidth ?? (doc ? doc.clientWidth : 0)); \
                    const height = Number(window.innerHeight ?? (doc ? doc.clientHeight : 0)); \
                    return [width, height]; \
                })()",
            )?,
            "page viewport size",
        )
    }

    pub fn window_max(&self) -> OpenPageResult<()> {
        let current = self.window_state()?;
        if current == "fullscreen" || current == "minimized" {
            self.window_normal()?;
        }
        self.set_window_bounds(
            Bounds::builder()
                .window_state(WindowState::Maximized)
                .build(),
        )
    }

    pub fn window_min(&self) -> OpenPageResult<()> {
        if self.window_state()? == "fullscreen" {
            self.window_normal()?;
        }
        self.set_window_bounds(
            Bounds::builder()
                .window_state(WindowState::Minimized)
                .build(),
        )
    }

    pub fn window_full(&self) -> OpenPageResult<()> {
        if self.window_state()? == "minimized" {
            self.window_normal()?;
        }
        self.set_window_bounds(
            Bounds::builder()
                .window_state(WindowState::Fullscreen)
                .build(),
        )
    }

    pub fn window_normal(&self) -> OpenPageResult<()> {
        self.set_window_bounds(Bounds::builder().window_state(WindowState::Normal).build())
    }

    pub fn window_size_set(&self, width: Option<i64>, height: Option<i64>) -> OpenPageResult<()> {
        if width.is_none() && height.is_none() {
            return Ok(());
        }
        if self.window_state()? != "normal" {
            self.window_normal()?;
        }
        let info = self.window_info()?;
        let bounds = Bounds::builder()
            .width(width.unwrap_or(info.bounds.width.unwrap_or_default()))
            .height(height.unwrap_or(info.bounds.height.unwrap_or_default()))
            .build();
        self.set_window_bounds(bounds)
    }

    pub fn window_location_set(&self, left: Option<i64>, top: Option<i64>) -> OpenPageResult<()> {
        if left.is_none() && top.is_none() {
            return Ok(());
        }
        if self.window_state()? != "normal" {
            self.window_normal()?;
        }
        let info = self.window_info()?;
        let bounds = Bounds::builder()
            .left(left.unwrap_or(info.bounds.left.unwrap_or_default()))
            .top(top.unwrap_or(info.bounds.top.unwrap_or_default()))
            .build();
        self.set_window_bounds(bounds)
    }

    pub fn zoom_factor(&self) -> OpenPageResult<f64> {
        if let Some(value) = self.managed_zoom_factor()? {
            return Ok(value);
        }
        let metrics = self.execute_cdp(GetLayoutMetricsParams::default())?;
        Ok(metrics
            .css_visual_viewport
            .zoom
            .unwrap_or(metrics.css_visual_viewport.scale))
    }

    pub fn set_zoom_factor(&self, factor: f64) -> OpenPageResult<()> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(OpenPageError::BrowserOperation(
                zoom_factor_must_be_positive_message(factor),
            ));
        }
        let script = format!(
            "(() => {{ \
                const root = document.documentElement; \
                if (!root) return null; \
                if (root.getAttribute('{managed}') !== '1') {{ \
                    root.setAttribute('{managed}', '1'); \
                    root.setAttribute('{original}', root.style.zoom || ''); \
                }} \
                root.style.zoom = String({factor}); \
                const value = Number.parseFloat(getComputedStyle(root).zoom || root.style.zoom || '1'); \
                return Number.isFinite(value) && value > 0 ? value : 1; \
            }})()",
            managed = PAGE_ZOOM_MANAGED_ATTRIBUTE,
            original = PAGE_ZOOM_ORIGINAL_ATTRIBUTE,
            factor = factor,
        );
        self.run_js_with_options(&script, &[], true, None)?;
        Ok(())
    }

    pub fn reset_zoom_factor(&self) -> OpenPageResult<()> {
        let script = format!(
            "(() => {{ \
                const root = document.documentElement; \
                if (!root) return null; \
                if (root.getAttribute('{managed}') === '1') {{ \
                    const original = root.getAttribute('{original}') || ''; \
                    if (original === '') {{ \
                        root.style.removeProperty('zoom'); \
                    }} else {{ \
                        root.style.zoom = original; \
                    }} \
                    root.removeAttribute('{managed}'); \
                    root.removeAttribute('{original}'); \
                }} \
                return true; \
            }})()",
            managed = PAGE_ZOOM_MANAGED_ATTRIBUTE,
            original = PAGE_ZOOM_ORIGINAL_ATTRIBUTE,
        );
        self.run_js_with_options(&script, &[], true, None)?;
        Ok(())
    }

    fn managed_zoom_factor(&self) -> OpenPageResult<Option<f64>> {
        let script = format!(
            "(() => {{ \
                const root = document.documentElement; \
                if (!root || root.getAttribute('{managed}') !== '1') return null; \
                const raw = getComputedStyle(root).zoom || root.style.zoom || '1'; \
                const value = Number.parseFloat(raw); \
                return Number.isFinite(value) && value > 0 ? value : 1; \
            }})()",
            managed = PAGE_ZOOM_MANAGED_ATTRIBUTE,
        );
        match self.run_js_with_options(&script, &[], true, None)? {
            Value::Null => Ok(None),
            Value::Number(value) => value
                .as_f64()
                .ok_or_else(|| {
                    OpenPageError::JavaScript(value_did_not_return_message(
                        "managed page zoom",
                        "a numeric value",
                        "数值",
                        &value.to_string(),
                    ))
                })
                .map(Some),
            other => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "managed page zoom",
                "a number or null",
                "数字或 null",
                &other.to_string(),
            ))),
        }
    }

    fn ensure_clipboard_api_available(&self, method_name: &str) -> OpenPageResult<()> {
        let available = self.run_js_with_options(
            "Boolean(window.isSecureContext && navigator.clipboard)",
            &[],
            true,
            None,
        )?;
        if available.as_bool() == Some(true) {
            Ok(())
        } else {
            Err(OpenPageError::UnsupportedOperation(
                clipboard_secure_context_required_message(method_name),
            ))
        }
    }

    pub fn listener(&self) -> Listener {
        Listener::new(Arc::clone(&self.runtime), self.inner.clone())
    }

    pub fn listen(&self) -> Listener {
        self.listener()
    }

    pub fn interceptor(&self) -> Interceptor {
        self.interceptor.clone()
    }

    pub fn intercept(&self) -> Interceptor {
        self.interceptor()
    }

    pub fn console(&self) -> Console {
        self.console.clone()
    }

    pub fn screencast(&self) -> Screencast {
        self.screencast.clone()
    }

    pub fn recorder(&self) -> Recorder {
        self.recorder.clone()
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "wait_for_download_begin()",
            ))
        })?;
        browser.wait_for_download_begin_in_frames(
            &self.download_scope_frame_ids()?,
            timeout_ms,
            cancel_it,
        )
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "wait_for_downloads_done()",
            ))
        })?;
        browser.wait_for_downloads_done_in_frames(
            &self.download_scope_frame_ids()?,
            timeout_ms,
            cancel_if_timeout,
        )
    }

    pub fn snapshot(&self) -> OpenPageResult<crate::session::Document> {
        let html = self.html()?;
        let base_url = self.url().ok();
        Ok(crate::session::Document::from_html(html, base_url))
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<DocumentElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        snapshot_find(&self.html()?, locator.raw())
    }

    pub fn snapshot_find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<DocumentElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        snapshot_find_all(&self.html()?, locator.raw())
    }

    pub fn snapshot_find_by(&self, by: &str, value: &str) -> OpenPageResult<DocumentElement> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all_by(
        &self,
        by: &str,
        value: &str,
    ) -> OpenPageResult<Vec<DocumentElement>> {
        let locator = Locator::from_by(by, value)?;
        self.snapshot_find_all(locator.raw())
    }

    pub fn snapshot_query_xpath(
        &self,
        expression: &str,
    ) -> OpenPageResult<Vec<SessionXPathResult>> {
        snapshot_query_xpath(&self.html()?, expression)
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

    pub fn snapshot_root(&self) -> OpenPageResult<DocumentElement> {
        snapshot_root(&self.html()?)
    }

    pub fn user_agent(&self) -> OpenPageResult<String> {
        match self.evaluate("navigator.userAgent")? {
            Value::String(value) => Ok(value),
            value => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "navigator.userAgent",
                "a string",
                "字符串",
                &value.to_string(),
            ))),
        }
    }

    pub fn set_user_agent(&self, user_agent: &str, platform: Option<&str>) -> OpenPageResult<()> {
        let mut params = SetUserAgentOverrideParams::new(user_agent.to_string());
        if let Some(platform) = platform {
            params.platform = Some(platform.to_string());
        }
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::set_user_agent()",
        )?;
        Ok(())
    }

    pub fn set_headers<'a, H>(&self, headers: H) -> OpenPageResult<()>
    where
        H: Into<HeadersInput<'a>>,
    {
        let headers = parse_headers_input(headers)?;
        let header_map = headers
            .into_iter()
            .map(|(name, value)| (name, serde_json::Value::String(value)))
            .collect::<serde_json::Map<_, _>>();
        let params =
            SetExtraHttpHeadersParams::new(Headers::new(serde_json::Value::Object(header_map)));
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            NetworkEnableParams::default(),
            "Page::set_headers()",
        )?;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::set_headers()",
        )?;
        Ok(())
    }

    pub fn set_session_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        let script = match value {
            Some(value) => format!(
                "(() => {{ sessionStorage.setItem({item}, {value}); return true; }})()",
                item = serde_json::to_string(item)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
                value = serde_json::to_string(value)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
            ),
            None => format!(
                "(() => {{ sessionStorage.removeItem({item}); return true; }})()",
                item = serde_json::to_string(item)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
            ),
        };
        self.run_js(&script)?;
        Ok(())
    }

    pub fn session_storage(&self, item: Option<&str>) -> OpenPageResult<Value> {
        self.run_js(&storage_lookup_script("sessionStorage", item)?)
    }

    pub fn set_local_storage(&self, item: &str, value: Option<&str>) -> OpenPageResult<()> {
        let script = match value {
            Some(value) => format!(
                "(() => {{ localStorage.setItem({item}, {value}); return true; }})()",
                item = serde_json::to_string(item)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
                value = serde_json::to_string(value)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
            ),
            None => format!(
                "(() => {{ localStorage.removeItem({item}); return true; }})()",
                item = serde_json::to_string(item)
                    .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
            ),
        };
        self.run_js(&script)?;
        Ok(())
    }

    pub fn local_storage(&self, item: Option<&str>) -> OpenPageResult<Value> {
        self.run_js(&storage_lookup_script("localStorage", item)?)
    }

    pub fn add_init_js(&self, script: &str) -> OpenPageResult<String> {
        let params = AddScriptToEvaluateOnNewDocumentParams::new(script.to_string());
        let identifier: String = execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::add_init_js()",
        )?
        .identifier
        .into();
        self.init_scripts
            .lock()
            .map_err(|_| {
                OpenPageError::PageOperation(component_state_lock_poisoned_message(
                    "page init scripts",
                    "页面初始化脚本",
                ))
            })?
            .push(identifier.clone());
        Ok(identifier)
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        let script_ids = match script_id {
            Some(script_id) => vec![script_id.to_string()],
            None => self
                .init_scripts
                .lock()
                .map_err(|_| {
                    OpenPageError::PageOperation(component_state_lock_poisoned_message(
                        "page init scripts",
                        "页面初始化脚本",
                    ))
                })?
                .clone(),
        };
        if script_ids.is_empty() {
            return Ok(());
        }
        for script_id in &script_ids {
            let params = RemoveScriptToEvaluateOnNewDocumentParams::new(script_id.clone());
            execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.inner,
                params,
                "Page::remove_init_js()",
            )?;
        }
        let mut stored = self.init_scripts.lock().map_err(|_| {
            OpenPageError::PageOperation(component_state_lock_poisoned_message(
                "page init scripts",
                "页面初始化脚本",
            ))
        })?;
        if let Some(script_id) = script_id {
            stored.retain(|existing| existing != script_id);
        } else {
            stored.clear();
        }
        Ok(())
    }

    pub fn clear_cache(
        &self,
        session_storage: bool,
        local_storage: bool,
        cache: bool,
        cookies: bool,
    ) -> OpenPageResult<()> {
        if session_storage {
            self.run_js("(() => { sessionStorage.clear(); return true; })()")?;
        }
        if local_storage {
            self.run_js("(() => { localStorage.clear(); return true; })()")?;
        }
        if cache {
            execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.inner,
                ClearBrowserCacheParams::default(),
                "Page::clear_cache()",
            )?;
        }
        if cookies {
            execute_page_command_blocking(
                self.runtime.as_ref(),
                &self.inner,
                ClearBrowserCookiesParams::default(),
                "Page::clear_cache()",
            )?;
        }
        Ok(())
    }

    pub fn set_cookie_header(&self, url: &str, cookie_header: &str) -> OpenPageResult<()> {
        let url = Url::parse(url).map_err(|err| {
            OpenPageError::PageOperation(invalid_url_message(url, Some(&err.to_string())))
        })?;
        let cookies = cookie_header_to_params(&url, cookie_header);
        if cookies.is_empty() {
            return Ok(());
        }

        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.set_cookies(cookies), "set cookie header")
                .await?;
            Ok(())
        })
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        let current_url = current_cookie_scope_url(self.url()?);
        let current_url = current_url
            .as_deref()
            .map(Url::parse)
            .transpose()
            .map_err(|err| page_operation_error("parse cookie scope url", err))?;
        let cookies = cookie_input_to_params_allow_missing_scope(cookies.into())
            .map_err(|err| page_operation_error("parse cookies", err))?;
        if cookies.is_empty() {
            return Ok(());
        }
        self.runtime.block_on(async {
            for cookie in &cookies {
                set_page_cookie_with_inferred_scope(&self.inner, cookie, current_url.as_ref())
                    .await?;
            }
            Ok(())
        })
    }

    pub fn set_cookie(
        &self,
        name: &str,
        value: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let cookie = cookie_param(name, value, url, domain, path);
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.set_cookie(cookie), "set cookie").await?;
            Ok(())
        })
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        let params = delete_cookie_params(name, url, domain, path);
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.delete_cookie(params), "delete cookie")
                .await?;
            Ok(())
        })
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            ClearBrowserCookiesParams::default(),
            "Page::clear_cookies()",
        )?;
        Ok(())
    }

    pub fn set_permission(
        &self,
        name: &str,
        setting: &str,
        origin: Option<&str>,
        embedded_origin: Option<&str>,
    ) -> OpenPageResult<String> {
        let browser = self.browser_backed_ref("set_permission")?;
        let origin = resolve_permission_origin(origin, &self.url()?)?;
        let embedded_origin = embedded_origin
            .map(permission_origin_from_input)
            .transpose()?;
        let setting = setting.parse::<PermissionSetting>().map_err(|_| {
            OpenPageError::BrowserOperation(permission_setting_invalid_message(setting))
        })?;
        let context_id = browser.browser_context_id(&self.target_id())?;
        browser.set_permission(
            PermissionDescriptor::new(name),
            setting,
            Some(&origin),
            embedded_origin.as_deref(),
            context_id.as_deref(),
        )?;
        Ok(origin)
    }

    pub fn reset_permissions(&self) -> OpenPageResult<()> {
        let browser = self.browser_backed_ref("reset_permissions")?;
        let context_id = browser.browser_context_id(&self.target_id())?;
        browser.reset_permissions(context_id.as_deref())
    }

    pub fn clipboard_read_text(&self) -> OpenPageResult<String> {
        self.ensure_clipboard_api_available("clipboard_read_text")?;
        match self.run_js_with_options("navigator.clipboard.readText()", &[], true, None)? {
            Value::String(value) => Ok(value),
            other => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "clipboard read",
                "text",
                "文本",
                &other.to_string(),
            ))),
        }
    }

    pub fn clipboard_write_text(&self, text: &str) -> OpenPageResult<()> {
        self.ensure_clipboard_api_available("clipboard_write_text")?;
        let text = serde_json::to_string(text)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        let script = format!("navigator.clipboard.writeText({text}).then(() => true)");
        self.run_js_with_options(&script, &[], true, None)?;
        Ok(())
    }

    pub fn main_frame_id(&self) -> OpenPageResult<String> {
        Ok(execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            GetFrameTreeParams::default(),
            "Page::main_frame_id()",
        )?
        .frame_tree
        .frame
        .id
        .as_ref()
        .to_string())
    }

    pub(crate) fn download_scope_frame_ids(&self) -> OpenPageResult<Vec<String>> {
        let frame_tree = execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            GetFrameTreeParams::default(),
            "Page::download_scope_frame_ids()",
        )?;
        let mut frame_ids = Vec::new();
        collect_frame_ids(&frame_tree.frame_tree, &mut frame_ids);
        Ok(frame_ids)
    }

    pub(super) fn window_info(&self) -> OpenPageResult<GetWindowForTargetReturns> {
        let params = GetWindowForTargetParams::builder()
            .target_id(TargetId::new(self.target_id()))
            .build();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::window_info()",
        )
    }

    fn set_window_bounds(&self, bounds: Bounds) -> OpenPageResult<()> {
        let info = self.window_info()?;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            SetWindowBoundsParams::new(info.window_id, bounds),
            "Page::set_window_bounds()",
        )?;
        Ok(())
    }

    pub fn close(self) -> OpenPageResult<()> {
        let target_id = self.target_id();
        if let Some(browser) = &self.browser {
            return browser.close_target(&target_id);
        }
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.close(), "close page").await?;
            Ok::<(), OpenPageError>(())
        })?;
        Ok(())
    }

    pub(super) fn download_with_cookie_scope(
        &self,
        url: &str,
        cookie_scope_url: Option<&str>,
    ) -> OpenPageResult<String> {
        self.build_download_session(cookie_scope_url)?.download(url)
    }

    pub(super) fn download_to_with_cookie_scope(
        &self,
        url: &str,
        path: impl AsRef<Path>,
        cookie_scope_url: Option<&str>,
    ) -> OpenPageResult<String> {
        self.build_download_session(cookie_scope_url)?
            .download_to(url, path)
    }

    fn build_download_session(&self, cookie_scope_url: Option<&str>) -> OpenPageResult<Session> {
        let mut options = SessionOptions {
            user_agent: Some(self.user_agent()?),
            ..SessionOptions::default()
        };
        if let Some(download_path) = self.download_path()? {
            options.download_path = PathBuf::from(download_path);
        }

        let session = Session::new(options)?;
        if let Some(scope_url) = cookie_scope_url {
            if (scope_url.starts_with("http://") || scope_url.starts_with("https://"))
                && let Some(cookie_header) = self.cookie_header()?
            {
                session.set_cookie_header(scope_url, &cookie_header)?;
            }
        }
        Ok(session)
    }

    pub(super) fn wait_for_change<F>(
        &self,
        timeout_ms: u64,
        mut predicate: F,
    ) -> OpenPageResult<bool>
    where
        F: FnMut(&Self) -> OpenPageResult<bool>,
    {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            if predicate(self)? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("Page::wait_for_change()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub(super) fn load_mode_value(&self) -> OpenPageResult<LoadMode> {
        self.load_mode.lock().map(|mode| *mode).map_err(|_| {
            OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
                "page load mode",
                "页面加载模式",
            ))
        })
    }

    pub(super) fn navigate_via_cdp(&self, url: &str) -> OpenPageResult<()> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(self.inner.goto(url.to_string()), "navigate").await?;
            Ok::<(), OpenPageError>(())
        })
    }

    pub(super) fn navigate_via_script(&self, url: &str) -> OpenPageResult<()> {
        let script = format!(
            "(() => {{ window.location.href = {url}; return true; }})()",
            url = serde_json::to_string(url)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
        );
        self.run_js(&script)?;
        Ok(())
    }

    pub(super) fn navigate_history(&self, offset: isize) -> OpenPageResult<bool> {
        if offset == 0 {
            return Ok(true);
        }
        let history = execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            GetNavigationHistoryParams::default(),
            "Page::navigate_history()",
        )?;
        let Some(target_index) = history_entry_index(
            history.current_index as usize,
            history.entries.len(),
            offset,
        ) else {
            return Ok(false);
        };
        let entry_id = history
            .entries
            .get(target_index)
            .ok_or_else(|| {
                OpenPageError::PageOperation(navigation_history_index_out_of_bounds_message(
                    target_index,
                ))
            })?
            .id;
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            NavigateToHistoryEntryParams::new(entry_id),
            "Page::navigate_history()",
        )?;
        Ok(true)
    }

    pub(super) fn wait_for_ready_state_change(
        &self,
        timeout_ms: u64,
        include_interactive: bool,
    ) -> OpenPageResult<bool> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.ready_state() {
                Ok(state) if state == "complete" => return Ok(true),
                Ok(state) if include_interactive && state == "interactive" => return Ok(true),
                Ok(_) => {}
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub(super) fn wait_for_dom_ready(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            if self.html().is_ok() {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub(crate) fn frame_from_element(&self, element: Element) -> OpenPageResult<Frame> {
        self.frame_from_element_with_config_source(element, &self.none_element_config)
    }

    pub(super) fn frame_from_locator_with_config_source(
        &self,
        locator: &str,
        config_source: &ElementsOneConfigHandle,
    ) -> OpenPageResult<Frame> {
        let element = self.get_frame_ele(locator)?;
        self.frame_from_element_with_config_source(element, config_source)
    }

    pub(crate) fn frame_from_element_with_config_source(
        &self,
        element: Element,
        config_source: &ElementsOneConfigHandle,
    ) -> OpenPageResult<Frame> {
        let backend_node_id = element.backend_node_id();
        let frame_id = self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.inner,
                DescribeNodeParams::builder()
                    .backend_node_id(backend_node_id)
                    .build(),
                "Page::frame_from_element()",
            )
            .await?;
            response
                .node
                .frame_id
                .map(|frame_id| frame_id.as_ref().to_string())
                .ok_or_else(|| {
                    OpenPageError::PageOperation(frame_element_missing_frame_id_message())
                })
        });
        let frame_id = match frame_id {
            Ok(frame_id) => frame_id,
            Err(describe_err) => {
                let marker = next_page_marker();
                element.set_attr(PAGE_MARKER_ATTRIBUTE, &marker)?;
                let detected = (|| -> OpenPageResult<Option<String>> {
                    let main_frame_id = self.main_frame_id()?;
                    for candidate_frame_id in self.download_scope_frame_ids()? {
                        if candidate_frame_id == main_frame_id {
                            continue;
                        }
                        let owner_element = self.frame_owner_element_by_id(&candidate_frame_id)?;
                        if owner_element.attr(PAGE_MARKER_ATTRIBUTE)?.as_deref() == Some(&marker) {
                            return Ok(Some(candidate_frame_id));
                        }
                    }
                    Ok(None)
                })();
                let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
                match detected {
                    Ok(Some(frame_id)) => frame_id,
                    Ok(None) => return Err(describe_err),
                    Err(err) => return Err(err),
                }
            }
        };
        if let Some(frame) = self.cached_frame(&frame_id)? {
            return Ok(frame);
        }
        let none_element_config = self.frame_none_element_config_from(&frame_id, config_source)?;
        let frame = Frame::new(self.clone(), frame_id, element, none_element_config);
        self.cache_frame(&frame)?;
        Ok(frame)
    }

    pub(super) fn frame_owner_element_by_id(&self, frame_id: &str) -> OpenPageResult<Element> {
        let (node_id, backend_node_id) = self.runtime.block_on(async {
            let response = execute_page_command_async(
                &self.inner,
                GetFrameOwnerParams::new(FrameId::new(frame_id.to_string())),
                "Page::frame_owner_element_by_id()",
            )
            .await?;
            Ok::<
                (
                    Option<chromiumoxide::cdp::browser_protocol::dom::NodeId>,
                    BackendNodeId,
                ),
                OpenPageError,
            >((response.node_id, response.backend_node_id))
        })?;
        if let Some(node_id) = node_id.as_ref()
            && let Some(parent_frame_id) = self.frame_parent_id(frame_id)?
            && parent_frame_id != self.main_frame_id()?
            && let Ok(element) =
                self.resolve_frame_owner_node_in_parent_frame(&parent_frame_id, node_id.clone())
        {
            return Ok(element);
        }
        if let Some(node_id) = node_id {
            match self
                .resolve_dom_node_id(node_id, "frame owner could not be resolved to an element")
            {
                Ok(element) => Ok(element),
                Err(OpenPageError::PageOperation(message))
                    if message.contains("Could not find node with given id") =>
                {
                    self.resolve_dom_backend_node_id(backend_node_id)
                }
                Err(err) => Err(err),
            }
        } else {
            self.resolve_dom_backend_node_id(backend_node_id)
        }
    }

    fn resolve_frame_owner_node_in_parent_frame(
        &self,
        parent_frame_id: &str,
        node_id: chromiumoxide::cdp::browser_protocol::dom::NodeId,
    ) -> OpenPageResult<Element> {
        let marker = next_page_marker();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            SetAttributeValueParams::new(node_id, PAGE_MARKER_ATTRIBUTE, marker.clone()),
            "Page::resolve_frame_owner_node_in_parent_frame()",
        )?;

        let selector = marker_selector(&marker);
        let element = (|| -> OpenPageResult<Element> {
            let parent_owner = self.frame_owner_element_by_id(parent_frame_id)?;
            let parent_frame = self.frame_from_element(parent_owner)?;
            parent_frame.find(selector.as_str())
        })();
        let cleanup = self.runtime.block_on(async {
            let _ = execute_page_command_async(
                &self.inner,
                RemoveAttributeParams::new(node_id, PAGE_MARKER_ATTRIBUTE),
                "Page::resolve_frame_owner_node_in_parent_frame()",
            )
            .await;
            Ok::<(), OpenPageError>(())
        });

        match (element, cleanup) {
            (Ok(element), Ok(())) => Ok(element),
            (Err(OpenPageError::Timeout(message)), _) => Err(OpenPageError::Timeout(message)),
            (Err(_), Ok(())) => Err(OpenPageError::ElementNotFound(
                "frame owner could not be resolved to an element".to_string(),
            )),
            (Err(err), Err(_)) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    pub(crate) fn resolve_dom_backend_node_id(
        &self,
        backend_node_id: BackendNodeId,
    ) -> OpenPageResult<Element> {
        let node_id = self.runtime.block_on(async {
            let resolved = execute_page_command_async(
                &self.inner,
                ResolveNodeParams::builder()
                    .backend_node_id(backend_node_id)
                    .build(),
                "Page::resolve_dom_backend_node_id()",
            )
            .await?;
            let object_id = resolved.object.object_id.ok_or_else(|| {
                OpenPageError::PageOperation(resolved_frame_owner_missing_object_id_message())
            })?;
            let requested = execute_page_command_async(
                &self.inner,
                RequestNodeParams::new(object_id),
                "Page::resolve_dom_backend_node_id()",
            )
            .await?;
            Ok::<chromiumoxide::cdp::browser_protocol::dom::NodeId, OpenPageError>(
                requested.node_id,
            )
        })?;
        self.resolve_dom_node_id(node_id, "frame owner could not be resolved to an element")
    }

    fn resolve_dom_node_id(
        &self,
        node_id: chromiumoxide::cdp::browser_protocol::dom::NodeId,
        error_message: &str,
    ) -> OpenPageResult<Element> {
        let marker = next_page_marker();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            SetAttributeValueParams::new(node_id, PAGE_MARKER_ATTRIBUTE, marker.clone()),
            "Page::resolve_dom_node_id()",
        )?;

        let element = self.find(marker_selector(&marker).as_str());
        let cleanup = self.runtime.block_on(async {
            let _ = execute_page_command_async(
                &self.inner,
                RemoveAttributeParams::new(node_id, PAGE_MARKER_ATTRIBUTE),
                "Page::resolve_dom_node_id()",
            )
            .await;
            Ok::<(), OpenPageError>(())
        });

        match (element, cleanup) {
            (Ok(element), Ok(())) => Ok(element),
            (Err(OpenPageError::Timeout(message)), _) => Err(OpenPageError::Timeout(message)),
            (Err(_), Ok(())) => Err(OpenPageError::ElementNotFound(error_message.to_string())),
            (Err(err), Err(_)) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    pub(super) fn frame_name_by_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.frame_name(FrameId::new(frame_id.to_string())),
                "read frame name",
            )
            .await
        })
    }

    pub(super) fn frame_url_by_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.frame_url(FrameId::new(frame_id.to_string())),
                "read frame url",
            )
            .await
        })
    }

    pub(crate) fn frame_parent_id(&self, frame_id: &str) -> OpenPageResult<Option<String>> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner.frame_parent(FrameId::new(frame_id.to_string())),
                "read frame parent",
            )
            .await
            .map(|value| value.map(|frame_id| frame_id.as_ref().to_string()))
        })
    }

    fn frame_context_id(&self, frame_id: &str) -> OpenPageResult<ExecutionContextId> {
        self.runtime.block_on(async {
            run_page_future_with_cdp_timeout(
                self.inner
                    .frame_execution_context(FrameId::new(frame_id.to_string())),
                "read frame execution context",
            )
            .await?
            .ok_or_else(|| {
                OpenPageError::PageOperation(frame_execution_context_unavailable_message(frame_id))
            })
        })
    }

    pub(super) fn evaluate_in_frame(
        &self,
        frame_id: &str,
        expression: &str,
    ) -> OpenPageResult<Value> {
        self.evaluate_in_frame_with_options(frame_id, expression, None, false)
    }

    pub(super) fn evaluate_in_frame_with_options(
        &self,
        frame_id: &str,
        expression: &str,
        timeout_ms: Option<u64>,
        await_promise: bool,
    ) -> OpenPageResult<Value> {
        let timeout_ms = resolve_javascript_timeout_ms(timeout_ms, self.javascript_timeout_ms()?);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            let context_id = self.frame_context_id(frame_id)?;
            let params = EvaluateParams::builder()
                .expression(expression)
                .context_id(context_id)
                .await_promise(await_promise)
                .build()
                .map_err(OpenPageError::PageOperation)?;
            match self.evaluate_params_with_timeout(params, remaining_timeout_ms(deadline)) {
                Ok(value) => return Ok(value),
                Err(OpenPageError::JavaScript(message))
                    if frame_execution_context_was_stale(&message) && Instant::now() < deadline =>
                {
                    sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub(super) fn clear_page_markers(&self, markers: &[&str]) -> OpenPageResult<()> {
        if markers.is_empty() {
            return Ok(());
        }
        let markers = serde_json::to_string(markers)
            .map_err(|err| OpenPageError::Serialization(err.to_string()))?;
        let script = format!(
            "(() => {{ \
                const attr = {attr}; \
                const markers = {markers}; \
                for (const marker of markers) {{ \
                    const element = document.querySelector(`[${{attr}}=\"${{marker}}\"]`); \
                    if (element) element.removeAttribute(attr); \
                }} \
                return true; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            markers = markers,
        );
        self.run_js(&script)?;
        Ok(())
    }
}
