use super::*;

impl Frame {
    pub fn id(&self) -> &str {
        &self.frame_id
    }

    pub fn frame_id(&self) -> &str {
        self.id()
    }

    pub fn frame_element(&self) -> &Element {
        &self.frame_element
    }

    pub fn frame_ele(&self) -> &Element {
        self.frame_element()
    }

    pub fn owner(&self) -> &Page {
        &self.page
    }

    pub fn page(&self) -> &Page {
        self.owner()
    }

    pub fn tab(&self) -> &Page {
        &self.page
    }

    pub fn tab_id(&self) -> String {
        self.page.target_id()
    }

    pub fn scroll(&self) -> FrameScroller<'_> {
        FrameScroller { frame: self }
    }

    pub fn set(&self) -> FrameSetter<'_> {
        FrameSetter { frame: self }
    }

    pub fn states(&self) -> FrameStates<'_> {
        FrameStates { frame: self }
    }

    pub fn wait(&self) -> FrameWait<'_> {
        FrameWait { frame: self }
    }

    pub fn rect(&self) -> FrameRect<'_> {
        FrameRect { frame: self }
    }

    pub fn link(&self) -> OpenPageResult<Option<String>> {
        self.frame_element.link()
    }

    pub fn tag(&self) -> OpenPageResult<String> {
        self.frame_element.tag()
    }

    pub fn attrs(&self) -> OpenPageResult<Vec<(String, String)>> {
        self.frame_element.attrs()
    }

    pub fn attr(&self, name: &str) -> OpenPageResult<Option<String>> {
        self.frame_element.attr(name)
    }

    pub fn property(&self, name: &str) -> OpenPageResult<Option<Value>> {
        self.frame_element.property(name)
    }

    pub fn text(&self) -> OpenPageResult<Option<String>> {
        self.frame_element.text()
    }

    pub fn raw_text(&self) -> OpenPageResult<Option<String>> {
        self.frame_element.raw_text()
    }

    pub fn value(&self) -> OpenPageResult<Option<String>> {
        self.frame_element.value()
    }

    pub fn comments(&self) -> OpenPageResult<Vec<String>> {
        self.frame_element.comments()
    }

    pub fn texts(&self, text_node_only: bool) -> OpenPageResult<Vec<String>> {
        self.frame_element.texts(text_node_only)
    }

    pub fn src(
        &self,
        timeout_ms: u64,
        base64_to_bytes: bool,
    ) -> OpenPageResult<Option<ElementResource>> {
        self.frame_element.src(timeout_ms, base64_to_bytes)
    }

    pub fn save(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        timeout_ms: u64,
        rename: bool,
    ) -> OpenPageResult<PathBuf> {
        self.frame_element.save(path, name, timeout_ms, rename)
    }

    pub fn style(&self, name: &str, pseudo: Option<&str>) -> OpenPageResult<String> {
        self.frame_element.style(name, pseudo)
    }

    pub fn pseudo_before(&self) -> OpenPageResult<String> {
        self.frame_element.pseudo_before()
    }

    pub fn pseudo_after(&self) -> OpenPageResult<String> {
        self.frame_element.pseudo_after()
    }

    pub fn scroll_to_see(&self, center: Option<bool>) -> OpenPageResult<()> {
        self.frame_element.scroll_to_see(center)
    }

    pub fn scroll_to_center(&self) -> OpenPageResult<()> {
        self.frame_element.scroll_to_center()
    }

    pub fn css_path(&self) -> OpenPageResult<String> {
        self.frame_element.css_path()
    }

    pub fn xpath(&self) -> OpenPageResult<String> {
        self.frame_element.xpath()
    }

    pub fn child_count(&self) -> OpenPageResult<usize> {
        self.find_all("xpath:./*").map(|elements| elements.len())
    }

    pub fn sr(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.frame_element.sr()
    }

    pub fn shadow_root(&self) -> OpenPageResult<Option<ShadowRoot>> {
        self.frame_element.shadow_root()
    }

    pub fn name(&self) -> OpenPageResult<Option<String>> {
        self.page.frame_name_by_id(&self.frame_id)
    }

    pub fn url(&self) -> OpenPageResult<Option<String>> {
        self.page.frame_url_by_id(&self.frame_id)
    }

    pub fn parent_id(&self) -> OpenPageResult<Option<String>> {
        self.page.frame_parent_id(&self.frame_id)
    }

    pub fn title(&self) -> OpenPageResult<Option<String>> {
        value_as_optional_string(self.run_js("document.title")?, "frame title")
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        self.page.download_path()
    }

    pub fn download(&self, url: &str) -> OpenPageResult<String> {
        let scope_url = self.url()?;
        self.page
            .download_with_cookie_scope(url, scope_url.as_deref())
    }

    pub fn download_to(&self, url: &str, path: impl AsRef<Path>) -> OpenPageResult<String> {
        let scope_url = self.url()?;
        self.page
            .download_to_with_cookie_scope(url, path, scope_url.as_deref())
    }

    pub fn inner_html(&self) -> OpenPageResult<String> {
        value_as_string(
            self.run_js("document.documentElement ? document.documentElement.outerHTML : ''")?,
            "frame inner html",
        )
    }

    pub fn html(&self) -> OpenPageResult<String> {
        let tag = self.frame_element.tag()?;
        let outer_html = self
            .frame_element
            .html()?
            .ok_or_else(|| OpenPageError::ElementNotFound(frame_html_unavailable_message()))?;
        let inner_html = self.inner_html()?;
        Ok(compose_frame_html(&tag, &outer_html, &inner_html))
    }

    pub fn run_js(&self, expression: &str) -> OpenPageResult<Value> {
        let script = load_javascript_source(expression)?;
        match script {
            Cow::Borrowed(expression) => self.page.evaluate_in_frame(&self.frame_id, expression),
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
        self.page
            .evaluate_in_frame_with_options(&self.frame_id, &expression, timeout_ms, true)
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
        let _ = self.wait_for_doc_loaded(self.page.navigation_page_load_timeout_ms()?);
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
        self.page
            .evaluate_in_frame_with_options(&self.frame_id, &expression, timeout_ms, false)
            .map(|_| ())
    }

    pub fn add_init_js(&self, script: &str) -> OpenPageResult<String> {
        self.page.add_init_js(script)
    }

    pub fn remove_init_js(&self, script_id: Option<&str>) -> OpenPageResult<()> {
        self.page.remove_init_js(script_id)
    }

    pub fn refresh(&self) -> OpenPageResult<()> {
        self.refresh_with_options(false)
    }

    pub fn refresh_with_options(&self, ignore_cache: bool) -> OpenPageResult<()> {
        let script = format!(
            "(() => {{ window.location.reload({ignore_cache}); return true; }})()",
            ignore_cache = if ignore_cache { "true" } else { "false" },
        );
        self.run_js(&script).map(|_| ())
    }

    pub fn get(&self, url: &str) -> OpenPageResult<bool> {
        self.goto(url).map(|_| true)
    }

    pub fn goto(&self, url: &str) -> OpenPageResult<()> {
        let url = normalize_navigation_target(url)?;
        let old_url = self.url().ok().flatten();
        let timeout_ms = self.page.navigation_page_load_timeout_ms()?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let script = format!(
            "(() => {{ window.location.href = {url}; return true; }})()",
            url = serde_json::to_string(&url)
                .map_err(|err| OpenPageError::Serialization(err.to_string()))?,
        );
        self.run_js(&script)?;

        if self.page.load_mode_value()? == LoadMode::None {
            return Ok(());
        }

        loop {
            let current_url = self.url().ok().flatten();
            if current_url.as_deref() == Some(url.as_str())
                || (current_url.is_some() && current_url != old_url)
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(OpenPageError::Timeout(page_connect_timed_out_message(&url)));
            }
            sleep(Duration::from_millis(50));
        }

        if self.wait_for_doc_loaded(remaining_timeout_ms(deadline))? {
            Ok(())
        } else {
            Err(OpenPageError::Timeout(page_connect_timed_out_message(&url)))
        }
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Self> {
        let page = self.page.reconnect(wait_ms)?;
        if let Ok(Some(id)) = self.frame_element.attr("id")
            && !id.is_empty()
        {
            let locator = format!("css:#{id}");
            if let Ok(frame) = page
                .frame_from_locator_with_config_source(locator.as_str(), &self.none_element_config)
            {
                return Ok(frame);
            }
        }
        if let Ok(Some(name)) = self.frame_element.attr("name")
            && !name.is_empty()
        {
            let locator = format!(r#"css:iframe[name="{name}"],frame[name="{name}"]"#);
            if let Ok(frame) = page
                .frame_from_locator_with_config_source(locator.as_str(), &self.none_element_config)
            {
                return Ok(frame);
            }
        }
        if let Ok(xpath) = self.frame_element.xpath()
            && !xpath.is_empty()
        {
            let locator = format!("xpath:{xpath}");
            if let Ok(frame) = page
                .frame_from_locator_with_config_source(locator.as_str(), &self.none_element_config)
            {
                return Ok(frame);
            }
        }
        if let Ok(css_path) = self.frame_element.css_path()
            && !css_path.is_empty()
        {
            let locator = format!("css:{css_path}");
            if let Ok(frame) = page
                .frame_from_locator_with_config_source(locator.as_str(), &self.none_element_config)
            {
                return Ok(frame);
            }
        }
        let frame_element = page.get_frame_ele(self.frame_element())?;
        page.frame_from_element_with_config_source(frame_element, &self.none_element_config)
    }

    pub fn disconnect(self) -> OpenPageResult<DisconnectedFrame> {
        let frame_dom_id = self.frame_element.attr("id")?;
        let frame_dom_name = self.frame_element.attr("name")?;
        let frame_xpath = self
            .frame_element
            .xpath()
            .ok()
            .filter(|xpath| !xpath.is_empty());
        let frame_css_path = self
            .frame_element
            .css_path()
            .ok()
            .filter(|css_path| !css_path.is_empty());
        Ok(DisconnectedFrame {
            page: self.page.disconnect()?,
            frame_id: self.frame_id,
            frame_dom_id,
            frame_dom_name,
            frame_xpath,
            frame_css_path,
            frame_backend_node_id: self.frame_element.backend_node_id(),
            none_element_config: self.none_element_config,
        })
    }

    pub fn remove_attr(&self, name: &str) -> OpenPageResult<()> {
        self.frame_element.remove_attr(name)
    }

    pub fn set_attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame_element.set_attr(name, value)
    }

    pub fn set_property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        self.frame_element.set_property(name, value)
    }

    pub fn set_style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame_element.set_style(name, value)
    }

    pub fn click(&self) -> OpenPageResult<()> {
        self.frame_element.click()
    }

    pub fn click_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        self.frame_element
            .click_with_options(by_js, timeout_ms, wait_stop)
    }

    pub fn click_left_with_options(
        &self,
        by_js: Option<bool>,
        timeout_ms: Option<u64>,
        wait_stop: bool,
    ) -> OpenPageResult<bool> {
        self.frame_element
            .click_left_with_options(by_js, timeout_ms, wait_stop)
    }

    pub fn click_at(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
        button: &str,
        count: u32,
    ) -> OpenPageResult<()> {
        self.frame_element
            .click_at(offset_x, offset_y, button, count)
    }

    pub fn click_multi(&self, times: u32) -> OpenPageResult<()> {
        self.frame_element.click_multi(times)
    }

    pub fn click_left(&self) -> OpenPageResult<()> {
        self.frame_element.click_left()
    }

    pub fn click_right(&self) -> OpenPageResult<()> {
        self.frame_element.click_right()
    }

    pub fn input<'a, I>(&self, text: I) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.frame_element.input(text)
    }

    pub fn input_with_options<'a, I>(&self, text: I, clear: bool, by_js: bool) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.frame_element.input_with_options(text, clear, by_js)
    }

    pub fn input_keys_with_options<'a, I>(
        &self,
        values: I,
        clear: bool,
        by_js: bool,
    ) -> OpenPageResult<()>
    where
        I: Into<ActionsInput<'a>>,
    {
        self.frame_element
            .input_keys_with_options(values, clear, by_js)
    }

    pub fn press_key(&self, key: &str) -> OpenPageResult<()> {
        self.frame_element.press_key(key)
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.frame_element.clear()
    }

    pub fn clear_with_mode(&self, by_js: bool) -> OpenPageResult<()> {
        self.frame_element.clear_with_mode(by_js)
    }

    pub fn submit(&self) -> OpenPageResult<()> {
        self.frame_element.submit()
    }

    pub fn focus(&self) -> OpenPageResult<()> {
        self.frame_element.focus()
    }

    pub fn hover(&self) -> OpenPageResult<()> {
        self.frame_element.hover()
    }

    pub fn hover_with_offset(
        &self,
        offset_x: Option<f64>,
        offset_y: Option<f64>,
    ) -> OpenPageResult<()> {
        self.frame_element.hover_with_offset(offset_x, offset_y)
    }

    pub fn drag(&self, offset_x: f64, offset_y: f64, duration_secs: f64) -> OpenPageResult<()> {
        self.frame_element.drag(offset_x, offset_y, duration_secs)
    }

    pub fn drag_to<'a, T>(&self, target: T, duration_secs: f64) -> OpenPageResult<()>
    where
        T: Into<ElementDragTarget<'a>>,
    {
        self.frame_element.drag_to(target, duration_secs)
    }

    pub fn drag_to_point(&self, x: f64, y: f64, duration_secs: f64) -> OpenPageResult<()> {
        self.frame_element.drag_to_point(x, y, duration_secs)
    }

    pub fn set_checked(&self, checked: bool) -> OpenPageResult<()> {
        self.frame_element.set_checked(checked)
    }

    pub fn check(&self, uncheck: bool, by_js: bool) -> OpenPageResult<()> {
        self.frame_element.check(uncheck, by_js)
    }

    pub fn uncheck(&self, by_js: bool) -> OpenPageResult<()> {
        self.frame_element.uncheck(by_js)
    }

    pub fn set_upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.page.set_upload_files(files)
    }

    pub fn set_upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.set_upload_files(files)
    }

    pub fn set_cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.page.set_cookies(cookies)
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.page.remove_cookie(name, url, domain, path)
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.page.clear_cookies()
    }

    pub fn set_download_path(&self, path: &str) -> OpenPageResult<()> {
        self.page.set_tab_download_path(path)
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
                    "none element runtime config",
                    "未找到元素运行时配置",
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
                    "none element runtime config",
                    "未找到元素运行时配置",
                ))
            })
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        self.page.set_tab_download_file_exists_mode(mode)
    }

    pub fn set_when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.page.set_tab_when_download_file_exists(mode)
    }

    pub fn set_download_filename(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.page
            .set_tab_download_filename(rename, suffix, suffix_specified)
    }

    pub fn set_download_file_name(
        &self,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.set_download_filename(rename, suffix, suffix_specified)
    }

    pub fn click_to_download<'a, L>(
        &self,
        locator: L,
        save_path: Option<&str>,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
        timeout_ms: Option<u64>,
        by_js: bool,
        new_tab: bool,
    ) -> OpenPageResult<Option<DownloadMission>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.page.click_to_download(
            locator,
            save_path,
            rename,
            suffix,
            suffix_specified,
            timeout_ms,
            by_js,
            new_tab,
        )
    }

    pub fn click_to_upload<'a, 'b, L, F>(
        &self,
        locator: L,
        files: F,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<bool>
    where
        L: Into<LocatorInput<'a>>,
        F: Into<UploadFilesInput<'b>>,
    {
        self.page.click_to_upload(locator, files, timeout_ms, by_js)
    }

    pub fn click_for_new_tab<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        by_js: bool,
    ) -> OpenPageResult<Option<Page>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.page.click_for_new_tab(locator, timeout_ms, by_js)
    }

    pub fn click_middle<'a, L>(
        &self,
        locator: L,
        timeout_ms: Option<u64>,
        get_tab: bool,
    ) -> OpenPageResult<Option<Page>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.page.click_middle(locator, timeout_ms, get_tab)
    }

    pub fn wait_for_upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.page.wait_for_upload_paths_inputted(timeout_ms)
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        self.page.wait_for_download_begin(timeout_ms, cancel_it)
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        self.page
            .wait_for_downloads_done(timeout_ms, cancel_if_timeout)
    }

    fn bind_element_runtime_config(&self, element: Element) -> Element {
        element.with_runtime_config_handles(
            Arc::clone(&self.none_element_config),
            Arc::clone(&self.page.frame_cache),
            Arc::clone(&self.page.frame_none_element_configs),
        )
    }

    pub fn active_element(&self) -> OpenPageResult<Option<Element>> {
        let marker = next_page_marker();
        let script = format!(
            "(() => {{ \
                const active = document.activeElement; \
                if (!active || !(active instanceof Element)) return null; \
                active.setAttribute({attr}, {marker}); \
                return {marker}; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            marker = json_string(&marker)?,
        );
        match self.run_js(&script)? {
            Value::Null => Ok(None),
            Value::String(_) => {
                let element = self.page.find(&marker_xpath(&marker))?;
                let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
                Ok(Some(self.bind_element_runtime_config(element)))
            }
            other => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "frame active element",
                "a string or null",
                "字符串或 null",
                &other.to_string(),
            ))),
        }
    }

    pub fn active_ele(&self) -> OpenPageResult<Option<Element>> {
        self.active_element()
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
        let marker = next_page_marker();
        let script = frame_find_script(&locator, &marker)?;
        match self.run_js(&script)? {
            Value::Null => Err(OpenPageError::ElementNotFound(
                frame_element_not_found_message(locator.raw()),
            )),
            Value::String(_) => {
                let element = self.page.find(&marker_xpath(&marker))?;
                let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
                Ok(self.bind_element_runtime_config(element))
            }
            other => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "frame find()",
                "a string or null",
                "字符串或 null",
                &other.to_string(),
            ))),
        }
    }

    pub fn find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        let batch = next_page_marker();
        let script = frame_find_all_script(&locator, &batch)?;
        let markers = value_as_string_vec(self.run_js(&script)?, "frame find_all() result")?;
        let mut elements = Vec::with_capacity(markers.len());
        for marker in markers {
            let element = self.page.find(&marker_xpath(&marker))?;
            let _ = element.remove_attr(PAGE_MARKER_ATTRIBUTE);
            elements.push(self.bind_element_runtime_config(element));
        }
        Ok(elements)
    }

    pub fn get_frame<'a, L>(&self, target: L) -> OpenPageResult<Frame>
    where
        L: Into<PageFrameTarget<'a>>,
    {
        let target = target.into();
        match &target {
            PageFrameTarget::Frame(frame) => {
                self.resolve_frame_target(target.clone())?;
                return Ok((*frame).clone());
            }
            PageFrameTarget::WebFrame(frame) => {
                self.resolve_frame_target(target.clone())?;
                return Ok(frame.frame().clone());
            }
            PageFrameTarget::OwnedFrame(frame) => {
                self.resolve_frame_target(target.clone())?;
                return Ok(frame.clone());
            }
            PageFrameTarget::OwnedWebFrame(frame) => {
                self.resolve_frame_target(target.clone())?;
                return Ok(frame.frame().clone());
            }
            _ => {}
        }
        self.page.frame_from_element_with_config_source(
            self.get_frame_ele(target)?,
            &self.none_element_config,
        )
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
        self.resolve_frame_target(target.into())
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
            .map(|element| {
                self.page
                    .frame_from_element_with_config_source(element, &self.none_element_config)
            })
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
        let locator = optional_frame_locator_input(locator)?;
        self.find_all(locator.as_str())
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

    pub fn parent(&self) -> OpenPageResult<Element> {
        self.frame_element.parent()
    }

    pub fn parent_level(&self, level: usize) -> OpenPageResult<Element> {
        self.frame_element.parent_level(level)
    }

    pub fn parent_with<'a, L>(&self, locator: L, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.parent_with(locator, index)
    }

    pub fn child(&self) -> OpenPageResult<Element> {
        self.frame_element.child()
    }

    pub fn child_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.child_with(locator, index)
    }

    pub fn children(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.children()
    }

    pub fn children_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.children_with(locator)
    }

    pub fn prev(&self) -> OpenPageResult<Element> {
        self.frame_element.prev()
    }

    pub fn prev_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.prev_with(locator, index)
    }

    pub fn next(&self) -> OpenPageResult<Element> {
        self.frame_element.next()
    }

    pub fn next_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.next_with(locator, index)
    }

    pub fn before(&self) -> OpenPageResult<Element> {
        self.frame_element.before()
    }

    pub fn before_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.before_with(locator, index)
    }

    pub fn after(&self) -> OpenPageResult<Element> {
        self.frame_element.after()
    }

    pub fn after_with<'a, L>(&self, locator: Option<L>, index: usize) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.after_with(locator, index)
    }

    pub fn prevs(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.prevs()
    }

    pub fn prevs_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.prevs_with(locator)
    }

    pub fn nexts(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.nexts()
    }

    pub fn nexts_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.nexts_with(locator)
    }

    pub fn befores(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.befores()
    }

    pub fn befores_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.befores_with(locator)
    }

    pub fn afters(&self) -> OpenPageResult<Vec<Element>> {
        self.frame_element.afters()
    }

    pub fn afters_with<'a, L>(&self, locator: Option<L>) -> OpenPageResult<Vec<Element>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.afters_with(locator)
    }

    pub fn over(&self) -> OpenPageResult<Option<Element>> {
        self.frame_element.over()
    }

    pub fn over_with_timeout(&self, timeout_ms: u64) -> OpenPageResult<Option<Element>> {
        self.frame_element.over_with_timeout(timeout_ms)
    }

    pub fn offset<'a, L>(
        &self,
        locator: Option<L>,
        x: Option<f64>,
        y: Option<f64>,
        timeout_ms: u64,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.offset(locator, x, y, timeout_ms)
    }

    pub fn east<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.east(locator, pixels, index)
    }

    pub fn south<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.south(locator, pixels, index)
    }

    pub fn west<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.west(locator, pixels, index)
    }

    pub fn north<'a, L>(
        &self,
        locator: Option<L>,
        pixels: Option<i64>,
        index: usize,
    ) -> OpenPageResult<Element>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.frame_element.north(locator, pixels, index)
    }

    pub fn screenshot_bytes(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<Vec<u8>> {
        self.frame_element
            .screenshot_bytes(scroll_to_center, timeout_ms)
    }

    pub fn screenshot_base64(
        &self,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        self.frame_element
            .screenshot_base64(scroll_to_center, timeout_ms)
    }

    pub fn get_screenshot(
        &self,
        path: Option<&Path>,
        name: Option<&str>,
        scroll_to_center: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<PathBuf> {
        self.frame_element
            .get_screenshot(path, name, scroll_to_center, timeout_ms)
    }

    pub fn save_screenshot(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        self.frame_element.save_screenshot(path)
    }

    pub fn scroll_to_top(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, 0), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_bottom(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, document.scrollingElement.scrollHeight), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_half(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollLeft, document.scrollingElement.scrollHeight / 2), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_rightmost(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(document.scrollingElement.scrollWidth, document.scrollingElement.scrollTop), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_leftmost(&self) -> OpenPageResult<()> {
        self.run_js(
            "(document.scrollingElement.scrollTo(0, document.scrollingElement.scrollTop), true)",
        )
        .map(|_| ())
    }

    pub fn scroll_to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollTo({x}, {y}), true)"
        ))
        .map(|_| ())
    }

    pub fn scroll_up(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy(0, {}), true)",
            -pixels
        ))
        .map(|_| ())
    }

    pub fn scroll_down(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy(0, {pixels}), true)"
        ))
        .map(|_| ())
    }

    pub fn scroll_left(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy({}, 0), true)",
            -pixels
        ))
        .map(|_| ())
    }

    pub fn scroll_right(&self, pixels: f64) -> OpenPageResult<()> {
        self.run_js(&format!(
            "(document.scrollingElement.scrollBy({pixels}, 0), true)"
        ))
        .map(|_| ())
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        value_as_f64_pair(
            self.run_js(
                "[document.documentElement.scrollLeft, document.documentElement.scrollTop]",
            )?,
            "frame scroll position",
        )
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js(
                "(() => { const rect = document.documentElement.getBoundingClientRect(); return [rect.left, rect.top]; })()",
            )?,
            "frame viewport location",
        )
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame_element.rect_screen_location()
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame_element.rect_size()
    }

    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame_element.rect_location()
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        value_as_optional_f64_pair(
            self.run_js(
                "(() => { \
                    const doc = document.documentElement; \
                    const body = document.body; \
                    const width = Math.max(doc ? doc.scrollWidth : 0, body ? body.scrollWidth : 0); \
                    const height = Math.max(doc ? doc.scrollHeight : 0, body ? body.scrollHeight : 0); \
                    return [width, height]; \
                })()",
            )?,
            "frame size",
        )
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        let Some((left, top)) = self.viewport_location()? else {
            return Ok(None);
        };
        let Some((width, height)) = self.viewport_size()? else {
            return Ok(None);
        };
        Ok(Some([
            (left, top),
            (left + width, top),
            (left + width, top + height),
            (left, top + height),
        ]))
    }

    pub fn corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        let Some((left, top)) = self.location()? else {
            return Ok(None);
        };
        let Some((width, height)) = self.size()? else {
            return Ok(None);
        };
        Ok(Some([
            (left, top),
            (left + width, top),
            (left + width, top + height),
            (left, top + height),
        ]))
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        value_as_optional_string(self.run_js("document.readyState")?, "frame ready state")
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        Ok(self.ready_state()?.as_deref() != Some("complete"))
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.frame_element.is_alive()
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        self.frame_element.is_displayed()
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        self.frame_element.is_enabled()
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        self.frame_element.has_rect()
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame_element.is_in_viewport()
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame_element.is_whole_in_viewport()
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        self.frame_element.is_covered()
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        self.frame_element.is_clickable()
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.page.has_alert()
    }

    pub fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            if self.ready_state()?.as_deref() == Some("complete") {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_until_displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_displayed(timeout_ms)
    }

    pub fn wait_until_hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_hidden(timeout_ms)
    }

    pub fn wait_until_enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_enabled(timeout_ms)
    }

    pub fn wait_until_disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_disabled(timeout_ms)
    }

    pub fn wait_until_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_deleted(timeout_ms)
    }

    pub fn wait_until_clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_clickable(timeout_ms)
    }

    pub fn wait_until_has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_has_rect(timeout_ms)
    }

    pub fn wait_until_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_covered(timeout_ms)
    }

    pub fn wait_until_not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element.wait_until_not_covered(timeout_ms)
    }

    pub fn wait_until_disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame_element
            .wait_until_disabled_or_deleted(timeout_ms)
    }

    pub fn wait_until_stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let Some(mut size) = self.frame_element.rect_size()? else {
            return Ok(false);
        };
        let Some(mut location) = self.frame_element.rect_location()? else {
            return Ok(false);
        };
        while Instant::now() < deadline {
            sleep(Duration::from_millis(100));
            let Some(next_size) = self.frame_element.rect_size()? else {
                return Ok(false);
            };
            let Some(next_location) = self.frame_element.rect_location()? else {
                return Ok(false);
            };
            if next_size == size && next_location == location {
                return Ok(true);
            }
            size = next_size;
            location = next_location;
        }
        Ok(false)
    }

    pub fn snapshot_root(&self) -> OpenPageResult<SessionElement> {
        snapshot_root(&self.inner_html()?)
    }

    pub fn snapshot_find<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        snapshot_find(&self.inner_html()?, locator.raw())
    }

    pub fn s_ele<'a, L>(&self, locator: L) -> OpenPageResult<SessionElement>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        self.snapshot_find(locator.raw())
    }

    pub fn snapshot_find_all<'a, L>(&self, locator: L) -> OpenPageResult<Vec<SessionElement>>
    where
        L: Into<LocatorInput<'a>>,
    {
        let locator = Locator::from_input(locator)?;
        snapshot_find_all(&self.inner_html()?, locator.raw())
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
        snapshot_query_xpath(&self.inner_html()?, expression)
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

    pub fn listener(&self) -> Listener {
        Listener::new_for_frame(
            Arc::clone(&self.page.runtime),
            self.page.inner.clone(),
            self.frame_id.clone(),
        )
    }

    pub fn listen(&self) -> Listener {
        self.listener()
    }

    pub fn console(&self) -> Console {
        self.page.console()
    }

    fn resolve_frame_target<'a>(&self, target: PageFrameTarget<'a>) -> OpenPageResult<Element> {
        match target {
            PageFrameTarget::Locator(locator) => {
                let locator = frame_locator_input(locator)?;
                self.find(locator.as_str())
            }
            PageFrameTarget::Index(index) => self.frame_element_by_index(index),
            PageFrameTarget::Element(element) => {
                find_frame_element_from_object(&self.page, element)
            }
            PageFrameTarget::WebElement(element) => match element {
                WebElement::Browser(element) | WebElement::Mix { element, .. } => {
                    find_frame_element_from_object(&self.page, element)
                }
                WebElement::Session(_) => Err(OpenPageError::UnsupportedOperation(
                    session_backed_element_driver_target_message(
                        "WebElement",
                        "frame frame",
                        "frame 元素定位",
                    ),
                )),
            },
            PageFrameTarget::Frame(frame) => {
                find_frame_element_from_object(&self.page, frame.frame_element())
            }
            PageFrameTarget::WebFrame(frame) => {
                find_frame_element_from_object(&self.page, frame.frame_element())
            }
            PageFrameTarget::OwnedFrame(frame) => {
                find_frame_element_from_object(&self.page, frame.frame_element())
            }
            PageFrameTarget::OwnedWebFrame(frame) => {
                find_frame_element_from_object(&self.page, frame.frame_element())
            }
        }
    }

    fn frame_element_by_index(&self, index: isize) -> OpenPageResult<Element> {
        if index == 0 {
            return Err(OpenPageError::ElementNotFound(
                frame_index_must_start_message(),
            ));
        }
        let frames = self.get_frame_eles(None::<&str>)?;
        let resolved_index = if index > 0 {
            (index as usize).checked_sub(1)
        } else {
            frames.len().checked_sub(index.unsigned_abs())
        };
        resolved_index
            .and_then(|resolved_index| frames.into_iter().nth(resolved_index))
            .ok_or_else(|| OpenPageError::ElementNotFound(frame_index_out_of_range_message(index)))
    }
}

impl FrameScroller<'_> {
    pub fn to_top(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_top()
    }

    pub fn to_bottom(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_bottom()
    }

    pub fn to_half(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_half()
    }

    pub fn to_rightmost(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_rightmost()
    }

    pub fn to_leftmost(&self) -> OpenPageResult<()> {
        self.frame.scroll_to_leftmost()
    }

    pub fn to_location(&self, x: f64, y: f64) -> OpenPageResult<()> {
        self.frame.scroll_to_location(x, y)
    }

    pub fn up(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame.scroll_up(pixels)
    }

    pub fn down(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame.scroll_down(pixels)
    }

    pub fn left(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame.scroll_left(pixels)
    }

    pub fn right(&self, pixels: f64) -> OpenPageResult<()> {
        self.frame.scroll_right(pixels)
    }

    pub fn to_see(&self, element: &Element, center: Option<bool>) -> OpenPageResult<()> {
        element.scroll_to_see(center)
    }

    pub fn to_center(&self, element: &Element) -> OpenPageResult<()> {
        element.scroll_to_center()
    }
}

impl FrameSetter<'_> {
    pub fn cookie(&self) -> FrameCookieSetter<'_> {
        FrameCookieSetter { frame: self.frame }
    }

    pub fn cookies<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.frame.set_cookies(cookies)
    }

    pub fn clear_cookies(&self) -> OpenPageResult<()> {
        self.frame.clear_cookies()
    }

    pub fn remove_cookie(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.frame.remove_cookie(name, url, domain, path)
    }

    pub fn attr(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame.set_attr(name, value)
    }

    pub fn property(&self, name: &str, value: &Value) -> OpenPageResult<()> {
        self.frame.set_property(name, value)
    }

    pub fn style(&self, name: &str, value: &str) -> OpenPageResult<()> {
        self.frame.set_style(name, value)
    }

    pub fn upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.frame.set_upload_files(files)
    }

    pub fn upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.frame.set_upload_paths(files)
    }

    pub fn download_path(&self, path: &str) -> OpenPageResult<()> {
        self.frame.set_download_path(path)
    }

    pub fn download_file_exists(&self, mode: DownloadFileExistsMode) -> OpenPageResult<()> {
        self.frame.set_download_file_exists_mode(mode)
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.frame.set_when_download_file_exists(mode)
    }

    pub fn download_file_name(
        &self,
        name: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        self.frame
            .set_download_filename(name, suffix, suffix_specified)
    }
}

impl FrameCookieSetter<'_> {
    pub fn set<'a, C>(&self, cookies: C) -> OpenPageResult<()>
    where
        C: Into<CookieInput<'a>>,
    {
        self.frame.set_cookies(cookies)
    }

    pub fn clear(&self) -> OpenPageResult<()> {
        self.frame.clear_cookies()
    }

    pub fn remove(
        &self,
        name: &str,
        url: Option<&str>,
        domain: Option<&str>,
        path: Option<&str>,
    ) -> OpenPageResult<()> {
        self.frame.remove_cookie(name, url, domain, path)
    }
}

impl FrameStates<'_> {
    pub fn is_loading(&self) -> OpenPageResult<bool> {
        self.frame.is_loading()
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.frame.is_alive()
    }

    pub fn ready_state(&self) -> OpenPageResult<Option<String>> {
        self.frame.ready_state()
    }

    pub fn is_displayed(&self) -> OpenPageResult<bool> {
        self.frame.is_displayed()
    }

    pub fn is_enabled(&self) -> OpenPageResult<bool> {
        self.frame.is_enabled()
    }

    pub fn has_rect(&self) -> OpenPageResult<bool> {
        self.frame.has_rect()
    }

    pub fn is_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame.is_in_viewport()
    }

    pub fn is_whole_in_viewport(&self) -> OpenPageResult<bool> {
        self.frame.is_whole_in_viewport()
    }

    pub fn is_covered(&self) -> OpenPageResult<bool> {
        self.frame.is_covered()
    }

    pub fn is_clickable(&self) -> OpenPageResult<bool> {
        self.frame.is_clickable()
    }

    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.frame.has_alert()
    }
}

impl FrameWait<'_> {
    pub fn doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_for_doc_loaded(timeout_ms)
    }

    pub fn download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        self.frame.wait_for_download_begin(timeout_ms, cancel_it)
    }

    pub fn downloads_done(&self, timeout_ms: u64, cancel_if_timeout: bool) -> OpenPageResult<bool> {
        self.frame
            .wait_for_downloads_done(timeout_ms, cancel_if_timeout)
    }

    pub fn displayed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_displayed(timeout_ms)
    }

    pub fn hidden(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_hidden(timeout_ms)
    }

    pub fn enabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_enabled(timeout_ms)
    }

    pub fn disabled(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_disabled(timeout_ms)
    }

    pub fn deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_deleted(timeout_ms)
    }

    pub fn clickable(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_clickable(timeout_ms)
    }

    pub fn has_rect(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_has_rect(timeout_ms)
    }

    pub fn covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_covered(timeout_ms)
    }

    pub fn not_covered(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_not_covered(timeout_ms)
    }

    pub fn disabled_or_deleted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_disabled_or_deleted(timeout_ms)
    }

    pub fn stop_moving(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_until_stop_moving(timeout_ms)
    }

    pub fn upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.frame.wait_for_upload_paths_inputted(timeout_ms)
    }
}

impl FrameRect<'_> {
    pub fn location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.location()
    }

    pub fn viewport_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.viewport_location()
    }

    pub fn screen_location(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.screen_location()
    }

    pub fn size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.size()
    }

    pub fn viewport_size(&self) -> OpenPageResult<Option<(f64, f64)>> {
        self.frame.viewport_size()
    }

    pub fn corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        self.frame.corners()
    }

    pub fn viewport_corners(&self) -> OpenPageResult<Option<[(f64, f64); 4]>> {
        self.frame.viewport_corners()
    }

    pub fn scroll_position(&self) -> OpenPageResult<(f64, f64)> {
        self.frame.scroll_position()
    }
}
