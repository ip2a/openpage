from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
import json
from pathlib import Path
from typing import Any
from .wait import ElementWait
from .states import ElementStates

_CLICK_TO_DOWNLOAD_MARKER_ATTR = "data-openpage-click-download-marker"
_CLICK_TO_UPLOAD_MARKER_ATTR = "data-openpage-click-upload-marker"
_CLICK_FOR_NEW_TAB_MARKER_ATTR = "data-openpage-click-new-tab-marker"
_CLICK_MIDDLE_MARKER_ATTR = "data-openpage-click-middle-marker"
_NEXT = {"download": 0, "upload": 0, "new_tab": 0, "middle": 0}
def _json_string(value): return json.dumps(value)
def _marker(kind):
    _NEXT[kind] += 1
    label = {"new_tab": "new-tab"}.get(kind, kind)
    return f"openpage-click-{label}-{_NEXT[kind]}"
def _next_click_to_download_marker(): return _marker("download")
def _next_click_to_upload_marker(): return _marker("upload")
def _next_click_for_new_tab_marker(): return _marker("new_tab")
def _next_click_middle_marker(): return _marker("middle")
def _normalize_input_values(values):
    if isinstance(values, Path): return [str(values)]
    if isinstance(values, (list, tuple)):
        result=[]
        for item in values: result.extend(_normalize_input_values(item))
        return result
    return [str(values)]

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
