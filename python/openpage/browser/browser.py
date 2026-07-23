from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
from ..options import ChromiumOptions
from ..download import DownloadMission
from .wait import BrowserWait
from .states import BrowserStates
from .settings import BrowserSetter
from ..page import Page

class Browser:
    def __init__(self, inner: _openpage_rs.Browser) -> None:
        self._inner = inner
        self._wait: BrowserWait | None = None
        self._states: BrowserStates | None = None
        self._set: BrowserSetter | None = None
        self.timeout = 10.0

    @classmethod
    def launch(
        cls,
        options: ChromiumOptions | None = None,
        timeout: float | None = None,
    ) -> "Browser":
        options = options or ChromiumOptions()
        inner = _openpage_rs.Browser.launch(
            browser_path=options.browser_path,
            download_path=options.download_path,
            download_file_exists_mode=options.download_file_exists_mode,
            load_mode=options.load_mode,
            headless=options.headless_mode,
            user_data_dir=options.user_data_path,
            width=options.width,
            height=options.height,
            no_sandbox=options.no_sandbox_mode,
        )
        browser = cls(inner)
        if timeout is not None:
            browser.timeout = timeout
        return browser

    def new_page(self, url: str | None = None) -> "Page":
        page = Page(self._inner.new_page(url))
        page.browser = self
        page.timeout = self.timeout
        return page

    def get_page(self, target_id: str) -> "Page":
        page = Page(self._inner.get_page(target_id))
        page.browser = self
        page.timeout = self.timeout
        return page

    @property
    def tabs_count(self) -> int:
        return self._inner.tabs_count()

    @property
    def tab_ids(self) -> list[str]:
        return self._inner.tab_ids()

    @property
    def version(self) -> str:
        return self._inner.version()

    @property
    def set(self) -> "BrowserSetter":
        if self._set is None:
            self._set = BrowserSetter(self)
        return self._set

    @property
    def wait(self) -> "BrowserWait":
        if self._wait is None:
            self._wait = BrowserWait(self)
        return self._wait

    @property
    def states(self) -> "BrowserStates":
        if self._states is None:
            self._states = BrowserStates(self)
        return self._states

    @property
    def download_path(self) -> str | None:
        return self._inner.download_path()

    def set_download_path(self, path: str) -> None:
        self._inner.set_download_path(path)

    @property
    def download_file_exists_mode(self) -> str:
        return self._inner.download_file_exists_mode()

    def set_download_file_exists_mode(self, mode: str) -> None:
        self._inner.set_download_file_exists_mode(mode)

    def wait_for_download(self, filename: str | None = None, timeout: float = 10.0) -> str:
        return self._inner.wait_for_download(filename, int(timeout * 1000))

    def download_missions(self) -> list["DownloadMission"]:
        return [DownloadMission(item) for item in self._inner.download_missions()]

    def last_download(self) -> "DownloadMission | None":
        mission = self._inner.last_download()
        return None if mission is None else DownloadMission(mission)

    def close(self) -> None:
        self._inner.close()
