from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
import json
from pathlib import Path
from typing import Any
from ..download import DownloadMission
from ..console import Console
from ..network import Listener, Interceptor
from ..element import Element, SessionElement
from .wait import PageWait
from .states import PageStates
from .settings import PageSetter

def _normalize_upload_files(values):
    if isinstance(values, str): return [str(Path(item).absolute()) for item in values.split("\n") if item]
    if isinstance(values, Path): return [str(values.absolute())]
    return [str(Path(item).absolute()) for item in values]

def _resolve_timeout_ms(owner, timeout):
    return int((timeout if timeout is not None else getattr(owner, "timeout", 10.0)) * 1000)

def _wrap_browser_page(inner, owner):
    page = Page(inner)
    browser = getattr(owner, "browser", None)
    if browser is not None: page.browser = browser
    page.timeout = getattr(owner, "timeout", 10.0)
    return page

class Page:
    def __init__(self, inner: _openpage_rs.Page) -> None:
        self._inner = inner
        self._console: Console | None = None
        self._listener: Listener | None = None
        self._interceptor: Interceptor | None = None
        self._wait: PageWait | None = None
        self._states: PageStates | None = None
        self._set: PageSetter | None = None
        self.timeout = 10.0

    def goto(self, url: str) -> None:
        self._inner.goto(url)

    def get(self, url: str) -> bool:
        self.goto(url)
        return True

    @property
    def url(self) -> str:
        return self._inner.url()

    @property
    def title(self) -> str:
        return self._inner.title()

    @property
    def tab_id(self) -> str:
        return self._inner.target_id()

    @property
    def html(self) -> str:
        return self._inner.html()

    @property
    def user_agent(self) -> str:
        return self._inner.user_agent()

    @property
    def wait(self) -> "PageWait":
        if self._wait is None:
            self._wait = PageWait(self)
        return self._wait

    @property
    def states(self) -> "PageStates":
        if self._states is None:
            self._states = PageStates(self)
        return self._states

    @property
    def set(self) -> "PageSetter":
        if self._set is None:
            self._set = PageSetter(self)
        return self._set

    def cookies(self) -> list[dict[str, str | None]]:
        return [
            {"name": name, "value": value, "domain": domain}
            for name, value, domain in self._inner.cookies()
        ]

    def run_js(self, expression: str) -> Any:
        return json.loads(self._inner.evaluate(expression))

    def evaluate(self, expression: str) -> Any:
        return self.run_js(expression)

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

    def s_ele(self, locator: str | None = None) -> "SessionElement":
        if locator is None:
            return SessionElement(self._inner.snapshot_root())
        return SessionElement(self._inner.snapshot_find(locator))

    def s_eles(self, locator: str) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.snapshot_find_all(locator)]

    def wait_for(self, locator: str, timeout: float = 10.0) -> "Element":
        return Element(self._inner.wait_for(locator, int(timeout * 1000)), owner=self)

    def ele(self, locator: str, timeout: float = 10.0) -> "Element":
        return self.wait_for(locator, timeout)

    def eles(self, locator: str) -> list["Element"]:
        return [Element(item, owner=self) for item in self._inner.find_all(locator)]

    def click(self, locator: str) -> None:
        self._inner.click(locator)

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

    def input(self, locator: str, text: str) -> None:
        self._inner.fill(locator, text)

    def text(self, locator: str) -> str | None:
        return self._inner.text(locator)

    def attr(self, locator: str, name: str) -> str | None:
        return self._inner.attr(locator, name)

    def save_screenshot(self, path: str, full_page: bool = True) -> None:
        self._inner.save_screenshot(path, full_page)

    def save_pdf(self, path: str) -> None:
        self._inner.save_pdf(path)

    def new_tab(self, url: str | None = None) -> "Page":
        if not hasattr(self, "browser"):
            raise RuntimeError("new_tab() is only available on ChromiumPage")
        return self.browser.new_page(url)

    def close(self) -> None:
        self._inner.close()
