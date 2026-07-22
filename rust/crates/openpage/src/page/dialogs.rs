use super::*;

impl Page {
    pub fn has_alert(&self) -> OpenPageResult<bool> {
        self.alerts.has_alert()
    }

    pub fn alert_text(&self) -> OpenPageResult<Option<String>> {
        self.alerts.alert_text()
    }

    pub fn handle_alert(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
        timeout_ms: u64,
    ) -> OpenPageResult<Option<String>> {
        self.alerts.handle_alert(accept, prompt_text, timeout_ms)
    }

    pub fn set_next_alert_action(
        &self,
        accept: bool,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.alerts.set_next_alert_action(accept, prompt_text)
    }

    pub fn set_auto_alert_action(
        &self,
        accept: Option<bool>,
        prompt_text: Option<&str>,
    ) -> OpenPageResult<()> {
        self.alerts.set_auto_alert_action(accept, prompt_text)
    }

    pub fn wait_for_alert_closed(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        self.alerts.wait_for_alert_closed(timeout_ms)
    }
}
