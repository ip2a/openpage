use super::*;

impl Browser {
    pub fn reconnect(&self) -> OpenPageResult<Self> {
        let mut browser = Browser::connect(&format!("http://{}", self.address()))?;
        browser.set_timeouts(self.timeouts()?)?;
        browser.set_retry(
            Some(self.retry_times()?),
            Some(self.retry_interval_millis()?),
        )?;
        browser.set_load_mode(self.load_mode_value()?)?;
        browser.apply_browser_download_settings(&self.snapshot_browser_download_settings()?)?;

        let page_download_settings = self
            .inner
            .page_download_settings
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        let mission_download_settings = self
            .inner
            .mission_download_settings
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| mission_download_settings_lock_poisoned_error())?;
        let isolated_contexts = self
            .inner
            .isolated_contexts
            .lock()
            .map(|contexts| contexts.clone())
            .map_err(|_| isolated_context_lock_poisoned_error())?;

        if let Some(state) = Arc::get_mut(&mut browser.inner) {
            state.browser_pid = self.inner.browser_pid;
            state.headless = self.inner.headless;
            state.temp_user_data_dir = self.inner.temp_user_data_dir.clone();
            state.temp_download_dir = self.inner.temp_download_dir.clone();
            *state
                .page_download_settings
                .get_mut()
                .map_err(|_| page_download_settings_lock_poisoned_error())? =
                page_download_settings;
            *state
                .mission_download_settings
                .get_mut()
                .map_err(|_| mission_download_settings_lock_poisoned_error())? =
                mission_download_settings;
            *state
                .isolated_contexts
                .get_mut()
                .map_err(|_| isolated_context_lock_poisoned_error())? = isolated_contexts;
        }

        Ok(browser)
    }

    pub fn close(&self) -> OpenPageResult<()> {
        self.inner.runtime.block_on(async {
            let mut browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::close().lock()").await?;
            run_browser_future_with_cdp_timeout(browser.close(), "Browser::close()").await?;
            run_browser_future_with_cdp_timeout(browser.wait(), "Browser::close().wait()").await?;
            Ok::<(), OpenPageError>(())
        })?;
        if let Ok(mut cache) = self.inner.page_cache.lock() {
            cache.clear();
        }
        if let Ok(mut contexts) = self.inner.isolated_contexts.lock() {
            contexts.clear();
        }
        if let Some(path) = &self.inner.temp_user_data_dir {
            let _ = std::fs::remove_dir_all(path);
        }
        if let Some(path) = &self.inner.temp_download_dir {
            let _ = std::fs::remove_dir_all(path);
        }
        Ok(())
    }

    pub(crate) fn download_info(&self, guid: &str) -> OpenPageResult<DownloadInfo> {
        self.inner.downloads.info(guid)
    }

    pub(crate) fn download_folder(&self, guid: &str) -> OpenPageResult<String> {
        let info = self.download_info(guid)?;
        let settings = self.resolve_download_settings(&info)?;
        let path = settings.path;
        Ok(path.to_string_lossy().into_owned())
    }

    pub(crate) fn download_tab_id(&self, guid: &str) -> OpenPageResult<String> {
        let info = self.download_info(guid)?;
        self.resolve_frame_target_id(&info.frame_id)
    }

    pub(crate) fn download_tmp_path(&self, guid: &str) -> OpenPageResult<String> {
        Ok(self
            .inner
            .download_spool_dir
            .join(guid)
            .to_string_lossy()
            .into_owned())
    }

    pub(crate) fn wait_for_download_guid(
        &self,
        guid: &str,
        timeout_ms: Option<u64>,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<Option<String>> {
        let info = match timeout_ms {
            Some(timeout_ms) => match self.inner.downloads.wait_for_guid(guid, timeout_ms) {
                Ok(info) => info,
                Err(OpenPageError::Timeout(_)) => {
                    if let Ok(info) = self.download_info(guid) {
                        if info.state != DownloadState::Running {
                            return self.finish_waited_download(&info);
                        }
                    }
                    if cancel_if_timeout {
                        let _ = self.cancel_download(guid);
                    }
                    return Ok(None);
                }
                Err(err) => return Err(err),
            },
            None => self.inner.downloads.wait_for_guid_forever(guid)?,
        };
        self.finish_waited_download(&info)
    }

    pub(super) fn finalize_download(
        &self,
        info: &DownloadInfo,
        filename: Option<&str>,
    ) -> OpenPageResult<String> {
        if info.state == DownloadState::Canceled {
            return Err(OpenPageError::BrowserOperation(download_canceled_message(
                &info.guid,
            )));
        }

        if info.state == DownloadState::Skipped {
            return info.final_path.clone().ok_or_else(|| {
                OpenPageError::BrowserOperation(download_skipped_without_final_path_message(
                    &info.guid,
                ))
            });
        }

        let source_path = download_source_path(info, &self.inner.download_spool_dir)?;
        let settings = self.resolve_download_settings(info)?;
        let download_dir = settings.path;
        let mode = settings.file_exists;
        let rename = settings.rename;
        let suffix = settings.suffix;
        let preferred_name = filename.map(str::to_string).unwrap_or_else(|| {
            resolved_download_name(
                &info.suggested_filename,
                rename.as_deref(),
                suffix.as_ref().map(|value| value.as_deref()),
            )
        });
        let preferred_path = download_dir.join(&preferred_name);
        let (state, final_path) = finalize_download_path(&source_path, &preferred_path, mode)?;

        self.inner
            .downloads
            .set_finalized(&info.guid, state, final_path.clone())?;
        Ok(final_path)
    }

    pub(super) fn finish_waited_download(
        &self,
        info: &DownloadInfo,
    ) -> OpenPageResult<Option<String>> {
        match info.state {
            DownloadState::Canceled => Ok(None),
            _ => self.finalize_download(info, None).map(Some),
        }
    }

    pub(super) fn capture_mission_download_settings(
        &self,
        info: &DownloadInfo,
    ) -> OpenPageResult<()> {
        let settings = self.resolve_download_settings(info)?;
        let mut mission_settings = self
            .inner
            .mission_download_settings
            .lock()
            .map_err(|_| mission_download_settings_lock_poisoned_error())?;
        mission_settings.insert(info.guid.clone(), settings);
        Ok(())
    }

    pub(super) fn resolve_page_frame_id(&self, target_id: &str) -> OpenPageResult<String> {
        self.get_page(target_id)?.main_frame_id()
    }

    pub(super) fn resolve_frame_target_id(&self, frame_id: &str) -> OpenPageResult<String> {
        for target_id in self.tab_ids()? {
            let page = self.get_page(&target_id)?;
            if page
                .download_scope_frame_ids()?
                .iter()
                .any(|current| current == frame_id)
            {
                return Ok(target_id);
            }
        }

        Err(OpenPageError::BrowserOperation(
            download_frame_not_mapped_to_tab_message(frame_id),
        ))
    }

    pub(super) fn load_mode_value(&self) -> OpenPageResult<LoadMode> {
        self.inner
            .load_mode
            .lock()
            .map(|mode| *mode)
            .map_err(|_| browser_load_mode_lock_poisoned_error())
    }

    pub(super) fn resolve_download_settings(
        &self,
        info: &DownloadInfo,
    ) -> OpenPageResult<ResolvedDownloadSettings> {
        if let Some(settings) = self
            .inner
            .mission_download_settings
            .lock()
            .map_err(|_| mission_download_settings_lock_poisoned_error())?
            .get(&info.guid)
            .cloned()
        {
            return Ok(settings);
        }

        let page_settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        let page_settings = page_settings.get(&info.frame_id);
        let path = if let Some(path) = page_settings.and_then(|settings| settings.path.clone()) {
            path
        } else {
            self.inner
                .download_path
                .lock()
                .map_err(|_| browser_download_path_lock_poisoned_error())?
                .clone()
                .ok_or_else(|| {
                    OpenPageError::UnsupportedOperation(download_path_not_configured_message())
                })?
        };
        let mode = if let Some(mode) = page_settings.and_then(|settings| settings.file_exists) {
            mode
        } else {
            *self
                .inner
                .download_file_exists
                .lock()
                .map_err(|_| browser_download_file_exists_lock_poisoned_error())?
        };
        let rename = page_settings.and_then(|settings| settings.rename.clone());
        let browser_naming = self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| browser_download_naming_lock_poisoned_error())?;
        let rename = rename.or_else(|| browser_naming.rename.clone());
        let suffix = page_settings
            .and_then(|settings| settings.suffix.clone())
            .or_else(|| browser_naming.suffix.clone());
        Ok(ResolvedDownloadSettings {
            path,
            file_exists: mode,
            rename,
            suffix,
        })
    }
}
