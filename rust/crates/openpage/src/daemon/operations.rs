use super::*;

impl ServeState {
    fn close_all_pages(&mut self) {
        for (_target, state) in self.pages.drain() {
            let _ = state.page.quit();
        }
    }

    pub(super) fn dispatch(&mut self, request: Request) -> OpenPageResult<Value> {
        match request.op.as_str() {
            "daemon.shutdown" => {
                self.close_all_pages();
                self.shutdown = true;
                Ok(json!({"shutdown": true}))
            }
            "page.create" => self.create_page(request.target.as_deref(), &request.params),
            "page.quit" => {
                let target = required_target(&request)?;
                let state = self
                    .pages
                    .remove(&target)
                    .ok_or_else(|| missing_target(&target))?;
                state.page.quit()?;
                Ok(json!({"target": target, "quit": true}))
            }
            _ => {
                let target = required_target(&request)?;
                let page = self
                    .pages
                    .get_mut(&target)
                    .ok_or_else(|| missing_target(&target))?;
                dispatch_page(page, &request.op, &request.params)
            }
        }
    }

    fn create_page(&mut self, target_hint: Option<&str>, params: &Value) -> OpenPageResult<Value> {
        let target = target_hint
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                self.next_page_id += 1;
                format!("page_{}", self.next_page_id)
            });
        if self.pages.contains_key(&target) {
            return Ok(json!({"target": target, "existing": true}));
        }
        let resolved = load_resolved_config()?;
        let mut launch = resolved.launch;
        let local_port = optional_u64(params, "port").map(|value| value as u16);
        let overrides = ConfigOverrides {
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
        let browser = crate::browser::Browser::launch(launch)?;
        let page = browser.new_page(None::<&str>)?;
        self.pages.insert(target.clone(), ServePage::new(page));
        Ok(json!({"target": target}))
    }
}
