from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
from ..download import DownloadMission

def _resolve_timeout_ms(owner, timeout):
    return int((timeout if timeout is not None else getattr(owner, "timeout", 10.0)) * 1000)

def _download_mission_to_dict(mission):
    return {"url": mission.url, "tab_id": mission.tab_id, "id": mission.id, "guid": mission.guid, "folder": mission.folder, "name": mission.name, "suggested_filename": mission.suggested_filename, "tmp_path": mission.tmp_path, "state": mission.state, "total_bytes": mission.total_bytes, "received_bytes": mission.received_bytes, "final_path": mission.final_path, "rate": mission.rate, "is_done": mission.is_done}

class BrowserWait:
    def __init__(self, browser: Browser) -> None:
        self._browser = browser

    def new_tab(self, timeout: float = 10.0, curr_tab: str | None = None) -> str | bool:
        target_id = self._browser._inner.wait_for_new_tab(curr_tab, int(timeout * 1000))
        return False if target_id is None else target_id

    def download_begin(
        self,
        timeout: float | None = None,
        cancel_it: bool = False,
    ) -> "DownloadMission | dict[str, Any] | bool":
        timeout_ms = _resolve_timeout_ms(self._browser, timeout)
        mission = self._browser._inner.wait_for_download_begin(timeout_ms, cancel_it)
        if mission is None:
            return False
        wrapped = DownloadMission(mission)
        return _download_mission_to_dict(wrapped) if cancel_it else wrapped

    def downloads_done(
        self,
        timeout: float | None = None,
        cancel_if_timeout: bool = True,
    ) -> bool:
        if timeout is not None:
            return self._browser._inner.wait_for_downloads_done(
                int(timeout * 1000),
                cancel_if_timeout,
            )
        while not self._browser._inner.wait_for_downloads_done(60000, False):
            pass
        return True
