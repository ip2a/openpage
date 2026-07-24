use super::*;

impl Page {
    pub fn navigation_snapshot(&self) -> OpenPageResult<PageNavigationSnapshot> {
        self.navigation.snapshot()
    }

    pub fn goto(&self, url: &str) -> OpenPageResult<()> {
        let requested_url = url.to_string();
        let timeout_ms = self.navigation_page_load_timeout_ms().ok();
        (|| {
            let (retry_times, retry_interval_millis) = self.navigation_retry_config()?;
            let load_mode = self.load_mode_value()?;
            let url = normalize_navigation_target(url)?;
            let mut last_err = None;

            for attempt in 0..=retry_times {
                match self.goto_once(&url, load_mode) {
                    Ok(()) => return Ok(()),
                    Err(err) => {
                        last_err = Some(err);
                        if attempt == retry_times {
                            break;
                        }
                    }
                }

                if retry_interval_millis > 0 {
                    sleep(Duration::from_millis(retry_interval_millis));
                }
            }

            Err(last_err
                .unwrap_or_else(|| OpenPageError::Timeout(page_connect_timed_out_message(&url))))
        })()
        .map_err(|error| {
            let failure_reason = error.to_string();
            error.diagnosed(ErrorDiagnostic {
                operation: Some("goto".to_string()),
                url: Some(requested_url),
                timeout_ms,
                failure_reason: Some(failure_reason),
                ..ErrorDiagnostic::default()
            })
        })
    }

    fn goto_once(&self, url: &str, load_mode: LoadMode) -> OpenPageResult<()> {
        let supports_script_navigation = url.starts_with("http://") || url.starts_with("https://");
        let page_load_timeout_ms = self.navigation_page_load_timeout_ms()?;
        let deadline = Instant::now() + Duration::from_millis(page_load_timeout_ms.max(1));

        match load_mode {
            LoadMode::Normal => {
                self.navigate_via_cdp(&url)?;
                if self.wait_for_doc_loaded(page_load_timeout_ms)?
                    && self.wait_for_dom_ready(remaining_timeout_ms(deadline))?
                {
                    Ok(())
                } else {
                    Err(OpenPageError::Timeout(page_connect_timed_out_message(url)))
                }
            }
            LoadMode::Eager if supports_script_navigation => {
                self.navigate_via_script(&url)?;
                if self.wait_for_ready_state_change(page_load_timeout_ms, true)? {
                    let _ = self.stop_loading();
                    if self.wait_for_dom_ready(remaining_timeout_ms(deadline))? {
                        Ok(())
                    } else {
                        Err(OpenPageError::Timeout(page_connect_timed_out_message(url)))
                    }
                } else {
                    Err(OpenPageError::Timeout(page_connect_timed_out_message(url)))
                }
            }
            LoadMode::None if supports_script_navigation => {
                self.navigate_via_script(&url)?;
                Ok(())
            }
            LoadMode::Eager => {
                self.navigate_via_cdp(&url)?;
                if self.wait_for_ready_state_change(page_load_timeout_ms, true)? {
                    Ok(())
                } else {
                    Err(OpenPageError::Timeout(page_connect_timed_out_message(url)))
                }
            }
            LoadMode::None => {
                self.navigate_via_cdp(&url)?;
                Ok(())
            }
        }
    }

    fn navigation_retry_config(&self) -> OpenPageResult<(usize, u64)> {
        match &self.browser {
            Some(browser) => Ok((browser.retry_times()?, browser.retry_interval_millis()?)),
            None => Ok((0, 0)),
        }
    }

    pub(super) fn navigation_page_load_timeout_ms(&self) -> OpenPageResult<u64> {
        match &self.browser {
            Some(browser) => Ok(browser.timeouts()?.page_load),
            None => Ok(DEFAULT_PAGE_LOAD_TIMEOUT_MS),
        }
    }

    pub fn refresh(&self, ignore_cache: bool) -> OpenPageResult<()> {
        let params = ReloadParams::builder().ignore_cache(ignore_cache).build();
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            params,
            "Page::refresh()",
        )?;
        Ok(())
    }

    pub fn back(&self, steps: usize) -> OpenPageResult<bool> {
        self.navigate_history(-(steps as isize))
    }

    pub fn forward(&self, steps: usize) -> OpenPageResult<bool> {
        self.navigate_history(steps as isize)
    }

    pub fn ready_state(&self) -> OpenPageResult<String> {
        match self.evaluate("document.readyState")? {
            Value::String(value) => Ok(value),
            value => Err(OpenPageError::JavaScript(value_did_not_return_message(
                "document.readyState",
                "a string",
                "字符串",
                &value.to_string(),
            ))),
        }
    }

    pub fn is_loading(&self) -> OpenPageResult<bool> {
        Ok(self.ready_state()? != "complete")
    }

    pub fn wait_for_url_change(
        &self,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        self.wait_for_change(timeout_ms, |page| {
            let value = page.url()?;
            Ok(if exclude {
                !value.contains(text)
            } else {
                value.contains(text)
            })
        })
    }

    pub fn wait_for_title_change(
        &self,
        text: &str,
        exclude: bool,
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        self.wait_for_change(timeout_ms, |page| {
            let value = page.title()?;
            Ok(if exclude {
                !value.contains(text)
            } else {
                value.contains(text)
            })
        })
    }

    pub fn wait_for_load_start(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.ready_state() {
                Ok(state) if state != "complete" => return Ok(true),
                Ok(_) => {}
                Err(_) => return Ok(true),
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("Page::wait_for_load_start()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn wait_for_doc_loaded(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        let deadline = Instant::now() + timeout;
        loop {
            match self.ready_state() {
                Ok(state) if state == "complete" => return Ok(true),
                Ok(_) => {}
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return wait_timeout_result("Page::wait_for_doc_loaded()", timeout_ms);
            }
            sleep(Duration::from_millis(50));
        }
    }

    pub fn stop_loading(&self) -> OpenPageResult<()> {
        execute_page_command_blocking(
            self.runtime.as_ref(),
            &self.inner,
            StopLoadingParams::default(),
            "Page::stop_loading()",
        )?;
        Ok(())
    }
}
