from __future__ import annotations

from .page import Page
from .settings import ChromiumPageSetter
from ..options import ChromiumOptions

class ChromiumPage(Page):
    def __init__(self, addr_or_opts: ChromiumOptions | None = None, timeout: float | None = None) -> None:
        from ..browser import Browser
        self.browser = Browser.launch(addr_or_opts)
        super().__init__(self.browser.new_page()._inner)
        self.timeout = 10.0 if timeout is None else timeout
        self.browser.timeout = self.timeout

    def quit(self) -> None:
        self.browser.close()

    @property
    def tabs_count(self) -> int:
        return self.browser.tabs_count

    @property
    def tab_ids(self) -> list[str]:
        return self.browser.tab_ids

    def get_tab(self, target_id: str) -> "Page":
        return self.browser.get_page(target_id)

    @property
    def download_path(self) -> str | None:
        return self.browser.download_path

    def set_download_path(self, path: str) -> None:
        self.browser.set_download_path(path)

    @property
    def set(self) -> "ChromiumPageSetter":
        if self._set is None:
            self._set = ChromiumPageSetter(self)
        return self._set

    def wait_for_download(self, filename: str | None = None, timeout: float = 10.0) -> str:
        return self.browser.wait_for_download(filename, timeout)

    def download_missions(self) -> list["DownloadMission"]:
        return self.browser.download_missions()

    def last_download(self) -> "DownloadMission | None":
        return self.browser.last_download()
