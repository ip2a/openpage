from __future__ import annotations

from ..browser.settings import LoadModeSetter
from pathlib import Path

def _normalize_url_patterns(values):
    if values is None: return []
    if isinstance(values, str): return [values]
    return list(values)

def _normalize_upload_files(values):
    if isinstance(values, str): return [str(Path(item).absolute()) for item in values.split("\n") if item]
    if isinstance(values, Path): return [str(values.absolute())]
    return [str(Path(item).absolute()) for item in values]

from .window import WindowSetter

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
