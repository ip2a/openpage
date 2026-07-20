use super::*;

impl ServeRuntime {
    fn close_all_webpages(&mut self) {
        for (_target, state) in self.webpages.drain() {
            let _ = state.page.quit();
        }
    }

    pub(super) fn dispatch(&mut self, request: Request) -> OpenPageResult<Value> {
        match request.op.as_str() {
            "daemon.shutdown" => {
                self.close_all_webpages();
                self.shutdown = true;
                Ok(json!({"shutdown": true}))
            }
            "webpage.create" => self.create_webpage(request.target.as_deref(), &request.params),
            "webpage.quit" => {
                let target = required_target(&request)?;
                let state = self
                    .webpages
                    .remove(&target)
                    .ok_or_else(|| missing_target(&target))?;
                state.page.quit()?;
                Ok(json!({"target": target, "quit": true}))
            }
            _ => {
                let target = required_target(&request)?;
                let page = self
                    .webpages
                    .get_mut(&target)
                    .ok_or_else(|| missing_target(&target))?;
                dispatch_webpage(page, &request.op, &request.params)
            }
        }
    }

    fn create_webpage(
        &mut self,
        target_hint: Option<&str>,
        params: &Value,
    ) -> OpenPageResult<Value> {
        let mode = optional_str(params, "mode")
            .map(WebMode::parse)
            .transpose()?
            .unwrap_or(WebMode::Driver);
        let target = target_hint
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| optional_str(params, "session").map(ToOwned::to_owned))
            .unwrap_or_else(|| {
                self.next_webpage_id += 1;
                format!("wp_{}", self.next_webpage_id)
            });
        if let Some(existing) = self.webpages.get(&target) {
            return Ok(json!({
                "target": target,
                "mode": existing.page.mode()?.as_str(),
                "existing": true
            }));
        }
        let resolved = load_resolved_config()?;
        let debugger_source = resolved.debugger_source;
        let session_options = resolved.session;
        let mut launch = resolved.launch;
        let local_port = optional_u64(params, "port").map(|value| value as u16);
        apply_session_default_user_data_dir(
            &mut launch,
            resolved.user_data_dir_source,
            optional_str(params, "session").unwrap_or(&target),
            optional_string(params, "user_data_dir").is_some(),
        )?;
        apply_runtime_default_debugger_port(&mut launch, debugger_source, local_port);
        let overrides = RuntimeOverrides {
            browser_path: optional_string(params, "browser_path").map(Into::into),
            user_data_dir: optional_string(params, "user_data_dir").map(Into::into),
            local_port,
            headless: optional_bool(params, "headless"),
            width: optional_u64(params, "width").map(|value| value as u32),
            height: optional_u64(params, "height").map(|value| value as u32),
            no_sandbox: optional_bool(params, "no_sandbox"),
            incognito: optional_bool(params, "incognito"),
            mute: optional_bool(params, "mute"),
        };
        overrides.apply_to_launch(&mut launch);

        if let Some(path) = optional_string(params, "download_path") {
            launch.download_path = Some(path.into());
        }
        if let Some(load_mode) = optional_str(params, "load_mode") {
            launch.load_mode = LoadMode::parse(load_mode)?;
        }
        if let Some(mode) = optional_str(params, "download_file_exists") {
            launch.download_file_exists = DownloadFileExistsMode::parse(mode)?;
        }

        let session = session_options_from_request(params, session_options)?;

        let page = WebPage::new(mode, launch, session)?;
        self.webpages
            .insert(target.clone(), ServeWebPage::new(page));
        Ok(json!({"target": target, "mode": mode.as_str()}))
    }
}
