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
        if let Some(page) = self.pages.get(&target) {
            return Ok(page_create_payload(&target, true, page.attached));
        }
        let attach = optional_string(params, "attach");
        let browser = if let Some(debugger_url) = attach.as_deref() {
            // An explicit attach is never a launch request. In particular, do
            // not let a failed connection fall through to Browser::launch().
            crate::browser::Browser::connect(debugger_url)?
        } else {
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
            crate::browser::Browser::launch(launch)?
        };
        let page = browser
            .pages()?
            .into_iter()
            .next()
            .map(Ok)
            .unwrap_or_else(|| browser.new_page(None::<&str>))?;
        let attached = attach.is_some();
        self.pages
            .insert(target.clone(), ServePage::new(page, attached));
        Ok(page_create_payload(&target, false, attached))
    }
}

fn page_create_payload(target: &str, existing: bool, attached: bool) -> Value {
    if existing {
        json!({"target": target, "existing": true, "attached": attached})
    } else {
        json!({"target": target, "attached": attached})
    }
}

#[cfg(test)]
mod tests {
    use super::page_create_payload;

    #[test]
    fn existing_page_payload_preserves_session_attach_state() {
        assert_eq!(
            page_create_payload("session", true, false)["attached"],
            false
        );
        assert_eq!(page_create_payload("session", true, true)["attached"], true);
    }
}
