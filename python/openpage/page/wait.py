from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
from ..download import DownloadMission

def _resolve_timeout_ms(owner, timeout):
    return int((timeout if timeout is not None else getattr(owner, "timeout", 10.0)) * 1000)

def _download_mission_to_dict(mission):
    return {"url": mission.url, "tab_id": mission.tab_id, "id": mission.id, "guid": mission.guid, "folder": mission.folder, "name": mission.name, "suggested_filename": mission.suggested_filename, "tmp_path": mission.tmp_path, "state": mission.state, "total_bytes": mission.total_bytes, "received_bytes": mission.received_bytes, "final_path": mission.final_path, "rate": mission.rate, "is_done": mission.is_done}

class PageWait:
    def __init__(self, page: Page) -> None:
        self._page = page

    def download_begin(
        self,
        timeout: float | None = None,
        cancel_it: bool = False,
    ) -> "DownloadMission | dict[str, Any] | bool":
        timeout_ms = _resolve_timeout_ms(self._page, timeout)
        mission = self._page._inner.wait_for_download_begin(timeout_ms, cancel_it)
        if mission is None:
            return False
        wrapped = DownloadMission(mission)
        return _download_mission_to_dict(wrapped) if cancel_it else wrapped

    def upload_paths_inputted(self) -> bool:
        return self._page._inner.wait_for_upload_paths_inputted(
            _resolve_timeout_ms(self._page, None)
        )

    def downloads_done(
        self,
        timeout: float | None = None,
        cancel_if_timeout: bool = True,
    ) -> bool:
        if timeout is not None:
            return self._page._inner.wait_for_downloads_done(
                int(timeout * 1000),
                cancel_if_timeout,
            )
        while not self._page._inner.wait_for_downloads_done(60000, False):
            pass
        return True

    def all_downloads_done(
        self,
        timeout: float | None = None,
        cancel_if_timeout: bool = True,
    ) -> bool:
        return self.downloads_done(timeout, cancel_if_timeout)

    def ele_displayed(
        self,
        loc_or_ele: str | "Element",
        timeout: float = 10.0,
    ) -> "Element | bool":
        from ..element.element import Element
        if isinstance(loc_or_ele, Element):
            return loc_or_ele.wait.displayed(timeout)
        if self._page._inner.wait_for_ele_displayed(loc_or_ele, int(timeout * 1000)):
            try:
                return self._page.ele(loc_or_ele)
            except Exception:
                return True
        return False

    def ele_hidden(
        self,
        loc_or_ele: str | "Element",
        timeout: float = 10.0,
    ) -> "Element | bool":
        from ..element.element import Element
        if isinstance(loc_or_ele, Element):
            return loc_or_ele.wait.hidden(timeout)
        if self._page._inner.wait_for_ele_hidden(loc_or_ele, int(timeout * 1000)):
            try:
                return self._page.ele(loc_or_ele)
            except Exception:
                return True
        return False

    def ele_deleted(
        self,
        loc_or_ele: str | "Element",
        timeout: float = 10.0,
    ) -> "Element | bool":
        from ..element.element import Element
        if isinstance(loc_or_ele, Element):
            return loc_or_ele.wait.deleted(timeout)
        if self._page._inner.wait_for_ele_deleted(loc_or_ele, int(timeout * 1000)):
            try:
                return self._page.ele(loc_or_ele)
            except Exception:
                return True
        return False

    def ele_enabled(
        self,
        loc_or_ele: str | "Element",
        timeout: float = 10.0,
    ) -> "Element | bool":
        from ..element.element import Element
        if isinstance(loc_or_ele, Element):
            return loc_or_ele.wait.enabled(timeout)
        if self._page._inner.wait_for_ele_enabled(loc_or_ele, int(timeout * 1000)):
            try:
                return self._page.ele(loc_or_ele)
            except Exception:
                return True
        return False

    def ele_clickable(
        self,
        loc_or_ele: str | "Element",
        timeout: float = 10.0,
    ) -> "Element | bool":
        from ..element.element import Element
        if isinstance(loc_or_ele, Element):
            return loc_or_ele.wait.clickable(timeout)
        if self._page._inner.wait_for_ele_clickable(loc_or_ele, int(timeout * 1000)):
            try:
                return self._page.ele(loc_or_ele)
            except Exception:
                return True
        return False

    def url_change(
        self,
        text: str,
        exclude: bool = False,
        timeout: float = 10.0,
    ) -> "Page | bool":
        return self._page if self._page._inner.wait_for_url_change(text, exclude, int(timeout * 1000)) else False

    def title_change(
        self,
        text: str,
        exclude: bool = False,
        timeout: float = 10.0,
    ) -> "Page | bool":
        return self._page if self._page._inner.wait_for_title_change(text, exclude, int(timeout * 1000)) else False

    def load_start(self, timeout: float = 10.0) -> bool:
        return self._page._inner.wait_for_load_start(int(timeout * 1000))

    def doc_loaded(self, timeout: float = 10.0) -> bool:
        return self._page._inner.wait_for_doc_loaded(int(timeout * 1000))

    def eles_loaded(
        self,
        locators: str | list[str] | tuple[str, ...] | set[str],
        timeout: float = 10.0,
        any_one: bool = False,
    ) -> bool:
        values = [locators] if isinstance(locators, str) else list(locators)
        return self._page._inner.wait_for_elements_loaded(values, int(timeout * 1000), any_one)

    def alert_closed(self, timeout: float = 10.0) -> "Page | bool":
        return self._page if self._page._inner.wait_for_alert_closed(int(timeout * 1000)) else False


class WebPageWait:
    def __init__(self, page: WebPage) -> None:
        self._page = page

    def new_tab(self, timeout: float = 10.0, curr_tab: str | None = None) -> str | bool:
        target_id = self._page._inner.wait_for_new_tab(curr_tab, int(timeout * 1000))
        return False if target_id is None else target_id

    def all_downloads_done(
        self,
        timeout: float | None = None,
        cancel_if_timeout: bool = True,
    ) -> bool:
        return self.downloads_done(timeout, cancel_if_timeout)

    def download_begin(
        self,
        timeout: float | None = None,
        cancel_it: bool = False,
    ) -> "DownloadMission | dict[str, Any] | bool":
        timeout_ms = _resolve_timeout_ms(self._page, timeout)
        mission = self._page._inner.wait_for_download_begin(timeout_ms, cancel_it)
        if mission is None:
            return False
        wrapped = DownloadMission(mission)
        return _download_mission_to_dict(wrapped) if cancel_it else wrapped

    def upload_paths_inputted(self) -> bool:
        return self._page._inner.wait_for_upload_paths_inputted(
            _resolve_timeout_ms(self._page, None)
        )

    def downloads_done(
        self,
        timeout: float | None = None,
        cancel_if_timeout: bool = True,
    ) -> bool:
        if timeout is not None:
            return self._page._inner.wait_for_downloads_done(
                int(timeout * 1000),
                cancel_if_timeout,
            )
        while not self._page._inner.wait_for_downloads_done(60000, False):
            pass
        return True

    def url_change(
        self,
        text: str,
        exclude: bool = False,
        timeout: float = 10.0,
    ) -> "WebPage | bool":
        return self._page if self._page._inner.wait_for_url_change(text, exclude, int(timeout * 1000)) else False

    def title_change(
        self,
        text: str,
        exclude: bool = False,
        timeout: float = 10.0,
    ) -> "WebPage | bool":
        return self._page if self._page._inner.wait_for_title_change(text, exclude, int(timeout * 1000)) else False

    def load_start(self, timeout: float = 10.0) -> bool:
        return self._page._inner.wait_for_load_start(int(timeout * 1000))

    def doc_loaded(self, timeout: float = 10.0) -> bool:
        return self._page._inner.wait_for_doc_loaded(int(timeout * 1000))

    def eles_loaded(
        self,
        locators: str | list[str] | tuple[str, ...] | set[str],
        timeout: float = 10.0,
        any_one: bool = False,
    ) -> bool:
        values = [locators] if isinstance(locators, str) else list(locators)
        return self._page._inner.wait_for_elements_loaded(values, int(timeout * 1000), any_one)

    def alert_closed(self, timeout: float = 10.0) -> "WebPage | bool":
        return self._page if self._page._inner.wait_for_alert_closed(int(timeout * 1000)) else False

    def ele_displayed(self, locator: str, timeout: float = 10.0) -> Any:
        if self._page._inner.wait_for_ele_displayed(locator, int(timeout * 1000)):
            try:
                return self._page.ele(locator)
            except Exception:
                return True
        return False

    def ele_hidden(self, locator: str, timeout: float = 10.0) -> Any:
        if self._page._inner.wait_for_ele_hidden(locator, int(timeout * 1000)):
            try:
                return self._page.ele(locator)
            except Exception:
                return True
        return False

    def ele_enabled(self, locator: str, timeout: float = 10.0) -> Any:
        if self._page._inner.wait_for_ele_enabled(locator, int(timeout * 1000)):
            try:
                return self._page.ele(locator)
            except Exception:
                return True
        return False

    def ele_deleted(self, locator: str, timeout: float = 10.0) -> Any:
        if self._page._inner.wait_for_ele_deleted(locator, int(timeout * 1000)):
            try:
                return self._page.ele(locator)
            except Exception:
                return True
        return False

    def ele_clickable(self, locator: str, timeout: float = 10.0) -> Any:
        if self._page._inner.wait_for_ele_clickable(locator, int(timeout * 1000)):
            try:
                return self._page.ele(locator)
            except Exception:
                return True
        return False
