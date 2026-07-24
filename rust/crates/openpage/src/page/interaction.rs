use super::*;

impl Page {
    pub fn actions(&self) -> OpenPageResult<Actions> {
        let _ = self.wait_for_doc_loaded(self.navigation_page_load_timeout_ms()?);
        Ok(Actions::new(self.clone()))
    }

    pub fn new_actions(&self) -> Actions {
        Actions::new(self.clone())
    }

    pub fn click<'a, L>(&self, locator: L) -> OpenPageResult<()>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.click_with_timeout(locator, self.implicit_wait_timeout_ms()?)
    }

    pub fn click_with_timeout<'a, L>(&self, locator: L, timeout_ms: u64) -> OpenPageResult<()>
    where
        L: Into<LocatorInput<'a>>,
    {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        self.wait_for(locator, timeout_ms)?
            .click_with_timeout(remaining_timeout_ms(deadline))
    }

    pub fn fill<'a, L>(&self, locator: L, text: &str) -> OpenPageResult<()>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.fill_with_timeout(locator, text, self.implicit_wait_timeout_ms()?)
    }

    pub fn fill_with_timeout<'a, L>(
        &self,
        locator: L,
        text: &str,
        timeout_ms: u64,
    ) -> OpenPageResult<()>
    where
        L: Into<LocatorInput<'a>>,
    {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        self.wait_for(locator, timeout_ms)?
            .input_with_timeout(text, remaining_timeout_ms(deadline))
    }

    pub fn text<'a, L>(&self, locator: L) -> OpenPageResult<Option<String>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .text()
    }

    pub fn attr<'a, L>(&self, locator: L, name: &str) -> OpenPageResult<Option<String>>
    where
        L: Into<LocatorInput<'a>>,
    {
        self.wait_for(locator, self.implicit_wait_timeout_ms()?)?
            .attr(name)
    }

    pub fn active_element(&self) -> OpenPageResult<Option<Element>> {
        let marker = next_page_marker();
        let script = format!(
            "(() => {{ \
                const active = document.activeElement; \
                if (!active) return null; \
                active.setAttribute({attr}, {marker}); \
                return true; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            marker = json_string(&marker)?,
        );
        let result = self.run_js(&script)?;
        if result.is_null() {
            return Ok(None);
        }
        let selector = marker_selector(&marker);
        let element = self.find(&selector)?;
        self.clear_page_markers(&[marker.as_str()])?;
        Ok(Some(element))
    }

    pub fn remove_element<'a, L>(&self, target: L) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        match target.into() {
            PageElementTarget::Locator(locator) => {
                match self.find(Locator::from_input(locator)?.raw()) {
                    Ok(element) => {
                        element.run_js("this.remove(); return true;")?;
                        Ok(true)
                    }
                    Err(OpenPageError::ElementNotFound(_)) => Ok(false),
                    Err(err) => Err(err),
                }
            }
            target => {
                resolve_page_element_target(self, target)?
                    .element()
                    .run_js("this.remove(); return true;")?;
                Ok(true)
            }
        }
    }

    pub fn remove_ele<'a, L>(&self, target: L) -> OpenPageResult<bool>
    where
        L: Into<PageElementTarget<'a>>,
    {
        self.remove_element(target)
    }

    pub fn add_element_html<'a, 'b, I, B>(
        &self,
        html: &str,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<Element>
    where
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        let insert_to = insert_to
            .map(|target| resolve_page_element_target(self, target.into()))
            .transpose()?;
        let before = before
            .map(|target| resolve_page_element_target(self, target.into()))
            .transpose()?;
        let inserted_marker = next_page_marker();
        let parent_marker = insert_to.as_ref().map(|_| next_page_marker());
        let before_marker = before.as_ref().map(|_| next_page_marker());

        if let (Some(target), Some(marker)) = (insert_to.as_ref(), parent_marker.as_deref()) {
            target.set_attr(PAGE_MARKER_ATTRIBUTE, marker)?;
        }
        if let (Some(target), Some(marker)) = (before.as_ref(), before_marker.as_deref()) {
            target.set_attr(PAGE_MARKER_ATTRIBUTE, marker)?;
        }

        let script = format!(
            "(() => {{ \
                const markerAttr = {attr}; \
                const insertMarker = {insert_marker}; \
                const parent = {parent}; \
                const before = {before}; \
                const template = document.createElement('template'); \
                template.innerHTML = {html}; \
                const element = template.content.firstElementChild; \
                if (!element) return null; \
                element.setAttribute(markerAttr, insertMarker); \
                if (before && before.parentNode) {{ \
                    before.parentNode.insertBefore(element, before); \
                }} else {{ \
                    (parent || document.body || document.documentElement).appendChild(element); \
                }} \
                return true; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            insert_marker = json_string(&inserted_marker)?,
            parent = page_marker_lookup_expression(parent_marker.as_deref())?,
            before = page_marker_lookup_expression(before_marker.as_deref())?,
            html = json_string(html)?,
        );
        self.run_js(&script)?;

        let selector = marker_selector(&inserted_marker);
        let element = self.find(&selector)?;
        self.clear_page_markers(&[inserted_marker.as_str()])?;
        if let Some(target) = insert_to.as_ref() {
            target.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        }
        if let Some(target) = before.as_ref() {
            target.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        }
        Ok(element)
    }

    pub fn add_element<'a, 'b, 'c, C, I, B>(
        &self,
        content: C,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<Element>
    where
        C: Into<PageElementContent<'c>>,
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        match content.into() {
            PageElementContent::Html(html) => {
                self.add_element_html(html.as_ref(), insert_to, before)
            }
            PageElementContent::Info(info) => self.add_element_info(info, insert_to, before),
        }
    }

    pub fn add_ele<'a, 'b, 'c, C, I, B>(
        &self,
        content: C,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<Element>
    where
        C: Into<PageElementContent<'c>>,
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
    {
        self.add_element(content, insert_to, before)
    }

    pub fn add_element_info<'a, 'b, I, B, H>(
        &self,
        info: H,
        insert_to: Option<I>,
        before: Option<B>,
    ) -> OpenPageResult<Element>
    where
        I: Into<PageElementTarget<'a>>,
        B: Into<PageElementTarget<'b>>,
        H: Into<PageElementInfo>,
    {
        let info = info.into();
        let insert_to = insert_to
            .map(|target| resolve_page_element_target(self, target.into()))
            .transpose()?;
        let before = before
            .map(|target| resolve_page_element_target(self, target.into()))
            .transpose()?;
        let inserted_marker = next_page_marker();
        let parent_marker = insert_to.as_ref().map(|_| next_page_marker());
        let before_marker = before.as_ref().map(|_| next_page_marker());
        let detached_after_lookup = insert_to.is_none() && before.is_none();

        if let (Some(target), Some(marker)) = (insert_to.as_ref(), parent_marker.as_deref()) {
            target.set_attr(PAGE_MARKER_ATTRIBUTE, marker)?;
        }
        if let (Some(target), Some(marker)) = (before.as_ref(), before_marker.as_deref()) {
            target.set_attr(PAGE_MARKER_ATTRIBUTE, marker)?;
        }

        let script = format!(
            "(() => {{ \
                const markerAttr = {attr}; \
                const insertMarker = {insert_marker}; \
                const parent = {parent}; \
                const before = {before}; \
                const data = {data}; \
                const element = document.createElement({tag}); \
                for (const [name, value] of Object.entries(data)) {{ \
                    if (value === null || value === undefined) continue; \
                    if (name === 'innerHTML' || name === 'innerText' || name === 'textContent') {{ \
                        element[name] = String(value); \
                        continue; \
                    }} \
                    if (name in element) {{ \
                        try {{ element[name] = value; }} catch (_) {{}} \
                    }} \
                    try {{ element.setAttribute(name, String(value)); }} catch (_) {{}} \
                }} \
                element.setAttribute(markerAttr, insertMarker); \
                if (before && before.parentNode) {{ \
                    before.parentNode.insertBefore(element, before); \
                }} else if (parent) {{ \
                    parent.appendChild(element); \
                }} else {{ \
                    const root = document.body || document.documentElement; \
                    if (!root) return null; \
                    root.appendChild(element); \
                }} \
                return true; \
            }})()",
            attr = json_string(PAGE_MARKER_ATTRIBUTE)?,
            insert_marker = json_string(&inserted_marker)?,
            parent = page_marker_lookup_expression(parent_marker.as_deref())?,
            before = page_marker_lookup_expression(before_marker.as_deref())?,
            data = page_element_info_properties_json(&info)?,
            tag = json_string(info.tag())?,
        );
        self.run_js(&script)?;

        let selector = marker_selector(&inserted_marker);
        let element = self.find(&selector)?;
        if detached_after_lookup {
            element.run_js("this.remove(); return true;")?;
        }
        element.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        if let Some(target) = insert_to.as_ref() {
            target.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        }
        if let Some(target) = before.as_ref() {
            target.remove_attr(PAGE_MARKER_ATTRIBUTE)?;
        }
        Ok(element)
    }

    pub fn scroll(&self) -> PageScroller<'_> {
        PageScroller { page: self }
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

    pub fn set_upload_files<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.uploader.set_files(files)
    }

    pub fn set_upload_paths<'a, F>(&self, files: F) -> OpenPageResult<()>
    where
        F: Into<UploadFilesInput<'a>>,
    {
        self.set_upload_files(files)
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
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "click_to_download()",
            ))
        })?;
        let target_id = self.target_id();
        let previous_settings = browser.snapshot_page_download_settings(&target_id)?;
        let previous_browser_settings = browser.snapshot_browser_download_settings()?;
        let timeout_ms = timeout_ms.unwrap_or(browser.timeouts()?.implicit_wait);
        let download_started_before = browser.download_started_len()?;
        let action_result = (|| {
            if new_tab {
                let mut temp_settings = browser.snapshot_browser_download_settings()?;
                if let Some(path) = save_path {
                    temp_settings.path = Some(PathBuf::from(path));
                } else if let Some(path) = previous_settings
                    .as_ref()
                    .and_then(|settings| settings.path.clone())
                {
                    temp_settings.path = Some(path);
                } else if temp_settings.path.is_none() {
                    temp_settings.path = Some(PathBuf::from("."));
                }
                if let Some(mode) = previous_settings
                    .as_ref()
                    .and_then(|settings| settings.file_exists)
                {
                    temp_settings.file_exists = mode;
                }
                if rename.is_some() || suffix_specified {
                    temp_settings.rename = rename.map(str::to_string);
                    temp_settings.suffix = if suffix_specified {
                        Some(suffix.map(str::to_string))
                    } else {
                        None
                    };
                } else if let Some(settings) = previous_settings.as_ref() {
                    temp_settings.rename = settings.rename.clone();
                    temp_settings.suffix = settings.suffix.clone();
                }
                browser.apply_browser_download_settings(&temp_settings)?;
            } else {
                if let Some(path) = save_path {
                    self.set_tab_download_path(path)?;
                } else if self.download_path()?.is_none() {
                    self.set_tab_download_path(".")?;
                }
                if rename.is_some() || suffix_specified {
                    self.set_tab_download_filename(rename, suffix, suffix_specified)?;
                }
            }
            let element = self.wait_for(locator, timeout_ms)?;
            if !element.click_with_options(Some(by_js), Some(timeout_ms), true)? {
                return Ok(None);
            }
            if new_tab {
                browser.wait_for_download_begin_after(download_started_before, timeout_ms, false)
            } else {
                browser.wait_for_download_begin_after_in_frames(
                    download_started_before,
                    &self.download_scope_frame_ids()?,
                    timeout_ms,
                    false,
                )
            }
        })();
        let restore_result = browser.restore_page_download_settings(&target_id, previous_settings);
        let browser_restore_result =
            browser.restore_browser_download_settings(previous_browser_settings);
        match (action_result, restore_result, browser_restore_result) {
            (Ok(result), Ok(()), Ok(())) => Ok(result),
            (Err(err), _, _) => Err(err),
            (Ok(_), Err(err), _) => Err(err),
            (Ok(_), Ok(()), Err(err)) => Err(err),
        }
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
        let timeout_ms = match timeout_ms {
            Some(timeout_ms) => timeout_ms,
            None => self.implicit_wait_timeout_ms()?,
        };
        self.set_upload_files(files)?;
        let element = self.wait_for(locator, timeout_ms)?;
        if !element.click_with_options(Some(by_js), Some(timeout_ms), true)? {
            return Ok(false);
        }
        self.wait_for_upload_paths_inputted(timeout_ms)
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
        let browser = self.browser.as_ref().ok_or_else(|| {
            OpenPageError::UnsupportedOperation(browser_backed_page_only_message(
                "click_for_new_tab()",
            ))
        })?;
        let timeout_ms = timeout_ms.unwrap_or(browser.timeouts()?.implicit_wait);
        let current_tab_id = self.target_id();
        browser.activate_tab(&current_tab_id)?;
        let element = self.wait_for(locator, timeout_ms)?;
        let baseline = browser.tab_ids()?;
        let _ = element.click_with_options(Some(by_js), Some(timeout_ms), true)?;
        let Some(target_id) =
            browser.wait_for_new_tab_from(&baseline, Some(&current_tab_id), timeout_ms)?
        else {
            return Err(OpenPageError::PageOperation(no_new_tab_message()));
        };
        let page = browser.wait_for_page(&target_id, timeout_ms)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        while page.url()? == "about:blank" && Instant::now() < deadline {
            sleep(Duration::from_millis(25));
        }
        page.wait_for_doc_loaded(remaining_timeout_ms(deadline))?;
        Ok(Some(page))
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
        if get_tab && self.browser.is_none() {
            return Err(OpenPageError::UnsupportedOperation(
                browser_backed_page_only_message("click_middle(get_tab=True)"),
            ));
        }
        let timeout_ms = match timeout_ms {
            Some(timeout_ms) => timeout_ms,
            None => self.implicit_wait_timeout_ms()?,
        };
        let element = self.wait_for(locator, timeout_ms)?;
        let browser = self.browser.as_ref();
        let current_tab_id = browser.map(|_| self.target_id());
        if get_tab && let Some(browser) = browser {
            browser.activate_tab(self.target_id().as_str())?;
        }
        let baseline = browser.map(Browser::tab_ids).transpose()?;
        element.click_middle()?;
        let detect_timeout_ms = if get_tab {
            timeout_ms
        } else {
            timeout_ms.min(500)
        };
        if let Some(browser) = browser {
            if let Some(target_id) = browser.wait_for_new_tab_from(
                baseline.as_deref().unwrap_or(&[]),
                current_tab_id.as_deref(),
                detect_timeout_ms,
            )? {
                if get_tab {
                    let page = browser.wait_for_page(&target_id, detect_timeout_ms)?;
                    let deadline = Instant::now() + Duration::from_millis(detect_timeout_ms.max(1));
                    while page.url()? == "about:blank" && Instant::now() < deadline {
                        sleep(Duration::from_millis(25));
                    }
                    page.wait_for_doc_loaded(remaining_timeout_ms(deadline))?;
                    return Ok(Some(page));
                }
                return Ok(None);
            }
        }
        if get_tab {
            return Err(OpenPageError::PageOperation(no_new_tab_message()));
        }
        Ok(None)
    }

    pub fn wait_for_upload_paths_inputted(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.uploader.wait_until_inputted(timeout_ms)
    }
}
