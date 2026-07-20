use super::*;

impl Browser {
    pub fn wait_for_new_tab(
        &self,
        current_tab_id: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        let initial_baseline = self.tab_ids()?;
        // CDP can report a newly created background target a few milliseconds after
        // new_page() returns; stabilize the baseline before waiting for the next one.
        sleep(Duration::from_millis(50));
        let baseline = self.tab_ids()?;
        if let Some(current_tab_id) = current_tab_id
            .map(str::trim)
            .filter(|target_id| !target_id.is_empty())
        {
            let newest = resolve_newest_tab_id(&baseline, self.tracked_newest_tab_id()?);
            if newest.as_deref().is_some_and(|target_id| {
                target_id != current_tab_id
                    && !initial_baseline.iter().any(|seen| seen == target_id)
            }) {
                return Ok(newest);
            }
        }
        self.wait_for_new_tab_from(&baseline, current_tab_id, timeout_ms)
    }

    pub(crate) fn wait_for_new_tab_from(
        &self,
        baseline: &[String],
        current_tab_id: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        let tracked_baseline = self.tracked_newest_tab_id()?;
        let baseline_marker = current_tab_id
            .map(str::trim)
            .filter(|target_id| !target_id.is_empty())
            .map(str::to_string)
            .or_else(|| resolve_newest_tab_id(&baseline, tracked_baseline.clone()));
        let baseline_newest = resolve_newest_tab_id(&baseline, tracked_baseline);
        if let Some(new_id) = find_new_tab_id(
            &baseline,
            &baseline,
            baseline_marker.as_deref(),
            baseline_newest.as_deref(),
        ) {
            return Ok(Some(new_id));
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        loop {
            let current = self.tab_ids()?;
            let current_newest = resolve_newest_tab_id(&current, self.tracked_newest_tab_id()?);
            if let Some(new_id) = find_new_tab_id(
                &baseline,
                &current,
                baseline_marker.as_deref(),
                current_newest.as_deref(),
            ) {
                return Ok(Some(new_id));
            }
            if Instant::now() >= deadline {
                break;
            }
            sleep(Duration::from_millis(50));
        }
        if wait_failed_should_raise() {
            return Err(timeout_error("Browser::wait_for_new_tab()", timeout_ms));
        }
        Ok(None)
    }

    pub fn download_path(&self) -> OpenPageResult<Option<String>> {
        self.inner
            .download_path
            .lock()
            .map(|path| {
                path.as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .map_err(|_| browser_download_path_lock_poisoned_error())
    }

    pub fn set_download_path(&self, path: impl AsRef<Path>) -> OpenPageResult<()> {
        let path = path.as_ref().to_path_buf();
        create_download_directory(&path)?;

        self.inner
            .download_path
            .lock()
            .map_err(|_| browser_download_path_lock_poisoned_error())?
            .replace(path);
        Ok(())
    }

    pub fn download_file_exists_mode(&self) -> OpenPageResult<String> {
        self.inner
            .download_file_exists
            .lock()
            .map(|mode| mode.as_str().to_string())
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())
    }

    pub fn set_download_file_exists_mode(
        &self,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        *self
            .inner
            .download_file_exists
            .lock()
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())? = mode;
        Ok(())
    }

    pub(crate) fn snapshot_browser_download_settings(
        &self,
    ) -> OpenPageResult<BrowserDownloadSettingsSnapshot> {
        let path = self
            .inner
            .download_path
            .lock()
            .map_err(|_| browser_download_path_lock_poisoned_error())?
            .clone();
        let file_exists = *self
            .inner
            .download_file_exists
            .lock()
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())?;
        let naming = self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| browser_download_naming_lock_poisoned_error())?
            .clone();
        Ok(BrowserDownloadSettingsSnapshot {
            path,
            file_exists,
            rename: naming.rename,
            suffix: naming.suffix,
        })
    }

    pub(crate) fn restore_browser_download_settings(
        &self,
        settings: BrowserDownloadSettingsSnapshot,
    ) -> OpenPageResult<()> {
        *self
            .inner
            .download_path
            .lock()
            .map_err(|_| browser_download_path_lock_poisoned_error())? = settings.path;
        *self
            .inner
            .download_file_exists
            .lock()
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())? =
            settings.file_exists;
        *self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| browser_download_naming_lock_poisoned_error())? = BrowserDownloadNaming {
            rename: settings.rename,
            suffix: settings.suffix,
        };
        Ok(())
    }

    pub(crate) fn apply_browser_download_settings(
        &self,
        settings: &BrowserDownloadSettingsSnapshot,
    ) -> OpenPageResult<()> {
        if let Some(path) = &settings.path {
            create_download_directory(path)?;
        }
        *self
            .inner
            .download_path
            .lock()
            .map_err(|_| browser_download_path_lock_poisoned_error())? = settings.path.clone();
        *self
            .inner
            .download_file_exists
            .lock()
            .map_err(|_| browser_download_file_exists_lock_poisoned_error())? =
            settings.file_exists;
        *self
            .inner
            .browser_download_naming
            .lock()
            .map_err(|_| browser_download_naming_lock_poisoned_error())? = BrowserDownloadNaming {
            rename: settings.rename.clone(),
            suffix: settings.suffix.clone(),
        };
        Ok(())
    }

    pub fn when_download_file_exists(&self, mode: &str) -> OpenPageResult<()> {
        self.set_download_file_exists_mode(DownloadFileExistsMode::parse(mode)?)
    }

    pub fn load_mode(&self) -> OpenPageResult<String> {
        Ok(self.load_mode_value()?.as_str().to_string())
    }

    pub fn set_load_mode(&self, mode: LoadMode) -> OpenPageResult<()> {
        *self
            .inner
            .load_mode
            .lock()
            .map_err(|_| browser_load_mode_lock_poisoned_error())? = mode;
        Ok(())
    }

    pub fn page_download_path(&self, target_id: &str) -> OpenPageResult<Option<String>> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        if let Some(path) = settings
            .get(&frame_id)
            .and_then(|settings| settings.path.as_ref())
        {
            return Ok(Some(path.to_string_lossy().into_owned()));
        }
        drop(settings);
        self.download_path()
    }

    pub fn set_page_download_path(
        &self,
        target_id: &str,
        path: impl AsRef<Path>,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let path = path.as_ref().to_path_buf();
        create_download_directory(&path)?;
        let mut settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        settings.entry(frame_id).or_default().path = Some(path);
        Ok(())
    }

    pub fn page_download_file_exists_mode(&self, target_id: &str) -> OpenPageResult<String> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        if let Some(mode) = settings
            .get(&frame_id)
            .and_then(|settings| settings.file_exists)
        {
            return Ok(mode.as_str().to_string());
        }
        drop(settings);
        self.download_file_exists_mode()
    }

    pub fn set_page_download_file_exists_mode(
        &self,
        target_id: &str,
        mode: DownloadFileExistsMode,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        settings.entry(frame_id).or_default().file_exists = Some(mode);
        Ok(())
    }

    pub fn when_page_download_file_exists(
        &self,
        target_id: &str,
        mode: &str,
    ) -> OpenPageResult<()> {
        self.set_page_download_file_exists_mode(target_id, DownloadFileExistsMode::parse(mode)?)
    }

    pub fn set_page_download_filename(
        &self,
        target_id: &str,
        rename: Option<&str>,
        suffix: Option<&str>,
        suffix_specified: bool,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        let entry = settings.entry(frame_id).or_default();
        entry.rename = rename.map(str::to_string);
        entry.suffix = if suffix_specified {
            Some(suffix.map(str::to_string))
        } else {
            None
        };
        Ok(())
    }

    pub(crate) fn snapshot_page_download_settings(
        &self,
        target_id: &str,
    ) -> OpenPageResult<Option<PageDownloadSettings>> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let settings = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        Ok(settings.get(&frame_id).cloned())
    }

    pub(crate) fn restore_page_download_settings(
        &self,
        target_id: &str,
        settings: Option<PageDownloadSettings>,
    ) -> OpenPageResult<()> {
        let frame_id = self.resolve_page_frame_id(target_id)?;
        let mut current = self
            .inner
            .page_download_settings
            .lock()
            .map_err(|_| page_download_settings_lock_poisoned_error())?;
        if let Some(settings) = settings {
            current.insert(frame_id, settings);
        } else {
            current.remove(&frame_id);
        }
        Ok(())
    }

    pub fn download_missions(&self) -> OpenPageResult<Vec<DownloadMission>> {
        Ok(self
            .inner
            .downloads
            .mission_ids()?
            .into_iter()
            .map(|guid| DownloadMission::new(self.clone(), guid))
            .collect())
    }

    pub fn last_download(&self) -> OpenPageResult<Option<DownloadMission>> {
        Ok(self
            .inner
            .downloads
            .last_guid()?
            .map(|guid| DownloadMission::new(self.clone(), guid)))
    }

    pub fn clear_finished_downloads(&self) -> OpenPageResult<usize> {
        self.inner.downloads.clear_finished()
    }

    pub fn wait_for_download(
        &self,
        filename: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<String> {
        match filename {
            Some(filename) => {
                let info = self.inner.downloads.wait_for_name(filename, timeout_ms)?;
                self.finalize_download(&info, None)
            }
            None => {
                let completed_before = self.inner.downloads.completed_len()?;
                let info = self
                    .inner
                    .downloads
                    .wait_for_next_after(completed_before, timeout_ms)?;
                self.finalize_download(&info, None)
            }
        }
    }

    pub fn wait_for_download_begin(
        &self,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let started_before = self.inner.downloads.started_len()?;
        self.wait_for_download_begin_after(started_before, timeout_ms, cancel_it)
    }

    pub fn wait_for_download_begin_in_frames(
        &self,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let started_before = self.inner.downloads.started_len()?;
        self.wait_for_download_begin_after_in_frames(
            started_before,
            frame_ids,
            timeout_ms,
            cancel_it,
        )
    }

    pub(crate) fn download_started_len(&self) -> OpenPageResult<usize> {
        self.inner.downloads.started_len()
    }

    pub(crate) fn wait_for_download_begin_after(
        &self,
        started_before: usize,
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let info = match self
            .inner
            .downloads
            .wait_for_begin_after(started_before, timeout_ms)
        {
            Ok(info) => info,
            Err(OpenPageError::Timeout(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        self.capture_mission_download_settings(&info)?;
        let mission = DownloadMission::new(self.clone(), info.guid.clone());
        if cancel_it {
            mission.cancel()?;
        }
        Ok(Some(mission))
    }

    pub(crate) fn wait_for_download_begin_after_in_frames(
        &self,
        started_before: usize,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_it: bool,
    ) -> OpenPageResult<Option<DownloadMission>> {
        let info = match self.inner.downloads.wait_for_begin_after_in_frames(
            started_before,
            frame_ids,
            timeout_ms,
        ) {
            Ok(info) => info,
            Err(OpenPageError::Timeout(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        self.capture_mission_download_settings(&info)?;
        let mission = DownloadMission::new(self.clone(), info.guid.clone());
        if cancel_it {
            mission.cancel()?;
        }
        Ok(Some(mission))
    }

    pub fn wait_for_downloads_done(
        &self,
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        let done = self.inner.downloads.wait_until_idle(timeout_ms)?;
        if done {
            return Ok(true);
        }
        if cancel_if_timeout {
            for guid in self.inner.downloads.running_ids()? {
                let _ = self.cancel_download(&guid);
            }
        }
        Ok(false)
    }

    pub fn wait_for_downloads_done_in_frames(
        &self,
        frame_ids: &[String],
        timeout_ms: u64,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<bool> {
        let done = self
            .inner
            .downloads
            .wait_until_idle_in_frames(frame_ids, timeout_ms)?;
        if done {
            return Ok(true);
        }
        if cancel_if_timeout {
            for guid in self.inner.downloads.running_ids_in_frames(frame_ids)? {
                let _ = self.cancel_download(&guid);
            }
        }
        Ok(false)
    }

    pub fn cancel_download(&self, guid: &str) -> OpenPageResult<()> {
        let guid = guid.to_string();
        execute_browser_command_blocking(
            self.inner.runtime.as_ref(),
            &self.inner.browser,
            CancelDownloadParams::new(guid),
            "Browser::cancel_download()",
        )?;
        Ok(())
    }
}
