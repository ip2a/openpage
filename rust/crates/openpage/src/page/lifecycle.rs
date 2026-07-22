use super::*;

impl Page {
    pub fn browser(&self) -> Option<&Browser> {
        self.browser.as_ref()
    }

    pub fn target_id(&self) -> String {
        self.inner.target_id().as_ref().to_string()
    }

    pub fn browser_pid(&self) -> Option<u32> {
        self.browser_pid
    }

    pub fn process_id(&self) -> Option<u32> {
        self.browser_pid()
    }

    pub fn browser_version(&self) -> OpenPageResult<String> {
        self.browser_backed_ref("browser_version")?.version()
    }

    pub fn address(&self) -> OpenPageResult<String> {
        Ok(self.browser_backed_ref("address")?.address())
    }

    pub fn quit(&self) -> OpenPageResult<()> {
        self.browser_backed_ref("quit")?.close()
    }

    pub fn reconnect(&self, wait_ms: u64) -> OpenPageResult<Self> {
        if wait_ms > 0 {
            sleep(Duration::from_millis(wait_ms));
        }
        let browser = self.browser_backed_ref("reconnect")?.reconnect()?;
        browser.get_page(&self.target_id())
    }

    pub fn disconnect(self) -> OpenPageResult<DisconnectedPage> {
        let target_id = self.target_id();
        let browser = self.browser_backed_ref("disconnect")?.clone();
        Ok(DisconnectedPage { browser, target_id })
    }

    pub fn is_alive(&self) -> OpenPageResult<bool> {
        self.runtime.block_on(async {
            Ok(run_with_timeout(
                async {
                    self.inner
                        .url()
                        .await
                        .map_err(|err| page_operation_error("Page::is_alive()", err))
                },
                timeout_duration_millis(cdp_timeout_duration()),
                "Page::is_alive()",
            )
            .await
            .is_ok())
        })
    }
}
