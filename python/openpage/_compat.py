from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any

import openpage_rs as _openpage_rs


@dataclass
class ChromiumOptions:
    browser_path: str | None = None
    download_path: str | None = None
    headless_mode: bool = True
    user_data_path: str | None = None
    width: int = 1280
    height: int = 900
    no_sandbox_mode: bool = False

    def set_browser_path(self, path: str) -> "ChromiumOptions":
        self.browser_path = path
        return self

    def set_user_data_path(self, path: str) -> "ChromiumOptions":
        self.user_data_path = path
        return self

    def set_download_path(self, path: str) -> "ChromiumOptions":
        self.download_path = path
        return self

    def headless(self, on_off: bool = True) -> "ChromiumOptions":
        self.headless_mode = on_off
        return self

    def set_window_size(self, width: int, height: int) -> "ChromiumOptions":
        self.width = width
        self.height = height
        return self

    def no_sandbox(self, on_off: bool = True) -> "ChromiumOptions":
        self.no_sandbox_mode = on_off
        return self


@dataclass
class SessionOptions:
    timeout_secs: int = 15
    user_agent: str | None = None

    def set_timeout(self, timeout_secs: int) -> "SessionOptions":
        self.timeout_secs = timeout_secs
        return self

    def set_user_agent(self, user_agent: str) -> "SessionOptions":
        self.user_agent = user_agent
        return self


class Browser:
    def __init__(self, inner: _openpage_rs.Browser) -> None:
        self._inner = inner

    @classmethod
    def launch(cls, options: ChromiumOptions | None = None) -> "Browser":
        options = options or ChromiumOptions()
        inner = _openpage_rs.Browser.launch(
            browser_path=options.browser_path,
            download_path=options.download_path,
            headless=options.headless_mode,
            user_data_dir=options.user_data_path,
            width=options.width,
            height=options.height,
            no_sandbox=options.no_sandbox_mode,
        )
        return cls(inner)

    def new_page(self, url: str | None = None) -> "Page":
        return Page(self._inner.new_page(url))

    def get_page(self, target_id: str) -> "Page":
        return Page(self._inner.get_page(target_id))

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
    def download_path(self) -> str | None:
        return self._inner.download_path()

    def set_download_path(self, path: str) -> None:
        self._inner.set_download_path(path)

    def wait_for_download(self, filename: str | None = None, timeout: float = 10.0) -> str:
        return self._inner.wait_for_download(filename, int(timeout * 1000))

    def close(self) -> None:
        self._inner.close()


class Page:
    def __init__(self, inner: _openpage_rs.Page) -> None:
        self._inner = inner

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

    def cookies(self) -> list[dict[str, str | None]]:
        return [
            {"name": name, "value": value, "domain": domain}
            for name, value, domain in self._inner.cookies()
        ]

    def run_js(self, expression: str) -> Any:
        return json.loads(self._inner.evaluate(expression))

    def evaluate(self, expression: str) -> Any:
        return self.run_js(expression)

    def s_ele(self, locator: str | None = None) -> "SessionElement":
        if locator is None:
            return SessionElement(self._inner.snapshot_root())
        return SessionElement(self._inner.snapshot_find(locator))

    def s_eles(self, locator: str) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.snapshot_find_all(locator)]

    def wait_for(self, locator: str, timeout: float = 10.0) -> "Element":
        return Element(self._inner.wait_for(locator, int(timeout * 1000)))

    def ele(self, locator: str, timeout: float = 10.0) -> "Element":
        return self.wait_for(locator, timeout)

    def eles(self, locator: str) -> list["Element"]:
        return [Element(item) for item in self._inner.find_all(locator)]

    def click(self, locator: str) -> None:
        self._inner.click(locator)

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


class ChromiumPage(Page):
    def __init__(self, addr_or_opts: ChromiumOptions | None = None, timeout: float | None = None) -> None:
        del timeout
        self.browser = Browser.launch(addr_or_opts)
        super().__init__(self.browser.new_page()._inner)

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

    def wait_for_download(self, filename: str | None = None, timeout: float = 10.0) -> str:
        return self.browser.wait_for_download(filename, timeout)


class SessionPage:
    def __init__(self, session_or_options: SessionOptions | None = None) -> None:
        options = session_or_options or SessionOptions()
        self._inner = _openpage_rs.SessionPage.create(
            timeout_secs=options.timeout_secs,
            user_agent=options.user_agent,
        )

    def get(self, url: str) -> bool:
        return self._inner.get(url)

    def post(self, url: str, payload: dict[str, Any] | None = None) -> bool:
        payload_json = json.dumps(payload) if payload is not None else None
        return self._inner.post_json(url, payload_json)

    @property
    def url(self) -> str | None:
        return self._inner.url()

    @property
    def status_code(self) -> int | None:
        return self._inner.status_code()

    @property
    def raw_data(self) -> bytes:
        return self._inner.raw_data()

    @property
    def encoding(self) -> str | None:
        return self._inner.encoding()

    @property
    def html(self) -> str:
        return self._inner.html()

    @property
    def json(self) -> Any | None:
        raw = self._inner.json()
        return json.loads(raw) if raw is not None else None

    @property
    def title(self) -> str | None:
        return self._inner.title()

    @property
    def user_agent(self) -> str | None:
        return self._inner.user_agent()

    def set_user_agent(self, user_agent: str | None) -> None:
        self._inner.set_user_agent(user_agent)

    def cookies(self) -> list[dict[str, str | None]]:
        return [
            {"name": name, "value": value, "domain": domain}
            for name, value, domain in self._inner.cookies()
        ]

    def ele(self, locator: str) -> "SessionElement":
        return SessionElement(self._inner.find(locator))

    def eles(self, locator: str) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.find_all(locator)]

    def s_ele(self, locator: str | None = None) -> "SessionElement":
        if locator is None:
            return SessionElement(self._inner.root())
        return self.ele(locator)

    def s_eles(self, locator: str) -> list["SessionElement"]:
        return self.eles(locator)

    def _cookie_header(self, url: str) -> str | None:
        return self._inner.cookie_header(url)

    def _set_cookie_header(self, url: str, cookie_header: str) -> None:
        self._inner.set_cookie_header(url, cookie_header)


class WebPage:
    def __init__(
        self,
        mode: str = "d",
        timeout: float | None = None,
        chromium_options: ChromiumOptions | None = None,
        session_or_options: SessionOptions | None = None,
    ) -> None:
        del timeout
        chromium_options = chromium_options or ChromiumOptions()
        session_options = session_or_options or SessionOptions()
        self._inner = _openpage_rs.WebPage.create(
            mode=mode.lower(),
            browser_path=chromium_options.browser_path,
            download_path=chromium_options.download_path,
            headless=chromium_options.headless_mode,
            user_data_dir=chromium_options.user_data_path,
            width=chromium_options.width,
            height=chromium_options.height,
            no_sandbox=chromium_options.no_sandbox_mode,
            timeout_secs=session_options.timeout_secs,
            user_agent=session_options.user_agent,
        )

    @property
    def mode(self) -> str:
        return self._inner.mode()

    def change_mode(self, mode: str | None = None, go: bool = True, copy_cookies: bool = True) -> None:
        normalized = mode.lower() if mode is not None else None
        self._inner.change_mode(normalized, go, copy_cookies)

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

    def ele(self, locator: str) -> Any:
        return _wrap_compat_element(self._inner.find(locator))

    def eles(self, locator: str) -> list[Any]:
        return [_wrap_compat_element(item) for item in self._inner.find_all(locator)]

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

    def wait_for_download(self, filename: str | None = None, timeout: float = 10.0) -> str:
        return self._inner.wait_for_download(filename, int(timeout * 1000))

    def quit(self) -> None:
        self._inner.quit()


class Element:
    def __init__(self, inner: _openpage_rs.Element) -> None:
        self._inner = inner

    def click(self) -> None:
        self._inner.click()

    def input(self, text: str) -> None:
        self._inner.input(text)

    def clear(self) -> None:
        self._inner.clear()

    def press(self, key: str) -> None:
        self._inner.press_key(key)

    @property
    def text(self) -> str | None:
        return self._inner.text()

    @property
    def html(self) -> str | None:
        return self._inner.html()

    def attr(self, name: str) -> str | None:
        return self._inner.attr(name)

    def run_js(self, script: str) -> Any:
        return json.loads(self._inner.run_js(script))

    def ele(self, locator: str) -> "Element":
        return Element(self._inner.find(locator))

    def eles(self, locator: str) -> list["Element"]:
        return [Element(item) for item in self._inner.find_all(locator)]

    def save_screenshot(self, path: str) -> None:
        self._inner.save_screenshot(path)


class SessionElement:
    def __init__(self, inner: _openpage_rs.SessionElement) -> None:
        self._inner = inner

    @property
    def tag(self) -> str:
        return self._inner.tag()

    @property
    def text(self) -> str | None:
        return self._inner.text()

    @property
    def html(self) -> str | None:
        return self._inner.html()

    @property
    def inner_html(self) -> str | None:
        return self._inner.inner_html()

    @property
    def raw_text(self) -> str | None:
        return self._inner.raw_text()

    @property
    def attrs(self) -> dict[str, str]:
        return dict(self._inner.attrs())

    def attr(self, name: str) -> str | None:
        return self._inner.attr(name)

    def ele(self, locator: str) -> "SessionElement":
        return SessionElement(self._inner.find(locator))

    def eles(self, locator: str) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.find_all(locator)]

    def child(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.child(locator, index))

    def parent(self) -> "SessionElement":
        return SessionElement(self._inner.parent())

    def children(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.children(locator)]

    def prev(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.prev(locator, index))

    def next(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.next(locator, index))

    def before(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.before(locator, index))

    def after(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.after(locator, index))

    def prevs(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.prevs(locator)]

    def nexts(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.nexts(locator)]

    def befores(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.befores(locator)]

    def afters(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.afters(locator)]

    def s_ele(self, locator: str | None = None) -> "SessionElement":
        if locator is None:
            return self
        return self.ele(locator)

    def s_eles(self, locator: str) -> list["SessionElement"]:
        return self.eles(locator)


def _wrap_compat_element(inner: Any) -> Any:
    if isinstance(inner, _openpage_rs.Element):
        return Element(inner)
    if isinstance(inner, _openpage_rs.SessionElement):
        return SessionElement(inner)
    return inner
