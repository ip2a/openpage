from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
import json
from pathlib import Path
from typing import Any
from ..options import ChromiumOptions, SessionOptions
from ..download import DownloadMission
from ..console import Console
from ..network import Listener, Interceptor
from ..element import Element, SessionElement
from .wait import WebPageWait
from .states import WebPageStates
from .settings import WebPageSetter

def _normalize_upload_files(values):
    if isinstance(values, str): return [str(Path(item).absolute()) for item in values.split("\n") if item]
    if isinstance(values, Path): return [str(values.absolute())]
    return [str(Path(item).absolute()) for item in values]

def _resolve_timeout_ms(owner, timeout):
    return int((timeout if timeout is not None else getattr(owner, "timeout", 10.0)) * 1000)

def _wrap_browser_page(inner, owner):
    from .page import Page
    page = Page(inner); page.timeout = getattr(owner, "timeout", 10.0); return page

def _wrap_element(inner, owner):
    return Element(inner, owner=owner)

class WebPage:
    def __init__(
        self,
        mode: str = "d",
        timeout: float | None = None,
        chromium_options: ChromiumOptions | None = None,
        session_or_options: SessionOptions | None = None,
    ) -> None:
        chromium_options = chromium_options or ChromiumOptions()
        session_options = session_or_options or SessionOptions()
        self._inner = _openpage_rs.WebPage.create(
            mode=mode.lower(),
            browser_path=chromium_options.browser_path,
            download_path=chromium_options.download_path,
            download_file_exists_mode=chromium_options.download_file_exists_mode,
            load_mode=chromium_options.load_mode,
            headless=chromium_options.headless_mode,
            user_data_dir=chromium_options.user_data_path,
            width=chromium_options.width,
            height=chromium_options.height,
            no_sandbox=chromium_options.no_sandbox_mode,
            timeout_secs=session_options.timeout_secs,
            user_agent=session_options.user_agent,
        )
        self.timeout = 10.0 if timeout is None else timeout
        self._console: Console | None = None
        self._listener: Listener | None = None
        self._interceptor: Interceptor | None = None
        self._wait: WebPageWait | None = None
        self._states: WebPageStates | None = None
        self._set: WebPageSetter | None = None

    @property
    def mode(self) -> str:
        return self._inner.mode()

    @property
    def wait(self) -> "WebPageWait":
        if self._wait is None:
            self._wait = WebPageWait(self)
        return self._wait

    @property
    def states(self) -> "WebPageStates":
        if self._states is None:
            self._states = WebPageStates(self)
        return self._states

    @property
    def set(self) -> "WebPageSetter":
        if self._set is None:
            self._set = WebPageSetter(self)
        return self._set

    def change_mode(self, mode: str | None = None, go: bool = True, copy_cookies: bool = True) -> None:
        normalized = mode.lower() if mode is not None else None
        self._inner.change_mode(normalized, go, copy_cookies)

    def handle_alert(
        self,
        accept: bool = True,
        send: str | None = None,
        timeout: float = 10.0,
        next_one: bool = False,
    ) -> str | bool | None:
        if next_one:
            self._inner.set_next_alert_action(accept, send)
            return None
        result = self._inner.handle_alert(accept, send, int(timeout * 1000))
        return False if result is None else result

    def get(self, url: str) -> bool:
        return self._inner.get(url)

    @property
    def url(self) -> str | None:
        return self._inner.url()

    @property
    def title(self) -> str | None:
        return self._inner.title()

    @property
    def user_agent(self) -> str | None:
        return self._inner.user_agent()

    @property
    def html(self) -> str:
        return self._inner.html()

    @property
    def raw_data(self) -> bytes:
        return self._inner.raw_data()

    @property
    def encoding(self) -> str | None:
        return self._inner.encoding()

    @property
    def status_code(self) -> int | None:
        return self._inner.status_code()

    @property
    def json(self) -> Any | None:
        raw = self._inner.json()
        return json.loads(raw) if raw is not None else None

    @property
    def listen(self) -> "Listener":
        if self._listener is None:
            self._listener = Listener(self._inner.listener())
        return self._listener

    @property
    def console(self) -> "Console":
        if self._console is None:
            self._console = Console(self._inner.console())
        return self._console

    @property
    def intercept(self) -> "Interceptor":
        if self._interceptor is None:
            self._interceptor = Interceptor(self._inner.interceptor())
        return self._interceptor

    def ele(self, locator: str) -> Any:
        return _wrap_element(self._inner.find(locator), owner=self)

    def eles(self, locator: str) -> list[Any]:
        return [_wrap_element(item, owner=self) for item in self._inner.find_all(locator)]

    def s_ele(self, locator: str | None = None) -> "SessionElement":
        if locator is None:
            return SessionElement(self._inner.snapshot_root())
        return SessionElement(self._inner.snapshot_find(locator))

    def s_eles(self, locator: str) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.snapshot_find_all(locator)]

    def run_js(self, expression: str) -> Any:
        return json.loads(self._inner.run_js(expression))

    @property
    def tabs_count(self) -> int:
        return self._inner.tabs_count()

    @property
    def tab_ids(self) -> list[str]:
        return self._inner.tab_ids()

    @property
    def download_path(self) -> str | None:
        return self._inner.download_path()

    def cookies(self) -> list[dict[str, str | None]]:
        return [
            {"name": name, "value": value, "domain": domain}
            for name, value, domain in self._inner.cookies()
        ]

    def post(self, url: str, payload: dict[str, Any] | None = None) -> bool:
        payload_json = json.dumps(payload) if payload is not None else None
        return self._inner.post_json(url, payload_json)

    def cookies_to_session(self, copy_user_agent: bool = True) -> None:
        self._inner.cookies_to_session(copy_user_agent)

    def cookies_to_browser(self) -> None:
        self._inner.cookies_to_browser()

    def set_download_path(self, path: str) -> None:
        self._inner.set_download_path(path)

    @property
    def download_file_exists_mode(self) -> str:
        return self._inner.download_file_exists_mode()

    def set_download_file_exists_mode(self, mode: str) -> None:
        self._inner.set_download_file_exists_mode(mode)

    def wait_for_download(self, filename: str | None = None, timeout: float = 10.0) -> str:
        return self._inner.wait_for_download(filename, int(timeout * 1000))

    def click_to_download(
        self,
        locator: str,
        save_path: str | None = None,
        rename: str | None = None,
        suffix: str | None = None,
        timeout: float | None = None,
        by_js: bool = False,
        new_tab: bool = False,
    ) -> "DownloadMission | bool":
        mission = self._inner.click_to_download(
            locator,
            save_path,
            rename,
            suffix,
            suffix is not None,
            _resolve_timeout_ms(self, timeout),
            by_js,
            new_tab,
        )
        return False if mission is None else DownloadMission(mission)

    def click_to_upload(
        self,
        locator: str,
        file_paths: Any,
        timeout: float | None = None,
        by_js: bool = False,
    ) -> bool:
        return self._inner.click_to_upload(
            locator,
            _normalize_upload_files(file_paths),
            _resolve_timeout_ms(self, timeout),
            by_js,
        )

    def click_for_new_tab(
        self,
        locator: str,
        timeout: float | None = None,
        by_js: bool = False,
    ) -> "Page | bool":
        page = self._inner.click_for_new_tab(
            locator,
            _resolve_timeout_ms(self, timeout),
            by_js,
        )
        return False if page is None else _wrap_browser_page(page, self)

    def click_middle(
        self,
        locator: str,
        get_tab: bool = True,
    ) -> "Page | bool | None":
        page = self._inner.click_middle(
            locator,
            _resolve_timeout_ms(self, None),
            get_tab,
        )
        if not get_tab:
            return None
        return False if page is None else _wrap_browser_page(page, self)

    def download_missions(self) -> list["DownloadMission"]:
        return [DownloadMission(item) for item in self._inner.download_missions()]

    def last_download(self) -> "DownloadMission | None":
        mission = self._inner.last_download()
        return None if mission is None else DownloadMission(mission)

    def quit(self) -> None:
        self._inner.quit()
