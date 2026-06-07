use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex as StdMutex};
use std::thread::sleep;
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
use crate::settings::{component_state_lock_poisoned_message, download_not_found_message};

fn download_state_lock_poisoned_error() -> OpenPageError {
    OpenPageError::BrowserOperation(component_state_lock_poisoned_message(
        "download state",
        "下载状态",
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    Running,
    Completed,
    Canceled,
    Skipped,
}

impl DownloadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "done",
            Self::Canceled => "canceled",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub guid: String,
    pub frame_id: String,
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

    pub fn id(&self) -> String {
        self.guid()
    }

    pub fn guid(&self) -> String {
        self.guid.clone()
    }

    pub fn url(&self) -> OpenPageResult<String> {
        Ok(self.info()?.url)
    }

    pub fn tab_id(&self) -> OpenPageResult<String> {
        self.browser.download_tab_id(&self.guid)
    }

    pub fn folder(&self) -> OpenPageResult<String> {
        self.browser.download_folder(&self.guid)
    }

    pub fn name(&self) -> OpenPageResult<String> {
        self.suggested_filename()
    }

    pub fn suggested_filename(&self) -> OpenPageResult<String> {
        Ok(self.info()?.suggested_filename)
    }

    pub fn tmp_path(&self) -> OpenPageResult<String> {
        self.browser.download_tmp_path(&self.guid)
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

    pub fn rate(&self) -> OpenPageResult<Option<f64>> {
        let info = self.info()?;
        Ok(match info.total_bytes {
            Some(total_bytes) if total_bytes > 0 => {
                Some(((info.received_bytes as f64 / total_bytes as f64) * 10000.0).round() / 100.0)
            }
            _ => None,
        })
    }

    pub fn final_path(&self) -> OpenPageResult<Option<String>> {
        Ok(self.info()?.final_path)
    }

    pub fn is_done(&self) -> OpenPageResult<bool> {
        Ok(self.info()?.state != DownloadState::Running)
    }

    pub fn wait(
        &self,
        show: bool,
        timeout_ms: Option<u64>,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<Option<String>> {
        if !show {
            return self
                .browser
                .wait_for_download_guid(&self.guid, timeout_ms, cancel_if_timeout);
        }

        self.wait_with_output(timeout_ms, cancel_if_timeout)
    }

    pub fn cancel(&self) -> OpenPageResult<()> {
        self.browser.cancel_download(&self.guid)
    }

    pub(crate) fn info(&self) -> OpenPageResult<DownloadInfo> {
        self.browser.download_info(&self.guid)
    }

    fn wait_with_output(
        &self,
        timeout_ms: Option<u64>,
        cancel_if_timeout: bool,
    ) -> OpenPageResult<Option<String>> {
        println!("url: {}", self.url()?);
        println!("name: {}", self.name()?);
        println!("folder: {}", self.folder()?);

        let deadline =
            timeout_ms.map(|timeout_ms| Instant::now() + Duration::from_millis(timeout_ms));
        loop {
            let info = self.info()?;
            if info.state != DownloadState::Running {
                break;
            }

            if let Some(rate) = download_rate(&info) {
                print!("\r{rate}% ");
            }
            io::stdout().flush()?;

            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                if cancel_if_timeout {
                    let _ = self.cancel();
                }
                println!();
                return Ok(None);
            }

            sleep(Duration::from_millis(200));
        }

        let info = self.info()?;
        let result = self
            .browser
            .wait_for_download_guid(&self.guid, Some(0), false)?;
        match info.state {
            DownloadState::Completed => {
                print!("\r100% ");
                if let Some(path) = result.as_deref() {
                    println!("{path}");
                } else {
                    println!();
                }
            }
            DownloadState::Canceled => println!("download canceled"),
            DownloadState::Skipped => {
                if let Some(path) = result.as_deref() {
                    println!("skipped {path}");
                } else {
                    println!("skipped");
                }
            }
            DownloadState::Running => println!(),
        }
        Ok(result)
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
            .map_err(|_| download_state_lock_poisoned_error())
    }

    pub(crate) fn last_guid(&self) -> OpenPageResult<Option<String>> {
        self.shared
            .state
            .lock()
            .map(|state| state.order.last().cloned())
            .map_err(|_| download_state_lock_poisoned_error())
    }

    pub(crate) fn info(&self, guid: &str) -> OpenPageResult<DownloadInfo> {
        self.shared
            .state
            .lock()
            .map_err(|_| download_state_lock_poisoned_error())?
            .missions
            .get(guid)
            .cloned()
            .ok_or_else(|| OpenPageError::BrowserOperation(download_not_found_message(guid)))
    }

    pub(crate) fn completed_len(&self) -> OpenPageResult<usize> {
        self.shared
            .state
            .lock()
            .map(|state| state.completed_order.len())
            .map_err(|_| download_state_lock_poisoned_error())
    }

    pub(crate) fn started_len(&self) -> OpenPageResult<usize> {
        self.shared
            .state
            .lock()
            .map(|state| state.order.len())
            .map_err(|_| download_state_lock_poisoned_error())
    }

    pub(crate) fn clear_finished(&self) -> OpenPageResult<usize> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| download_state_lock_poisoned_error())?;
        let finished = state
            .order
            .iter()
            .filter(|guid| {
                state
                    .missions
                    .get(*guid)
                    .is_some_and(|mission| mission.state != DownloadState::Running)
            })
            .cloned()
            .collect::<Vec<_>>();
        let removed = finished.len();
        if removed == 0 {
            return Ok(0);
        }
        for guid in &finished {
            state.missions.remove(guid);
        }
        state
            .order
            .retain(|guid| !finished.iter().any(|item| item == guid));
        state
            .completed_order
            .retain(|guid| !finished.iter().any(|item| item == guid));
        self.shared.condvar.notify_all();
        Ok(removed)
    }

    pub(crate) fn running_ids(&self) -> OpenPageResult<Vec<String>> {
        self.shared
            .state
            .lock()
            .map(|state| {
                state
                    .missions
                    .values()
                    .filter(|mission| mission.state == DownloadState::Running)
                    .map(|mission| mission.guid.clone())
                    .collect()
            })
            .map_err(|_| download_state_lock_poisoned_error())
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

    pub(crate) fn wait_for_guid_forever(&self, guid: &str) -> OpenPageResult<DownloadInfo> {
        self.wait_forever(|state| match state.missions.get(guid) {
            Some(mission) if mission.state != DownloadState::Running => Some(mission.clone()),
            _ => None,
        })
    }

    pub(crate) fn set_finalized(
        &self,
        guid: &str,
        state: DownloadState,
        final_path: String,
    ) -> OpenPageResult<()> {
        let mut store = self
            .shared
            .state
            .lock()
            .map_err(|_| download_state_lock_poisoned_error())?;
        let mission = store
            .missions
            .get_mut(guid)
            .ok_or_else(|| OpenPageError::BrowserOperation(download_not_found_message(guid)))?;
        mission.state = state;
        mission.final_path = Some(final_path);
        self.shared.condvar.notify_all();
        Ok(())
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

    pub(crate) fn wait_for_begin_after(
        &self,
        started_before: usize,
        timeout_ms: u64,
    ) -> OpenPageResult<DownloadInfo> {
        self.wait_for(timeout_ms, |state| {
            state
                .order
                .get(started_before)
                .and_then(|guid| state.missions.get(guid))
                .cloned()
        })
    }

    pub(crate) fn wait_for_begin_after_in_frames(
        &self,
        started_before: usize,
        frame_ids: &[String],
        timeout_ms: u64,
    ) -> OpenPageResult<DownloadInfo> {
        self.wait_for(timeout_ms, |state| {
            state
                .order
                .iter()
                .skip(started_before)
                .filter_map(|guid| state.missions.get(guid))
                .find(|mission| {
                    frame_ids
                        .iter()
                        .any(|frame_id| frame_id == &mission.frame_id)
                })
                .cloned()
        })
    }

    pub(crate) fn running_ids_in_frames(
        &self,
        frame_ids: &[String],
    ) -> OpenPageResult<Vec<String>> {
        self.shared
            .state
            .lock()
            .map(|state| {
                state
                    .missions
                    .values()
                    .filter(|mission| {
                        mission.state == DownloadState::Running
                            && frame_ids
                                .iter()
                                .any(|frame_id| frame_id == &mission.frame_id)
                    })
                    .map(|mission| mission.guid.clone())
                    .collect()
            })
            .map_err(|_| download_state_lock_poisoned_error())
    }

    pub(crate) fn wait_until_idle_in_frames(
        &self,
        frame_ids: &[String],
        timeout_ms: u64,
    ) -> OpenPageResult<bool> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| download_state_lock_poisoned_error())?;

        loop {
            if state
                .missions
                .values()
                .filter(|mission| {
                    frame_ids
                        .iter()
                        .any(|frame_id| frame_id == &mission.frame_id)
                })
                .all(|mission| mission.state != DownloadState::Running)
            {
                return Ok(true);
            }

            if let Some(error) = &state.last_error {
                return Err(OpenPageError::BrowserOperation(format!(
                    "download tracker stopped: {error}"
                )));
            }

            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }

            let remaining = deadline.saturating_duration_since(now);
            let result = self
                .shared
                .condvar
                .wait_timeout(state, remaining)
                .map_err(|_| download_state_lock_poisoned_error())?;
            state = result.0;
            if result.1.timed_out() {
                return Ok(state
                    .missions
                    .values()
                    .filter(|mission| {
                        frame_ids
                            .iter()
                            .any(|frame_id| frame_id == &mission.frame_id)
                    })
                    .all(|mission| mission.state != DownloadState::Running));
            }
        }
    }

    pub(crate) fn wait_until_idle(&self, timeout_ms: u64) -> OpenPageResult<bool> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| download_state_lock_poisoned_error())?;

        loop {
            if state
                .missions
                .values()
                .all(|mission| mission.state != DownloadState::Running)
            {
                return Ok(true);
            }

            if let Some(error) = &state.last_error {
                return Err(OpenPageError::BrowserOperation(format!(
                    "download tracker stopped: {error}"
                )));
            }

            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }

            let remaining = deadline.saturating_duration_since(now);
            let result = self
                .shared
                .condvar
                .wait_timeout(state, remaining)
                .map_err(|_| download_state_lock_poisoned_error())?;
            state = result.0;
            if result.1.timed_out() {
                return Ok(state
                    .missions
                    .values()
                    .all(|mission| mission.state != DownloadState::Running));
            }
        }
    }

    fn wait_for<F>(&self, timeout_ms: u64, predicate: F) -> OpenPageResult<DownloadInfo>
    where
        F: Fn(&DownloadManagerState) -> Option<DownloadInfo>,
    {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| download_state_lock_poisoned_error())?;

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
                .map_err(|_| download_state_lock_poisoned_error())?;
            state = result.0;
            if result.1.timed_out() {
                return Err(OpenPageError::Timeout(
                    "download did not complete in time".to_string(),
                ));
            }
        }
    }

    fn wait_forever<F>(&self, predicate: F) -> OpenPageResult<DownloadInfo>
    where
        F: Fn(&DownloadManagerState) -> Option<DownloadInfo>,
    {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| download_state_lock_poisoned_error())?;

        loop {
            if let Some(info) = predicate(&state) {
                return Ok(info);
            }

            if let Some(error) = &state.last_error {
                return Err(OpenPageError::BrowserOperation(format!(
                    "download tracker stopped: {error}"
                )));
            }

            state = self
                .shared
                .condvar
                .wait(state)
                .map_err(|_| download_state_lock_poisoned_error())?;
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
        .map_err(|_| download_state_lock_poisoned_error())?;
    let mission = state
        .missions
        .entry(event.guid.clone())
        .or_insert_with(|| DownloadInfo {
            guid: event.guid.clone(),
            frame_id: event.frame_id.as_ref().to_string(),
            url: event.url.clone(),
            suggested_filename: event.suggested_filename.clone(),
            state: DownloadState::Running,
            received_bytes: 0,
            total_bytes: None,
            final_path: None,
        });
    mission.frame_id = event.frame_id.as_ref().to_string();
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
        .map_err(|_| download_state_lock_poisoned_error())?;

    let mission = state
        .missions
        .entry(event.guid.clone())
        .or_insert_with(|| DownloadInfo {
            guid: event.guid.clone(),
            frame_id: String::new(),
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
        .map_err(|_| download_state_lock_poisoned_error())?;
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

fn download_rate(info: &DownloadInfo) -> Option<f64> {
    match info.total_bytes {
        Some(total_bytes) if total_bytes > 0 => {
            Some(((info.received_bytes as f64 / total_bytes as f64) * 10000.0).round() / 100.0)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::DownloadStore;
    use crate::Settings;
    use crate::settings::scoped_test_settings;
    use std::sync::Arc;
    use std::thread;

    fn poison_download_state(store: &DownloadStore) {
        let shared = Arc::clone(&store.shared);
        let join = thread::spawn(move || {
            let _guard = shared
                .state
                .lock()
                .expect("lock download state for poison test");
            panic!("poison download state");
        })
        .join();
        assert!(join.is_err(), "poison helper thread should panic");
    }

    #[test]
    fn download_state_lock_poisoned_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let store = DownloadStore::new();
        poison_download_state(&store);

        let english = store
            .mission_ids()
            .expect_err("mission_ids() should surface poisoned download state")
            .to_string();
        assert!(english.contains("download state lock poisoned"));
        assert!(english.contains("browser operation failed"));

        Settings::set_language("cn");

        let chinese = store
            .mission_ids()
            .expect_err("mission_ids() should localize poisoned download state")
            .to_string();
        assert!(chinese.contains("下载状态锁已损坏"));
        assert!(chinese.contains("浏览器操作失败"));
    }

    #[test]
    fn download_not_found_errors_follow_language_setting() {
        let _settings = scoped_test_settings();
        Settings::reset();

        let store = DownloadStore::new();

        let english = store
            .info("missing-guid")
            .expect_err("missing download should fail")
            .to_string();
        assert_eq!(
            english,
            "browser operation failed: download `missing-guid` was not found"
        );

        Settings::set_language("cn");

        let chinese = store
            .set_finalized(
                "missing-guid",
                super::DownloadState::Completed,
                "/tmp/out".into(),
            )
            .expect_err("missing download should localize")
            .to_string();
        assert_eq!(chinese, "浏览器操作失败: 没有找到下载任务 `missing-guid`");
    }
}
