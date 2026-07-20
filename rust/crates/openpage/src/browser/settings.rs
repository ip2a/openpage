use super::*;

impl Browser {
    pub fn is_alive(&self) -> OpenPageResult<bool> {
        Ok(self.version().is_ok())
    }

    pub fn is_headless(&self) -> bool {
        self.inner.headless
    }

    pub fn is_existed(&self) -> OpenPageResult<bool> {
        self.is_alive()
    }

    pub fn is_incognito(&self) -> OpenPageResult<bool> {
        self.inner.runtime.block_on(async {
            let browser =
                lock_with_cdp_timeout(&self.inner.browser, "Browser::is_incognito().lock()")
                    .await?;
            Ok(browser.is_incognito())
        })
    }

    pub fn browser_pid(&self) -> Option<u32> {
        self.inner.browser_pid
    }

    pub fn process_id(&self) -> Option<u32> {
        self.browser_pid()
    }

    pub fn timeouts(&self) -> OpenPageResult<TimeoutConfig> {
        self.inner
            .timeouts
            .lock()
            .map(|t| t.clone())
            .map_err(|_| browser_timeouts_lock_poisoned_error())
    }

    pub fn set_timeouts(&self, timeouts: TimeoutConfig) -> OpenPageResult<()> {
        *self
            .inner
            .timeouts
            .lock()
            .map_err(|_| browser_timeouts_lock_poisoned_error())? = timeouts;
        Ok(())
    }

    pub fn retry_times(&self) -> OpenPageResult<usize> {
        self.inner
            .retry_times
            .lock()
            .map(|retry_times| *retry_times)
            .map_err(|_| browser_retry_times_lock_poisoned_error())
    }

    pub fn retry_interval_millis(&self) -> OpenPageResult<u64> {
        self.inner
            .retry_interval_millis
            .lock()
            .map(|retry_interval_millis| *retry_interval_millis)
            .map_err(|_| browser_retry_interval_lock_poisoned_error())
    }

    pub fn set_retry(
        &self,
        retry_times: Option<usize>,
        retry_interval_millis: Option<u64>,
    ) -> OpenPageResult<()> {
        if let Some(retry_times) = retry_times {
            *self
                .inner
                .retry_times
                .lock()
                .map_err(|_| browser_retry_times_lock_poisoned_error())? = retry_times;
        }
        if let Some(retry_interval_millis) = retry_interval_millis {
            *self
                .inner
                .retry_interval_millis
                .lock()
                .map_err(|_| browser_retry_interval_lock_poisoned_error())? = retry_interval_millis;
        }
        Ok(())
    }
}
