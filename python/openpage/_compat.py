from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any

import openpage_rs as _openpage_rs


@dataclass
class ChromiumOptions:
    browser_path: str | None = None
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


class Browser:
    def __init__(self, inner: _openpage_rs.Browser) -> None:
        self._inner = inner

    @classmethod
    def launch(cls, options: ChromiumOptions | None = None) -> "Browser":
        options = options or ChromiumOptions()
        inner = _openpage_rs.Browser.launch(
            browser_path=options.browser_path,
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

    def run_js(self, expression: str) -> Any:
        return json.loads(self._inner.evaluate(expression))

    def evaluate(self, expression: str) -> Any:
        return self.run_js(expression)

    def s_ele(self, locator: str) -> "Element":
        return self.ele(locator)

    def s_eles(self, locator: str) -> list["Element"]:
        return self.eles(locator)

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
