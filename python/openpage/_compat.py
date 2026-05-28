from __future__ import annotations

import importlib.machinery
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import openpage_rs as _openpage_rs
except ModuleNotFoundError as _openpage_rs_import_error:
    def _load_local_openpage_rs():
        root = Path(__file__).resolve().parents[2]
        candidates: list[Path] = []
        for profile in ("debug", "release"):
            target_dir = root / "rust" / "target" / profile
            candidates.extend(target_dir.glob("openpage_rs*.so"))
            candidates.extend(target_dir.glob("openpage_rs*.pyd"))
            candidates.extend(target_dir.glob("openpage_rs*.dylib"))
            candidates.extend(target_dir.glob("libopenpage_rs*.dylib"))
            deps_dir = target_dir / "deps"
            candidates.extend(deps_dir.glob("openpage_rs*.so"))
            candidates.extend(deps_dir.glob("openpage_rs*.pyd"))
            candidates.extend(deps_dir.glob("openpage_rs*.dylib"))
            candidates.extend(deps_dir.glob("libopenpage_rs*.dylib"))
        for path in candidates:
            loader = importlib.machinery.ExtensionFileLoader("openpage_rs", str(path))
            spec = importlib.machinery.ModuleSpec("openpage_rs", loader, origin=str(path))
            module = loader.create_module(spec)
            if module is None:
                continue
            sys.modules["openpage_rs"] = module
            try:
                loader.exec_module(module)
            except Exception:
                sys.modules.pop("openpage_rs", None)
                continue
            return module
        raise _openpage_rs_import_error

    _openpage_rs = _load_local_openpage_rs()

_UNSET = object()
_CLICK_TO_DOWNLOAD_MARKER_ATTR = "data-openpage-click-download-marker"
_CLICK_TO_UPLOAD_MARKER_ATTR = "data-openpage-click-upload-marker"
_CLICK_FOR_NEW_TAB_MARKER_ATTR = "data-openpage-click-new-tab-marker"
_CLICK_MIDDLE_MARKER_ATTR = "data-openpage-click-middle-marker"
_NEXT_CLICK_TO_DOWNLOAD_MARKER = 1
_NEXT_CLICK_TO_UPLOAD_MARKER = 1
_NEXT_CLICK_FOR_NEW_TAB_MARKER = 1
_NEXT_CLICK_MIDDLE_MARKER = 1
_CTRL_COMM_KEY = "Meta" if sys.platform == "darwin" else "Control"


class Keys:
    BACKSPACE = "Backspace"
    TAB = "Tab"
    ENTER = "Enter"
    RETURN = "Enter"
    SHIFT = "Shift"
    CONTROL = "Control"
    CTRL = "Control"
    ALT = "Alt"
    ESCAPE = "Escape"
    ESC = "Escape"
    SPACE = " "
    META = "Meta"
    COMMAND = "Meta"
    DELETE = "Delete"
    DEL = "Delete"

    CTRL_COMM = _CTRL_COMM_KEY
    CTRL_A = (CTRL_COMM, "a")
    CTRL_C = (CTRL_COMM, "c")
    CTRL_X = (CTRL_COMM, "x")
    CTRL_V = (CTRL_COMM, "v")
    CTRL_Z = (CTRL_COMM, "z")
    CTRL_Y = (CTRL_COMM, "y")


def _normalize_listener_values(
    values: str | list[str] | tuple[str, ...] | set[str] | bool | None,
) -> list[str] | None:
    if values is None:
        return None
    if values is True:
        return None
    if isinstance(values, str):
        return [values]
    return list(values)


def _normalize_url_patterns(
    values: str | list[str] | tuple[str, ...] | set[str] | None,
) -> list[str]:
    if values is None:
        return []
    if isinstance(values, str):
        return [values]
    return list(values)


def _normalize_upload_files(values: Any) -> list[str]:
    if isinstance(values, str):
        return [str(Path(item).absolute()) for item in values.split("\n") if item]
    if isinstance(values, Path):
        return [str(values.absolute())]
    return [str(Path(item).absolute()) for item in values]


def _normalize_input_values(values: Any) -> list[str]:
    if isinstance(values, Path):
        return [str(values)]
    if isinstance(values, (list, tuple)):
        result: list[str] = []
        for item in values:
            result.extend(_normalize_input_values(item))
        return result
    return [str(values)]


def _download_mission_to_dict(mission: "DownloadMission") -> dict[str, Any]:
    return {
        "url": mission.url,
        "tab_id": mission.tab_id,
        "id": mission.id,
        "guid": mission.guid,
        "folder": mission.folder,
        "name": mission.name,
        "suggested_filename": mission.suggested_filename,
        "tmp_path": mission.tmp_path,
        "state": mission.state,
        "total_bytes": mission.total_bytes,
        "received_bytes": mission.received_bytes,
        "final_path": mission.final_path,
        "rate": mission.rate,
        "is_done": mission.is_done,
    }


def _resolve_timeout_ms(owner: Any, timeout: float | None) -> int:
    if timeout is not None:
        return int(timeout * 1000)
    return int(getattr(owner, "timeout", 10.0) * 1000)


def _json_string(value: str) -> str:
    return json.dumps(value)


def _next_click_to_download_marker() -> str:
    global _NEXT_CLICK_TO_DOWNLOAD_MARKER
    marker = f"openpage-click-download-{_NEXT_CLICK_TO_DOWNLOAD_MARKER}"
    _NEXT_CLICK_TO_DOWNLOAD_MARKER += 1
    return marker


def _next_click_to_upload_marker() -> str:
    global _NEXT_CLICK_TO_UPLOAD_MARKER
    marker = f"openpage-click-upload-{_NEXT_CLICK_TO_UPLOAD_MARKER}"
    _NEXT_CLICK_TO_UPLOAD_MARKER += 1
    return marker


def _next_click_for_new_tab_marker() -> str:
    global _NEXT_CLICK_FOR_NEW_TAB_MARKER
    marker = f"openpage-click-new-tab-{_NEXT_CLICK_FOR_NEW_TAB_MARKER}"
    _NEXT_CLICK_FOR_NEW_TAB_MARKER += 1
    return marker


def _next_click_middle_marker() -> str:
    global _NEXT_CLICK_MIDDLE_MARKER
    marker = f"openpage-click-middle-{_NEXT_CLICK_MIDDLE_MARKER}"
    _NEXT_CLICK_MIDDLE_MARKER += 1
    return marker


@dataclass
class ChromiumOptions:
    browser_path: str | None = None
    download_path: str | None = None
    download_file_exists_mode: str = "rename"
    load_mode: str = "normal"
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

    def set_file_exists(self, mode: str) -> "ChromiumOptions":
        self.download_file_exists_mode = mode
        return self

    def set_load_mode(self, value: str) -> "ChromiumOptions":
        self.load_mode = value
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


class BrowserStates:
    def __init__(self, browser: Browser) -> None:
        self._browser = browser

    @property
    def is_alive(self) -> bool:
        return self._browser._inner.is_alive()

    @property
    def is_headless(self) -> bool:
        return self._browser._inner.is_headless()

    @property
    def is_existed(self) -> bool:
        return self._browser._inner.is_existed()

    @property
    def is_incognito(self) -> bool:
        return self._browser._inner.is_incognito()


class LoadModeSetter:
    def __init__(self, owner: Any) -> None:
        self._owner = owner

    def normal(self) -> None:
        self._owner._inner.set_load_mode("normal")

    def eager(self) -> None:
        self._owner._inner.set_load_mode("eager")

    def none(self) -> None:
        self._owner._inner.set_load_mode("none")


class BrowserSetter:
    def __init__(self, browser: Browser) -> None:
        self._browser = browser
        self._load_mode: LoadModeSetter | None = None

    @property
    def load_mode(self) -> LoadModeSetter:
        if self._load_mode is None:
            self._load_mode = LoadModeSetter(self._browser)
        return self._load_mode


def _wrap_browser_page(inner: _openpage_rs.Page, owner: Any) -> "Page":
    page = Page(inner)
    browser = getattr(owner, "browser", None)
    if browser is not None:
        page.browser = browser
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


class ChromiumPage(Page):
    def __init__(self, addr_or_opts: ChromiumOptions | None = None, timeout: float | None = None) -> None:
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


class PageSetter:
    def __init__(self, page: Page) -> None:
        self._page = page
        self._load_mode: LoadModeSetter | None = None

    @property
    def window(self) -> "WindowSetter":
        return WindowSetter(self._page)

    @property
    def load_mode(self) -> LoadModeSetter:
        if self._load_mode is None:
            self._load_mode = LoadModeSetter(self._page)
        return self._load_mode

    def blocked_urls(self, urls: str | list[str] | tuple[str, ...] | set[str] | None) -> None:
        self._page._inner.set_blocked_urls(_normalize_url_patterns(urls))

    def headers(self, headers: dict[str, str]) -> None:
        self._page._inner.set_headers(headers)

    def user_agent(self, ua: str, platform: str | None = None) -> None:
        self._page._inner.set_user_agent_override(ua, platform)

    def session_storage(self, item: str, value: str | bool | None) -> None:
        self._page._inner.set_session_storage(item, None if value is False else None if value is None else str(value))

    def local_storage(self, item: str, value: str | bool | None) -> None:
        self._page._inner.set_local_storage(item, None if value is False else None if value is None else str(value))

    def auto_handle_alert(
        self,
        on_off: bool | None = True,
        accept: bool = True,
        send: str | None = None,
    ) -> None:
        if on_off is None:
            self._page._inner.set_auto_alert_action(None, None)
        else:
            self._page._inner.set_auto_alert_action(accept if on_off else False, send)

    def download_path(self, path: str) -> None:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("download_path() is only available on browser-backed pages")
        browser._inner.set_page_download_path(self._page.tab_id, path)

    def download_file_exists(self, mode: str) -> None:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("download_file_exists() is only available on browser-backed pages")
        browser._inner.set_page_download_file_exists_mode(self._page.tab_id, mode)

    def download_file_name(self, name: str | None = None, suffix: str | None = None) -> None:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("download_file_name() is only available on browser-backed pages")
        kwargs = {"rename": name}
        if suffix is not None:
            kwargs["suffix"] = suffix
            kwargs["suffix_specified"] = True
        browser._inner.set_page_download_filename(self._page.tab_id, **kwargs)

    def upload_files(self, files: Any) -> None:
        self._page._inner.set_upload_files(_normalize_upload_files(files))

    def activate(self) -> None:
        self._page._inner.activate()


class ChromiumPageSetter(PageSetter):
    def __init__(self, page: ChromiumPage) -> None:
        super().__init__(page)
        self._page = page

    def download_path(self, path: str) -> None:
        self._page.browser._inner.set_page_download_path(self._page.tab_id, path)

    def download_file_exists(self, mode: str) -> None:
        self._page.browser._inner.set_page_download_file_exists_mode(self._page.tab_id, mode)

    def download_file_name(self, name: str | None = None, suffix: str | None = None) -> None:
        kwargs = {"rename": name}
        if suffix is not None:
            kwargs["suffix"] = suffix
            kwargs["suffix_specified"] = True
        self._page.browser._inner.set_page_download_filename(self._page.tab_id, **kwargs)


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


class PageStates:
    def __init__(self, page: Page) -> None:
        self._page = page

    @property
    def ready_state(self) -> str:
        return self._page._inner.ready_state()

    @property
    def is_loading(self) -> bool:
        return self._page._inner.is_loading()

    @property
    def is_alive(self) -> bool:
        return self._page._inner.is_alive()

    @property
    def is_headless(self) -> bool:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("is_headless is only available on browser-backed pages")
        return browser._inner.is_headless()

    @property
    def has_alert(self) -> bool:
        return self._page._inner.has_alert()

    @property
    def is_existed(self) -> bool:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("is_existed is only available on browser-backed pages")
        return browser._inner.is_existed()

    @property
    def is_incognito(self) -> bool:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("is_incognito is only available on browser-backed pages")
        return browser._inner.is_incognito()


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
        return _wrap_compat_element(self._inner.find(locator), owner=self)

    def eles(self, locator: str) -> list[Any]:
        return [_wrap_compat_element(item, owner=self) for item in self._inner.find_all(locator)]

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


class WebPageSetter:
    def __init__(self, page: WebPage) -> None:
        self._page = page
        self._load_mode: LoadModeSetter | None = None

    @property
    def window(self) -> "WindowSetter":
        return WindowSetter(self._page)

    @property
    def load_mode(self) -> LoadModeSetter:
        if self._load_mode is None:
            self._load_mode = LoadModeSetter(self._page)
        return self._load_mode

    def blocked_urls(self, urls: str | list[str] | tuple[str, ...] | set[str] | None) -> None:
        self._page._inner.set_blocked_urls(_normalize_url_patterns(urls))

    def headers(self, headers: dict[str, str]) -> None:
        self._page._inner.set_headers(headers)

    def user_agent(self, ua: str, platform: str | None = None) -> None:
        self._page._inner.set_user_agent_override(ua, platform)

    def session_storage(self, item: str, value: str | bool | None) -> None:
        self._page._inner.set_session_storage(item, None if value is False else None if value is None else str(value))

    def local_storage(self, item: str, value: str | bool | None) -> None:
        self._page._inner.set_local_storage(item, None if value is False else None if value is None else str(value))

    def auto_handle_alert(
        self,
        on_off: bool | None = True,
        accept: bool = True,
        send: str | None = None,
    ) -> None:
        if on_off is None:
            self._page._inner.set_auto_alert_action(None, None)
        else:
            self._page._inner.set_auto_alert_action(accept if on_off else False, send)

    def download_path(self, path: str) -> None:
        self._page._inner.set_current_tab_download_path(path)

    def download_file_exists(self, mode: str) -> None:
        self._page._inner.set_current_tab_download_file_exists_mode(mode)

    def download_file_name(self, name: str | None = None, suffix: str | None = None) -> None:
        kwargs = {"rename": name}
        if suffix is not None:
            kwargs["suffix"] = suffix
            kwargs["suffix_specified"] = True
        self._page._inner.set_current_tab_download_filename(**kwargs)

    def upload_files(self, files: Any) -> None:
        self._page._inner.set_upload_files(_normalize_upload_files(files))

    def activate(self) -> None:
        self._page._inner.activate()


class WebPageStates:
    def __init__(self, page: WebPage) -> None:
        self._page = page

    @property
    def is_alive(self) -> bool:
        return self._page._inner.is_alive()

    @property
    def is_loading(self) -> bool:
        return self._page._inner.is_loading()

    @property
    def ready_state(self) -> str | None:
        return self._page._inner.ready_state()

    @property
    def is_headless(self) -> bool:
        return self._page._inner.is_headless()

    @property
    def has_alert(self) -> bool:
        return self._page._inner.has_alert()

    @property
    def is_existed(self) -> bool:
        return self._page._inner.is_existed()

    @property
    def is_incognito(self) -> bool:
        return self._page._inner.is_incognito()


class WindowSetter:
    def __init__(self, page: Page | WebPage) -> None:
        self._page = page

    def max(self) -> None:
        self._page._inner.window_max()

    def mini(self) -> None:
        self._page._inner.window_min()

    def full(self) -> None:
        self._page._inner.window_full()

    def normal(self) -> None:
        self._page._inner.window_normal()

    def hide(self) -> None:
        self._page._inner.window_hide()

    def show(self) -> None:
        self._page._inner.window_show()

    def size(self, width: int | None = None, height: int | None = None) -> None:
        self._page._inner.window_size_set(width, height)

    def location(self, x: int | None = None, y: int | None = None) -> None:
        self._page._inner.window_location_set(x, y)


class DownloadMission:
    def __init__(self, inner: _openpage_rs.DownloadMission) -> None:
        self._inner = inner

    def __repr__(self) -> str:
        return (
            f'<DownloadMission guid="{self.guid}" state="{self.state}" '
            f'name="{self.suggested_filename}">'
        )

    @property
    def id(self) -> str:
        return self._inner.id()

    @property
    def guid(self) -> str:
        return self._inner.guid()

    @property
    def tab_id(self) -> str:
        return self._inner.tab_id()

    @property
    def url(self) -> str:
        return self._inner.url()

    @property
    def folder(self) -> str:
        return self._inner.folder()

    @property
    def name(self) -> str:
        return self._inner.name()

    @property
    def suggested_filename(self) -> str:
        return self._inner.suggested_filename()

    @property
    def tmp_path(self) -> str:
        return self._inner.tmp_path()

    @property
    def state(self) -> str:
        return self._inner.state()

    @property
    def received_bytes(self) -> int:
        return self._inner.received_bytes()

    @property
    def total_bytes(self) -> int | None:
        return self._inner.total_bytes()

    @property
    def rate(self) -> float | None:
        return self._inner.rate()

    @property
    def final_path(self) -> str | None:
        return self._inner.final_path()

    @property
    def is_done(self) -> bool:
        return self._inner.is_done()

    def wait(
        self,
        show: bool = True,
        timeout: float | None = None,
        cancel_if_timeout: bool = True,
    ) -> str | bool:
        timeout_ms = None if timeout is None else int(timeout * 1000)
        result = self._inner.wait(show, timeout_ms, cancel_if_timeout)
        return False if result is None else result

    def cancel(self) -> None:
        self._inner.cancel()


class ConsoleMessage:
    def __init__(self, inner: _openpage_rs.ConsoleMessage) -> None:
        self._inner = inner

    @property
    def all_info(self) -> Any:
        return json.loads(self._inner.all_info())

    @property
    def source(self) -> str:
        return self._inner.source()

    @property
    def level(self) -> str:
        return self._inner.level()

    @property
    def text(self) -> str:
        return self._inner.text()

    @property
    def body(self) -> Any:
        return json.loads(self._inner.body())

    @property
    def url(self) -> str | None:
        return self._inner.url()

    @property
    def line(self) -> int | None:
        return self._inner.line()

    @property
    def column(self) -> int | None:
        return self._inner.column()


class Console:
    def __init__(self, inner: _openpage_rs.Console) -> None:
        self._inner = inner

    def start(self) -> None:
        self._inner.start()

    def stop(self) -> None:
        self._inner.stop()

    def clear(self) -> None:
        self._inner.clear()

    def wait(self, timeout: float | None = None) -> "ConsoleMessage | bool":
        timeout_ms = None if timeout is None else int(timeout * 1000)
        message = self._inner.wait(timeout_ms)
        return False if message is None else ConsoleMessage(message)

    def steps(self, timeout: float | None = None):
        while True:
            try:
                message = self.wait(timeout=timeout)
            except RuntimeError:
                return
            if message is False:
                return
            yield message

    @property
    def listening(self) -> bool:
        return self._inner.is_listening()

    @property
    def messages(self) -> list["ConsoleMessage"]:
        return [ConsoleMessage(item) for item in self._inner.messages()]


class Listener:
    def __init__(self, inner: _openpage_rs.Listener) -> None:
        self._inner = inner

    def start(
        self,
        targets: str | list[str] | tuple[str, ...] | set[str] | bool | None = None,
        is_regex: bool = False,
        method: str | list[str] | tuple[str, ...] | set[str] | bool | None = None,
        res_type: str | list[str] | tuple[str, ...] | set[str] | bool | None = None,
    ) -> None:
        self._inner.start(
            _normalize_listener_values(targets),
            is_regex,
            _normalize_listener_values(method),
            _normalize_listener_values(res_type),
        )

    def set_targets(
        self,
        targets: str | list[str] | tuple[str, ...] | set[str] | bool | None = True,
        is_regex: bool = False,
        method: str | list[str] | tuple[str, ...] | set[str] | bool | None = True,
        res_type: str | list[str] | tuple[str, ...] | set[str] | bool | None = True,
    ) -> None:
        self._inner.set_targets(
            _normalize_listener_values(targets),
            is_regex,
            _normalize_listener_values(method),
            _normalize_listener_values(res_type),
        )

    def wait(
        self,
        count: int = 1,
        timeout: float | None = None,
        fit_count: bool = True,
    ) -> "ListenerPacket | list[ListenerPacket]":
        timeout_ms = None if timeout is None else int(timeout * 1000)
        packets = [ListenerPacket(item) for item in self._inner.wait(count, timeout_ms, fit_count)]
        return packets[0] if count == 1 else packets

    def steps(
        self,
        count: int | None = None,
        timeout: float | None = None,
        gap: int = 1,
    ):
        received = 0
        while count is None or received < count:
            batch = self.wait(count=gap, timeout=timeout, fit_count=True)
            yield batch
            received += gap

    def wait_silent(
        self,
        timeout: float | None = None,
        targets_only: bool = False,
    ) -> bool:
        timeout_ms = None if timeout is None else int(timeout * 1000)
        return self._inner.wait_until_idle(timeout_ms, targets_only)

    def clear(self) -> None:
        self._inner.clear()

    def pause(self, clear: bool = True) -> None:
        self._inner.pause(clear)

    def resume(self) -> None:
        self._inner.resume()

    def stop(self) -> None:
        self._inner.stop()

    @property
    def listening(self) -> bool:
        return self._inner.is_listening()


class Interceptor:
    def __init__(self, inner: _openpage_rs.Interceptor) -> None:
        self._inner = inner

    def start(
        self,
        targets: str | list[str] | tuple[str, ...] | set[str] | bool | None = None,
        is_regex: bool = False,
        method: str | list[str] | tuple[str, ...] | set[str] | bool | None = None,
        res_type: str | list[str] | tuple[str, ...] | set[str] | bool | None = None,
    ) -> None:
        self._inner.start(
            _normalize_listener_values(targets),
            is_regex,
            _normalize_listener_values(method),
            _normalize_listener_values(res_type),
        )

    def wait(self, timeout: float | None = None) -> "InterceptedRequest | bool":
        timeout_ms = None if timeout is None else int(timeout * 1000)
        request = self._inner.wait(timeout_ms)
        return False if request is None else InterceptedRequest(request)

    def stop(self) -> None:
        self._inner.stop()

    @property
    def listening(self) -> bool:
        return self._inner.is_listening()


class InterceptedRequest:
    def __init__(self, inner: _openpage_rs.InterceptedRequest) -> None:
        self._inner = inner

    @property
    def request_id(self) -> str:
        return self._inner.request_id()

    @property
    def frame_id(self) -> str:
        return self._inner.frame_id()

    @property
    def url(self) -> str:
        return self._inner.url()

    @property
    def method(self) -> str:
        return self._inner.method()

    @property
    def headers(self) -> dict[str, str]:
        return dict(self._inner.headers())

    @property
    def resource_type(self) -> str:
        return self._inner.resource_type()

    @property
    def has_post_data(self) -> bool:
        return self._inner.has_post_data()

    @property
    def post_data_entries(self) -> int:
        return self._inner.post_data_entries()

    def continue_request(
        self,
        url: str | None = None,
        method: str | None = None,
        headers: dict[str, str] | None = None,
        post_data: str | bytes | None = None,
    ) -> None:
        self._inner.continue_request(url, method, headers, post_data)

    def fail(self, reason: str = "BlockedByClient") -> None:
        self._inner.fail(reason)

    def fulfill(
        self,
        response_code: int = 200,
        body: str | bytes | None = None,
        headers: dict[str, str] | None = None,
        response_phrase: str | None = None,
        body_base64: bool = False,
    ) -> None:
        self._inner.fulfill(response_code, body, headers, response_phrase, body_base64)


class ListenerPacket:
    def __init__(self, inner: _openpage_rs.ListenerPacket) -> None:
        self._inner = inner
        self._request: ListenerRequest | None = None
        self._response: ListenerResponse | None | object = _UNSET
        self._fail_info: ListenerFailInfo | None | object = _UNSET

    def __repr__(self) -> str:
        return f'<ListenerPacket url="{self.url}" method="{self.method}" failed={self.is_failed}>'

    @property
    def target(self) -> str | None:
        return self._inner.target()

    @property
    def url(self) -> str:
        return self._inner.url()

    @property
    def method(self) -> str:
        return self._inner.method()

    @property
    def resource_type(self) -> str | None:
        return self._inner.resource_type()

    @property
    def is_failed(self) -> bool:
        return self._inner.is_failed()

    @property
    def request(self) -> "ListenerRequest":
        if self._request is None:
            self._request = ListenerRequest(self._inner.request())
        return self._request

    @property
    def response(self) -> "ListenerResponse | None":
        if self._response is _UNSET:
            response = self._inner.response()
            self._response = None if response is None else ListenerResponse(response)
        return self._response

    @property
    def fail_info(self) -> "ListenerFailInfo | None":
        if self._fail_info is _UNSET:
            fail_info = self._inner.fail_info()
            self._fail_info = None if fail_info is None else ListenerFailInfo(fail_info)
        return self._fail_info


class ListenerRequest:
    def __init__(self, inner: _openpage_rs.ListenerRequest) -> None:
        self._inner = inner
        self._extra_info: ListenerRequestExtraInfo | None | object = _UNSET

    @property
    def url(self) -> str:
        return self._inner.url()

    @property
    def method(self) -> str:
        return self._inner.method()

    @property
    def headers(self) -> dict[str, str]:
        return dict(self._inner.headers())

    @property
    def post_data(self) -> str | None:
        return self._inner.post_data()

    @property
    def extra_info(self) -> "ListenerRequestExtraInfo | None":
        if self._extra_info is _UNSET:
            extra_info = self._inner.extra_info()
            self._extra_info = None if extra_info is None else ListenerRequestExtraInfo(extra_info)
        return self._extra_info


class ListenerRequestExtraInfo:
    def __init__(self, inner: _openpage_rs.ListenerRequestExtraInfo) -> None:
        self._inner = inner

    @property
    def headers(self) -> dict[str, str]:
        return dict(self._inner.headers())


class ListenerResponse:
    def __init__(self, inner: _openpage_rs.ListenerResponse) -> None:
        self._inner = inner
        self._extra_info: ListenerResponseExtraInfo | None | object = _UNSET

    @property
    def url(self) -> str:
        return self._inner.url()

    @property
    def status(self) -> int:
        return self._inner.status()

    @property
    def status_text(self) -> str:
        return self._inner.status_text()

    @property
    def headers(self) -> dict[str, str]:
        return dict(self._inner.headers())

    @property
    def mime_type(self) -> str:
        return self._inner.mime_type()

    @property
    def body(self) -> str | None:
        return self._inner.body()

    @property
    def body_base64(self) -> bool:
        return self._inner.body_base64()

    @property
    def extra_info(self) -> "ListenerResponseExtraInfo | None":
        if self._extra_info is _UNSET:
            extra_info = self._inner.extra_info()
            self._extra_info = None if extra_info is None else ListenerResponseExtraInfo(extra_info)
        return self._extra_info


class ListenerResponseExtraInfo:
    def __init__(self, inner: _openpage_rs.ListenerResponseExtraInfo) -> None:
        self._inner = inner

    @property
    def headers(self) -> dict[str, str]:
        return dict(self._inner.headers())

    @property
    def status_code(self) -> int:
        return self._inner.status_code()

    @property
    def headers_text(self) -> str | None:
        return self._inner.headers_text()


class ListenerFailInfo:
    def __init__(self, inner: _openpage_rs.ListenerFailInfo) -> None:
        self._inner = inner

    @property
    def error_text(self) -> str:
        return self._inner.error_text()

    @property
    def canceled(self) -> bool | None:
        return self._inner.canceled()

    @property
    def blocked_reason(self) -> str | None:
        return self._inner.blocked_reason()


class Element:
    def __init__(self, inner: _openpage_rs.Element, owner: Any | None = None) -> None:
        self._inner = inner
        self._owner = owner
        self._click: _ElementClickProxy | None = None
        self._states: ElementStates | None = None
        self._wait: ElementWait | None = None

    def _click_direct(self) -> None:
        self._inner.click()

    def click_at(
        self,
        offset_x: float | None = None,
        offset_y: float | None = None,
        button: str = "left",
        count: int = 1,
    ) -> None:
        self._inner.click_at(offset_x, offset_y, button, count)

    def click_multi(self, times: int = 2) -> None:
        self._inner.click_multi(times)

    def click_left(self) -> None:
        self._inner.click_left()

    def click_middle(self) -> None:
        self._inner.click_middle()

    def click_right(self) -> None:
        self._inner.click_right()

    def input(self, vals: Any, clear: bool = False, by_js: bool = False) -> None:
        if isinstance(vals, (list, tuple)):
            self._inner.input_keys(_normalize_input_values(vals), clear, by_js)
            return
        self._inner.input(str(vals), clear, by_js)

    def clear(self) -> None:
        self._inner.clear()

    def focus(self) -> None:
        self._inner.focus()

    def hover(
        self,
        offset_x: float | None = None,
        offset_y: float | None = None,
    ) -> None:
        self._inner.hover(offset_x, offset_y)

    def drag(
        self,
        offset_x: float = 0.0,
        offset_y: float = 0.0,
        duration: float = 0.5,
    ) -> None:
        self._inner.drag(offset_x, offset_y, duration)

    def drag_to(self, ele_or_loc: Any, duration: float = 0.5) -> None:
        target = ele_or_loc._inner if isinstance(ele_or_loc, Element) else ele_or_loc
        self._inner.drag_to(target, duration)

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

    def click_to_download(
        self,
        save_path: str | None = None,
        rename: str | None = None,
        suffix: str | None = None,
        timeout: float | None = None,
        by_js: bool = False,
        new_tab: bool = False,
    ) -> "DownloadMission | bool":
        if self._owner is None or not hasattr(self._owner, "click_to_download"):
            raise RuntimeError(
                "click_to_download() requires an element from a browser-backed page"
            )
        marker = _next_click_to_download_marker()
        self.run_js(
            f"this.setAttribute({_json_string(_CLICK_TO_DOWNLOAD_MARKER_ATTR)}, {_json_string(marker)}); return true;"
        )
        locator = f'css:[{_CLICK_TO_DOWNLOAD_MARKER_ATTR}="{marker}"]'
        try:
            return self._owner.click_to_download(
                locator,
                save_path=save_path,
                rename=rename,
                suffix=suffix,
                timeout=timeout,
                by_js=by_js,
                new_tab=new_tab,
            )
        finally:
            try:
                self.run_js(
                    f"this.removeAttribute({_json_string(_CLICK_TO_DOWNLOAD_MARKER_ATTR)}); return true;"
                )
            except Exception:
                pass

    def click_to_upload(self, file_paths: Any, by_js: bool = False) -> None:
        if self._owner is None or not hasattr(self._owner, "click_to_upload"):
            raise RuntimeError(
                "click_to_upload() requires an element from a browser-backed page"
            )
        marker = _next_click_to_upload_marker()
        self.run_js(
            f"this.setAttribute({_json_string(_CLICK_TO_UPLOAD_MARKER_ATTR)}, {_json_string(marker)}); return true;"
        )
        locator = f'css:[{_CLICK_TO_UPLOAD_MARKER_ATTR}="{marker}"]'
        try:
            self._owner.click_to_upload(
                locator,
                file_paths,
                by_js=by_js,
            )
        finally:
            try:
                self.run_js(
                    f"this.removeAttribute({_json_string(_CLICK_TO_UPLOAD_MARKER_ATTR)}); return true;"
                )
            except Exception:
                pass

    def click_for_new_tab(
        self,
        timeout: float | None = None,
        by_js: bool = False,
    ) -> "Page | bool":
        if self._owner is None or not hasattr(self._owner, "click_for_new_tab"):
            raise RuntimeError(
                "click_for_new_tab() requires an element from a browser-backed page"
            )
        marker = _next_click_for_new_tab_marker()
        self.run_js(
            f"this.setAttribute({_json_string(_CLICK_FOR_NEW_TAB_MARKER_ATTR)}, {_json_string(marker)}); return true;"
        )
        locator = f'css:[{_CLICK_FOR_NEW_TAB_MARKER_ATTR}="{marker}"]'
        try:
            return self._owner.click_for_new_tab(
                locator,
                timeout=timeout,
                by_js=by_js,
            )
        finally:
            try:
                self.run_js(
                    f"this.removeAttribute({_json_string(_CLICK_FOR_NEW_TAB_MARKER_ATTR)}); return true;"
                )
            except Exception:
                pass

    def click_middle(self, get_tab: bool = True) -> "Page | bool | None":
        if self._owner is None or not hasattr(self._owner, "click_middle"):
            if not get_tab:
                self._inner.click_middle()
                return None
            raise RuntimeError(
                "click_middle(get_tab=True) requires an element from a browser-backed page"
            )
        marker = _next_click_middle_marker()
        self.run_js(
            f"this.setAttribute({_json_string(_CLICK_MIDDLE_MARKER_ATTR)}, {_json_string(marker)}); return true;"
        )
        locator = f'css:[{_CLICK_MIDDLE_MARKER_ATTR}="{marker}"]'
        try:
            return self._owner.click_middle(locator, get_tab=get_tab)
        finally:
            try:
                self.run_js(
                    f"this.removeAttribute({_json_string(_CLICK_MIDDLE_MARKER_ATTR)}); return true;"
                )
            except Exception:
                pass

    @property
    def click(self) -> "_ElementClickProxy":
        if self._click is None:
            self._click = _ElementClickProxy(self)
        return self._click

    @property
    def states(self) -> "ElementStates":
        if self._states is None:
            self._states = ElementStates(self)
        return self._states

    @property
    def wait(self) -> "ElementWait":
        if self._wait is None:
            self._wait = ElementWait(self)
        return self._wait

    def ele(self, locator: str) -> "Element":
        return Element(self._inner.find(locator), owner=self._owner)

    def eles(self, locator: str) -> list["Element"]:
        return [Element(item, owner=self._owner) for item in self._inner.find_all(locator)]

    def save_screenshot(self, path: str) -> None:
        self._inner.save_screenshot(path)


class ElementStates:
    def __init__(self, element: Element) -> None:
        self._element = element

    @property
    def is_selected(self) -> bool:
        return self._element._inner.is_selected()

    @property
    def is_checked(self) -> bool:
        return self._element._inner.is_checked()

    @property
    def is_displayed(self) -> bool:
        return self._element._inner.is_displayed()

    @property
    def is_enabled(self) -> bool:
        return self._element._inner.is_enabled()

    @property
    def is_alive(self) -> bool:
        return self._element._inner.is_alive()

    @property
    def has_rect(self) -> list[tuple[float, float]] | bool:
        return self._element._inner.has_rect() or False

    @property
    def is_in_viewport(self) -> bool:
        return self._element._inner.is_in_viewport()

    @property
    def is_whole_in_viewport(self) -> bool:
        return self._element._inner.is_whole_in_viewport()

    @property
    def is_covered(self) -> bool:
        return self._element._inner.is_covered()

    @property
    def is_clickable(self) -> bool:
        return self._element._inner.is_clickable()


class ElementWait:
    def __init__(self, element: Element) -> None:
        self._element = element

    def displayed(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_displayed(int(timeout * 1000)) else False

    def hidden(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_hidden(int(timeout * 1000)) else False

    def enabled(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_enabled(int(timeout * 1000)) else False

    def disabled(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_disabled(int(timeout * 1000)) else False

    def deleted(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_deleted(int(timeout * 1000)) else False

    def clickable(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_clickable(int(timeout * 1000)) else False

    def has_rect(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_has_rect(int(timeout * 1000)) else False

    def covered(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_covered(int(timeout * 1000)) else False

    def not_covered(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_not_covered(int(timeout * 1000)) else False

    def disabled_or_deleted(self, timeout: float = 10.0) -> "Element | bool":
        return (
            self._element
            if self._element._inner.wait_until_disabled_or_deleted(int(timeout * 1000))
            else False
        )

    def stop_moving(self, timeout: float = 10.0) -> "Element | bool":
        return (
            self._element
            if self._element._inner.wait_until_stop_moving(int(timeout * 1000))
            else False
        )


class _ElementClickProxy:
    def __init__(self, element: Element) -> None:
        self._element = element

    def __call__(self) -> None:
        self._element._click_direct()

    def at(
        self,
        offset_x: float | None = None,
        offset_y: float | None = None,
        button: str = "left",
        count: int = 1,
    ) -> None:
        self._element.click_at(offset_x, offset_y, button, count)

    def multi(self, times: int = 2) -> None:
        self._element.click_multi(times)

    def left(self) -> None:
        self._element.click_left()

    def middle(self, get_tab: bool = True) -> "Page | bool | None":
        return self._element.click_middle(get_tab=get_tab)

    def right(self) -> None:
        self._element.click_right()

    def to_download(
        self,
        save_path: str | None = None,
        rename: str | None = None,
        suffix: str | None = None,
        timeout: float | None = None,
        by_js: bool = False,
        new_tab: bool = False,
    ) -> "DownloadMission | bool":
        return self._element.click_to_download(
            save_path=save_path,
            rename=rename,
            suffix=suffix,
            timeout=timeout,
            by_js=by_js,
            new_tab=new_tab,
        )

    def to_upload(self, file_paths: Any, by_js: bool = False) -> None:
        self._element.click_to_upload(file_paths, by_js=by_js)

    def for_new_tab(
        self,
        timeout: float | None = None,
        by_js: bool = False,
    ) -> "Page | bool":
        return self._element.click_for_new_tab(timeout=timeout, by_js=by_js)


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


def _wrap_compat_element(inner: Any, owner: Any | None = None) -> Any:
    if isinstance(inner, _openpage_rs.Element):
        return Element(inner, owner=owner)
    if isinstance(inner, _openpage_rs.SessionElement):
        return SessionElement(inner)
    return inner
