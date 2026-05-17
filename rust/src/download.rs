use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::time::{Duration, Instant};

use chromiumoxide::browser::Browser as OxBrowser;
use chromiumoxide::cdp::browser_protocol::browser::{
    DownloadProgressState, EventDownloadProgress, EventDownloadWillBegin,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::browser::Browser;
use crate::error::{OpenPageError, OpenPageResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    Running,
    Completed,
    Canceled,
}

impl DownloadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub guid: String,
    pub url: String,
    pub suggested_filename: String,
    pub state: DownloadState,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub final_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DownloadMission {
    browser: Browser,
    guid: String,
}

impl DownloadMission {
    pub(crate) fn new(browser: Browser, guid: String) -> Self {
        Self { browser, guid }
    }

    pub fn guid(&self) -> String {
        self.guid.clone()
    }

    pub fn url(&self) -> OpenPageResult<String> {
        Ok(self.info()?.url)
    }

    pub fn suggested_filename(&self) -> OpenPageResult<String> {
        Ok(self.info()?.suggested_filename)
    }

    pub fn state(&self) -> OpenPageResult<String> {
        Ok(self.info()?.state.as_str().to_string())
    }

    pub fn received_bytes(&self) -> OpenPageResult<u64> {
        Ok(self.info()?.received_bytes)
    }

    pub fn total_bytes(&self) -> OpenPageResult<Option<u64>> {
        Ok(self.info()?.total_bytes)
    }

    pub fn final_path(&self) -> OpenPageResult<Option<String>> {
        Ok(self.info()?.final_path)
    }

    pub fn is_done(&self) -> OpenPageResult<bool> {
        Ok(self.info()?.state != DownloadState::Running)
    }

    pub fn wait(&self, timeout_ms: u64) -> OpenPageResult<String> {
        self.browser.wait_for_download_guid(&self.guid, timeout_ms)
    }

    pub fn cancel(&self) -> OpenPageResult<()> {
        self.browser.cancel_download(&self.guid)
    }

    pub(crate) fn info(&self) -> OpenPageResult<DownloadInfo> {
        self.browser.download_info(&self.guid)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DownloadStore {
    shared: Arc<DownloadShared>,
}

impl DownloadStore {
    fn new() -> Self {
        Self {
            shared: Arc::new(DownloadShared::new()),
        }
    }

    pub(crate) fn mission_ids(&self) -> OpenPageResult<Vec<String>> {
        self.shared
            .state
            .lock()
            .map(|state| state.order.clone())
            .map_err(|_| {
                OpenPageError::BrowserOperation("download state lock poisoned".to_string())
            })
    }

    pub(crate) fn last_guid(&self) -> OpenPageResult<Option<String>> {
        self.shared
            .state
            .lock()
            .map(|state| state.order.last().cloned())
            .map_err(|_| {
                OpenPageError::BrowserOperation("download state lock poisoned".to_string())
            })
    }

    pub(crate) fn info(&self, guid: &str) -> OpenPageResult<DownloadInfo> {
        self.shared
            .state
            .lock()
            .map_err(|_| {
                OpenPageError::BrowserOperation("download state lock poisoned".to_string())
            })?
            .missions
            .get(guid)
            .cloned()
            .ok_or_else(|| {
                OpenPageError::BrowserOperation(format!("download `{guid}` was not found"))
            })
    }

    pub(crate) fn completed_len(&self) -> OpenPageResult<usize> {
        self.shared
            .state
            .lock()
            .map(|state| state.completed_order.len())
            .map_err(|_| {
                OpenPageError::BrowserOperation("download state lock poisoned".to_string())
            })
    }

    pub(crate) fn wait_for_name(
        &self,
        filename: &str,
        timeout_ms: u64,
    ) -> OpenPageResult<DownloadInfo> {
        self.wait_for(timeout_ms, |state| {
            state
                .missions
                .values()
                .filter(|mission| mission.state == DownloadState::Completed)
                .find(|mission| filename_matches(mission, filename))
                .cloned()
        })
    }

    pub(crate) fn wait_for_guid(
        &self,
        guid: &str,
        timeout_ms: u64,
    ) -> OpenPageResult<DownloadInfo> {
        self.wait_for(timeout_ms, |state| match state.missions.get(guid) {
            Some(mission) if mission.state != DownloadState::Running => Some(mission.clone()),
            _ => None,
        })
    }

    pub(crate) fn wait_for_next_after(
        &self,
        completed_before: usize,
        timeout_ms: u64,
    ) -> OpenPageResult<DownloadInfo> {
        self.wait_for(timeout_ms, |state| {
            state
                .completed_order
                .get(completed_before)
                .and_then(|guid| state.missions.get(guid))
                .cloned()
        })
    }

    fn wait_for<F>(&self, timeout_ms: u64, predicate: F) -> OpenPageResult<DownloadInfo>
    where
        F: Fn(&DownloadManagerState) -> Option<DownloadInfo>,
    {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut state = self.shared.state.lock().map_err(|_| {
            OpenPageError::BrowserOperation("download state lock poisoned".to_string())
        })?;

        loop {
            if let Some(info) = predicate(&state) {
                return Ok(info);
            }

            if let Some(error) = &state.last_error {
                return Err(OpenPageError::BrowserOperation(format!(
                    "download tracker stopped: {error}"
                )));
            }

            let now = Instant::now();
            if now >= deadline {
                return Err(OpenPageError::Timeout(
                    "download did not complete in time".to_string(),
                ));
            }

            let remaining = deadline.saturating_duration_since(now);
            let result = self
                .shared
                .condvar
                .wait_timeout(state, remaining)
                .map_err(|_| {
                    OpenPageError::BrowserOperation("download state lock poisoned".to_string())
                })?;
            state = result.0;
            if result.1.timed_out() {
                return Err(OpenPageError::Timeout(
                    "download did not complete in time".to_string(),
                ));
            }
        }
    }
}

#[derive(Debug)]
struct DownloadManagerState {
    missions: HashMap<String, DownloadInfo>,
    order: Vec<String>,
    completed_order: Vec<String>,
    last_error: Option<String>,
}

impl DownloadManagerState {
    fn new() -> Self {
        Self {
            missions: HashMap::new(),
            order: Vec::new(),
            completed_order: Vec::new(),
            last_error: None,
        }
    }
}

#[derive(Debug)]
struct DownloadShared {
    state: StdMutex<DownloadManagerState>,
    condvar: Condvar,
}

impl DownloadShared {
    fn new() -> Self {
        Self {
            state: StdMutex::new(DownloadManagerState::new()),
            condvar: Condvar::new(),
        }
    }
}

pub(crate) fn attach_download_store(
    runtime: Arc<Runtime>,
    browser: &OxBrowser,
) -> OpenPageResult<(DownloadStore, JoinHandle<()>)> {
    let (mut will_begin, mut progress) = runtime.block_on(async {
        let will_begin = browser
            .event_listener::<EventDownloadWillBegin>()
            .await
            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        let progress = browser
            .event_listener::<EventDownloadProgress>()
            .await
            .map_err(|err| OpenPageError::BrowserOperation(err.to_string()))?;
        Ok::<_, OpenPageError>((will_begin, progress))
    })?;

    let store = DownloadStore::new();
    let shared = Arc::clone(&store.shared);
    let task = runtime.spawn(async move {
        let result: OpenPageResult<()> = async {
            loop {
                tokio::select! {
                    event = will_begin.next() => match event {
                        Some(event) => on_download_will_begin(&shared, &event)?,
                        None => break,
                    },
                    event = progress.next() => match event {
                        Some(event) => on_download_progress(&shared, &event)?,
                        None => break,
                    },
                }
            }
            Ok(())
        }
        .await;

        let _ = mark_tracker_stopped(&shared, result.err().map(|err| err.to_string()));
    });

    Ok((store, task))
}

fn on_download_will_begin(
    shared: &Arc<DownloadShared>,
    event: &EventDownloadWillBegin,
) -> OpenPageResult<()> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("download state lock poisoned".to_string()))?;
    let mission = state
        .missions
        .entry(event.guid.clone())
        .or_insert_with(|| DownloadInfo {
            guid: event.guid.clone(),
            url: event.url.clone(),
            suggested_filename: event.suggested_filename.clone(),
            state: DownloadState::Running,
            received_bytes: 0,
            total_bytes: None,
            final_path: None,
        });
    mission.url = event.url.clone();
    mission.suggested_filename = event.suggested_filename.clone();
    if !state.order.iter().any(|guid| guid == &event.guid) {
        state.order.push(event.guid.clone());
    }
    shared.condvar.notify_all();
    Ok(())
}

fn on_download_progress(
    shared: &Arc<DownloadShared>,
    event: &EventDownloadProgress,
) -> OpenPageResult<()> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("download state lock poisoned".to_string()))?;

    let mission = state
        .missions
        .entry(event.guid.clone())
        .or_insert_with(|| DownloadInfo {
            guid: event.guid.clone(),
            url: String::new(),
            suggested_filename: event.guid.clone(),
            state: DownloadState::Running,
            received_bytes: 0,
            total_bytes: None,
            final_path: None,
        });
    mission.received_bytes = event.received_bytes.max(0.0) as u64;
    mission.total_bytes = Some(event.total_bytes.max(0.0) as u64);
    if let Some(file_path) = &event.file_path {
        mission.final_path = Some(file_path.clone());
    }

    let next_state = match event.state {
        DownloadProgressState::InProgress => DownloadState::Running,
        DownloadProgressState::Completed => DownloadState::Completed,
        DownloadProgressState::Canceled => DownloadState::Canceled,
    };
    if mission.state != next_state {
        mission.state = next_state;
        if next_state == DownloadState::Completed
            && !state.completed_order.iter().any(|guid| guid == &event.guid)
        {
            state.completed_order.push(event.guid.clone());
        }
    }

    shared.condvar.notify_all();
    Ok(())
}

fn mark_tracker_stopped(shared: &Arc<DownloadShared>, error: Option<String>) -> OpenPageResult<()> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| OpenPageError::BrowserOperation("download state lock poisoned".to_string()))?;
    state.last_error = error;
    shared.condvar.notify_all();
    Ok(())
}

fn filename_matches(mission: &DownloadInfo, filename: &str) -> bool {
    mission.suggested_filename == filename
        || mission.final_path.as_deref().is_some_and(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == filename)
        })
}
