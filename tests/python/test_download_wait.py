from __future__ import annotations

import json
import importlib
import sys
import types
import unittest
from unittest import mock
from pathlib import Path


PYTHON_DIR = Path(__file__).resolve().parents[2] / "python"
if str(PYTHON_DIR) not in sys.path:
    sys.path.insert(0, str(PYTHON_DIR))


if "openpage_rs" not in sys.modules:
    sys.modules["openpage_rs"] = types.ModuleType("openpage_rs")


compat = importlib.import_module("openpage")


class _FakeMissionInner:
    def __init__(self) -> None:
        self.wait_calls: list[tuple[bool, int | None, bool]] = []

    def id(self) -> str:
        return "guid-1"

    def guid(self) -> str:
        return "guid-1"

    def tab_id(self) -> str:
        return "tab-1"

    def url(self) -> str:
        return "https://example.com/file"

    def folder(self) -> str:
        return "/tmp/downloads"

    def name(self) -> str:
        return "openpage.txt"

    def suggested_filename(self) -> str:
        return "openpage.txt"

    def tmp_path(self) -> str:
        return "/tmp/downloads/guid-1"

    def state(self) -> str:
        return "done"

    def received_bytes(self) -> int:
        return 16

    def total_bytes(self) -> int:
        return 16

    def rate(self) -> float:
        return 100.0

    def final_path(self) -> str:
        return "/tmp/downloads/openpage.txt"

    def is_done(self) -> bool:
        return True

    def wait(self, show: bool, timeout_ms: int | None, cancel_if_timeout: bool) -> str:
        self.wait_calls.append((show, timeout_ms, cancel_if_timeout))
        return "/tmp/downloads/openpage.txt"

    def cancel(self) -> None:
        return None


class _FakeReturnedPageInner:
    pass


class _FakeConsoleMessageInner:
    def __init__(
        self,
        text: str = "openpage-console",
        level: str = "info",
        source: str = "javascript",
    ) -> None:
        self._text = text
        self._level = level
        self._source = source

    def all_info(self) -> str:
        return json.dumps({"text": self._text, "level": self._level})

    def source(self) -> str:
        return self._source

    def level(self) -> str:
        return self._level

    def text(self) -> str:
        return self._text

    def body(self) -> str:
        return json.dumps(self._text)

    def url(self):
        return "https://example.com/app"

    def line(self):
        return 12

    def column(self):
        return 34


class _FakeConsoleInner:
    def __init__(self) -> None:
        self.start_calls = 0
        self.stop_calls = 0
        self.clear_calls = 0
        self.wait_calls: list[int | None] = []
        self.listening = False
        self.wait_result = _FakeConsoleMessageInner()
        self.messages_result = [_FakeConsoleMessageInner(), _FakeConsoleMessageInner()]
        self.wait_results: list[_FakeConsoleMessageInner | None] = []

    def start(self) -> None:
        self.start_calls += 1
        self.listening = True

    def stop(self) -> None:
        self.stop_calls += 1
        self.listening = False

    def wait(self, timeout_ms: int | None):
        self.wait_calls.append(timeout_ms)
        if self.wait_results:
            return self.wait_results.pop(0)
        return self.wait_result

    def is_listening(self) -> bool:
        return self.listening

    def clear(self) -> None:
        self.clear_calls += 1

    def messages(self):
        result = self.messages_result
        self.messages_result = []
        return result


class _FakeInner:
    def __init__(self) -> None:
        self.begin_calls: list[tuple[int, bool]] = []
        self.done_calls: list[tuple[int, bool]] = []
        self.done_results: list[bool] = []
        self.upload_wait_calls: list[int] = []
        self.upload_wait_result = True
        self.click_for_new_tab_calls: list[tuple[str, int | None, bool]] = []
        self.click_for_new_tab_result: _FakeReturnedPageInner | None = _FakeReturnedPageInner()
        self.click_middle_calls: list[tuple[str, int | None, bool]] = []
        self.click_middle_result: _FakeReturnedPageInner | None = _FakeReturnedPageInner()
        self.click_to_download_calls: list[
            tuple[str, str | None, str | None, str | None, bool, int | None, bool, bool]
        ] = []
        self.click_to_download_result = _FakeMissionInner()
        self.click_to_upload_calls: list[tuple[str, list[str], int | None, bool]] = []
        self.click_to_upload_result = True
        self.console_inner = _FakeConsoleInner()
        self.element_inner = _FakeElementInner()

    def wait_for_download_begin(self, timeout_ms: int, cancel_it: bool):
        self.begin_calls.append((timeout_ms, cancel_it))
        return _FakeMissionInner()

    def wait_for_downloads_done(self, timeout_ms: int, cancel_if_timeout: bool) -> bool:
        self.done_calls.append((timeout_ms, cancel_if_timeout))
        return self.done_results.pop(0)

    def click_to_download(
        self,
        locator: str,
        save_path: str | None,
        rename: str | None,
        suffix: str | None,
        suffix_specified: bool,
        timeout_ms: int | None,
        by_js: bool,
        new_tab: bool,
    ):
        self.click_to_download_calls.append(
            (
                locator,
                save_path,
                rename,
                suffix,
                suffix_specified,
                timeout_ms,
                by_js,
                new_tab,
            )
        )
        return self.click_to_download_result

    def click_to_upload(
        self,
        locator: str,
        files: list[str],
        timeout_ms: int | None,
        by_js: bool,
    ) -> bool:
        self.click_to_upload_calls.append((locator, files, timeout_ms, by_js))
        return self.click_to_upload_result

    def wait_for_upload_paths_inputted(self, timeout_ms: int) -> bool:
        self.upload_wait_calls.append(timeout_ms)
        return self.upload_wait_result

    def click_for_new_tab(
        self,
        locator: str,
        timeout_ms: int | None,
        by_js: bool,
    ):
        self.click_for_new_tab_calls.append((locator, timeout_ms, by_js))
        return self.click_for_new_tab_result

    def click_middle(
        self,
        locator: str,
        timeout_ms: int | None,
        get_tab: bool,
    ):
        self.click_middle_calls.append((locator, timeout_ms, get_tab))
        return self.click_middle_result if get_tab else None

    def wait_for(self, locator: str, timeout_ms: int):
        self.last_wait_for = (locator, timeout_ms)
        return self.element_inner

    def find(self, locator: str):
        self.last_find = locator
        return self.element_inner

    def find_all(self, locator: str):
        self.last_find_all = locator
        return [self.element_inner]

    def console(self):
        return self.console_inner


class _FakeElementInner:
    def __init__(self) -> None:
        self.run_js_calls: list[str] = []
        self.click_calls = 0
        self.input_calls: list[tuple[str, bool, bool]] = []
        self.input_keys_calls: list[tuple[list[str], bool, bool]] = []
        self.wait_until_stop_moving_calls: list[int] = []
        self.click_at_calls: list[tuple[float | None, float | None, str, int]] = []
        self.click_multi_calls: list[int] = []
        self.click_left_calls = 0
        self.click_middle_calls = 0
        self.click_right_calls = 0
        self.focus_calls = 0
        self.hover_calls: list[tuple[float | None, float | None]] = []
        self.drag_calls: list[tuple[float, float, float]] = []
        self.drag_to_calls: list[tuple[object, float]] = []

    def run_js(self, script: str) -> str:
        self.run_js_calls.append(script)
        return json.dumps(True)

    def click(self) -> None:
        self.click_calls += 1

    def input(self, text: str, clear: bool = False, by_js: bool = False) -> None:
        self.input_calls.append((text, clear, by_js))

    def input_keys(self, values: list[str], clear: bool = False, by_js: bool = False) -> None:
        self.input_keys_calls.append((values, clear, by_js))

    def wait_until_stop_moving(self, timeout_ms: int) -> bool:
        self.wait_until_stop_moving_calls.append(timeout_ms)
        return True

    def click_at(
        self,
        offset_x: float | None,
        offset_y: float | None,
        button: str,
        count: int,
    ) -> None:
        self.click_at_calls.append((offset_x, offset_y, button, count))

    def click_multi(self, times: int) -> None:
        self.click_multi_calls.append(times)

    def click_left(self) -> None:
        self.click_left_calls += 1

    def click_middle(self) -> None:
        self.click_middle_calls += 1

    def click_right(self) -> None:
        self.click_right_calls += 1

    def focus(self) -> None:
        self.focus_calls += 1

    def hover(self, offset_x: float | None = None, offset_y: float | None = None) -> None:
        self.hover_calls.append((offset_x, offset_y))

    def drag(self, offset_x: float, offset_y: float, duration: float) -> None:
        self.drag_calls.append((offset_x, offset_y, duration))

    def drag_to(self, target, duration: float) -> None:
        self.drag_to_calls.append((target, duration))


class _FakeElementOwner:
    def __init__(self) -> None:
        self.calls: list[tuple[str, object, object, object, object, object, object]] = []
        self.result = compat.DownloadMission(_FakeMissionInner())
        self.upload_calls: list[tuple[str, object, object]] = []
        self.new_tab_calls: list[tuple[str, object, object]] = []
        self.middle_calls: list[tuple[str, object]] = []

    def click_to_download(
        self,
        locator: str,
        save_path=None,
        rename=None,
        suffix=None,
        timeout=None,
        by_js=False,
        new_tab=False,
    ):
        self.calls.append((locator, save_path, rename, suffix, timeout, by_js, new_tab))
        return self.result

    def click_to_upload(self, locator: str, file_paths=None, by_js=False):
        self.upload_calls.append((locator, file_paths, by_js))
        return True

    def click_for_new_tab(self, locator: str, timeout=None, by_js=False):
        self.new_tab_calls.append((locator, timeout, by_js))
        return compat.Page(_FakeReturnedPageInner())

    def click_middle(self, locator: str, get_tab=True):
        self.middle_calls.append((locator, get_tab))
        return compat.Page(_FakeReturnedPageInner()) if get_tab else None


class _FakeOwner:
    def __init__(self, timeout: float = 10.0) -> None:
        self.timeout = timeout
        self._inner = _FakeInner()


class _FakeBrowserCompatInner:
    def __init__(self) -> None:
        self.begin_calls: list[tuple[int, bool]] = []

    def new_page(self, url):
        return _FakeInner()

    def get_page(self, target_id):
        return _FakeInner()

    def wait_for_download_begin(self, timeout_ms: int, cancel_it: bool):
        self.begin_calls.append((timeout_ms, cancel_it))
        return _FakeMissionInner()


class _FakeBrowserApi:
    @staticmethod
    def launch(**kwargs):
        return _FakeBrowserCompatInner()


class CompatDownloadWaitTestCase(unittest.TestCase):
    def test_browser_launch_timeout_sets_browser_and_new_page_defaults(self) -> None:
        with mock.patch.object(compat._openpage_rs, "Browser", _FakeBrowserApi, create=True):
            browser = compat.Browser.launch(timeout=4.5)
            page = browser.new_page()

        self.assertEqual(browser.timeout, 4.5)
        self.assertEqual(page.timeout, 4.5)
        self.assertIs(page.browser, browser)

    def test_browser_launch_timeout_is_used_by_browser_wait_download_begin(self) -> None:
        with mock.patch.object(compat._openpage_rs, "Browser", _FakeBrowserApi, create=True):
            browser = compat.Browser.launch(timeout=4.5)
            mission = browser.wait.download_begin(timeout=None, cancel_it=False)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(browser._inner.begin_calls, [(4500, False)])

    def test_browser_new_page_inherits_timeout_for_page_wait_download_begin(self) -> None:
        with mock.patch.object(compat._openpage_rs, "Browser", _FakeBrowserApi, create=True):
            browser = compat.Browser.launch(timeout=4.5)
            page = browser.new_page()
            mission = page.wait.download_begin(timeout=None, cancel_it=False)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(page.timeout, 4.5)
        self.assertEqual(page._inner.begin_calls, [(4500, False)])

    def test_browser_get_page_inherits_timeout_for_page_wait_download_begin(self) -> None:
        with mock.patch.object(compat._openpage_rs, "Browser", _FakeBrowserApi, create=True):
            browser = compat.Browser.launch(timeout=4.5)
            page = browser.get_page("tab-1")
            mission = page.wait.download_begin(timeout=None, cancel_it=False)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(page.timeout, 4.5)
        self.assertEqual(page._inner.begin_calls, [(4500, False)])

    def test_page_wait_download_begin_cancel_it_returns_info_dict(self) -> None:
        owner = _FakeOwner(timeout=2.5)
        data = compat.PageWait(owner).download_begin(timeout=1.5, cancel_it=True)

        self.assertIsInstance(data, dict)
        self.assertEqual(owner._inner.begin_calls, [(1500, True)])
        self.assertEqual(data["id"], "guid-1")
        self.assertEqual(data["tab_id"], "tab-1")
        self.assertEqual(data["name"], "openpage.txt")
        self.assertEqual(data["rate"], 100.0)

    def test_page_wait_download_begin_cancel_it_timeout_none_uses_owner_timeout(self) -> None:
        owner = _FakeOwner(timeout=2.5)
        data = compat.PageWait(owner).download_begin(timeout=None, cancel_it=True)

        self.assertIsInstance(data, dict)
        self.assertEqual(owner._inner.begin_calls, [(2500, True)])
        self.assertEqual(data["id"], "guid-1")
        self.assertEqual(data["tab_id"], "tab-1")
        self.assertEqual(data["name"], "openpage.txt")
        self.assertEqual(data["rate"], 100.0)

    def test_page_console_start_wait_stop_wraps_message(self) -> None:
        page = compat.Page(_FakeInner())

        self.assertFalse(page.console.listening)
        page.console.start()
        self.assertTrue(page.console.listening)
        message = page.console.wait(timeout=1.25)
        page.console.stop()

        self.assertIs(page.console, page.console)
        self.assertFalse(page.console.listening)
        self.assertEqual(page._inner.console_inner.start_calls, 1)
        self.assertEqual(page._inner.console_inner.stop_calls, 1)
        self.assertEqual(page._inner.console_inner.wait_calls, [1250])
        self.assertIsInstance(message, compat.ConsoleMessage)
        assert isinstance(message, compat.ConsoleMessage)
        self.assertEqual(message.text, "openpage-console")
        self.assertEqual(message.body, "openpage-console")
        self.assertEqual(message.level, "info")

    def test_page_console_clear_forwards_to_inner(self) -> None:
        page = compat.Page(_FakeInner())

        page.console.clear()

        self.assertEqual(page._inner.console_inner.clear_calls, 1)

    def test_page_console_messages_wrap_and_drain(self) -> None:
        page = compat.Page(_FakeInner())

        messages = page.console.messages
        messages_again = page.console.messages

        self.assertEqual([item.text for item in messages], ["openpage-console", "openpage-console"])
        self.assertEqual(messages_again, [])

    def test_page_console_steps_yield_until_timeout(self) -> None:
        page = compat.Page(_FakeInner())
        page._inner.console_inner.wait_results = [
            _FakeConsoleMessageInner("step-console"),
            None,
        ]

        steps = page.console.steps(timeout=1.25)
        first = next(steps)

        self.assertEqual(first.text, "step-console")
        with self.assertRaises(StopIteration):
            next(steps)
        self.assertEqual(page._inner.console_inner.wait_calls, [1250, 1250])

    def test_page_wait_download_begin_timeout_none_uses_owner_timeout(self) -> None:
        owner = _FakeOwner(timeout=2.5)
        mission = compat.PageWait(owner).download_begin(timeout=None, cancel_it=False)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(owner._inner.begin_calls, [(2500, False)])

    def test_page_wait_upload_paths_inputted_uses_owner_timeout(self) -> None:
        owner = _FakeOwner(timeout=2.5)

        self.assertTrue(compat.PageWait(owner).upload_paths_inputted())
        self.assertEqual(owner._inner.upload_wait_calls, [2500])

    def test_webpage_wait_download_begin_timeout_none_uses_owner_timeout(self) -> None:
        owner = _FakeOwner(timeout=3.5)
        mission = compat.WebPageWait(owner).download_begin(timeout=None, cancel_it=False)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(owner._inner.begin_calls, [(3500, False)])

    def test_webpage_wait_upload_paths_inputted_uses_owner_timeout(self) -> None:
        owner = _FakeOwner(timeout=3.5)

        self.assertTrue(compat.WebPageWait(owner).upload_paths_inputted())
        self.assertEqual(owner._inner.upload_wait_calls, [3500])

    def test_webpage_wait_download_begin_cancel_it_returns_info_dict(self) -> None:
        owner = _FakeOwner(timeout=3.5)
        data = compat.WebPageWait(owner).download_begin(timeout=None, cancel_it=True)

        self.assertIsInstance(data, dict)
        self.assertEqual(owner._inner.begin_calls, [(3500, True)])
        self.assertEqual(data["id"], "guid-1")
        self.assertEqual(data["tab_id"], "tab-1")
        self.assertEqual(data["name"], "openpage.txt")
        self.assertEqual(data["rate"], 100.0)

    def test_browser_wait_download_begin_timeout_none_uses_owner_timeout(self) -> None:
        owner = _FakeOwner(timeout=4.5)
        mission = compat.BrowserWait(owner).download_begin(timeout=None, cancel_it=False)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(owner._inner.begin_calls, [(4500, False)])

    def test_browser_wait_download_begin_cancel_it_returns_info_dict(self) -> None:
        owner = _FakeOwner(timeout=4.5)
        data = compat.BrowserWait(owner).download_begin(timeout=None, cancel_it=True)

        self.assertIsInstance(data, dict)
        self.assertEqual(owner._inner.begin_calls, [(4500, True)])
        self.assertEqual(data["id"], "guid-1")
        self.assertEqual(data["tab_id"], "tab-1")
        self.assertEqual(data["name"], "openpage.txt")
        self.assertEqual(data["rate"], 100.0)

    def test_page_click_to_download_wraps_mission_and_forwards_options(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        mission = page.click_to_download(
            "t:a",
            save_path="/tmp/dl",
            rename="renamed",
            suffix="txt",
            timeout=1.5,
        )

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", "/tmp/dl", "renamed", "txt", True, 1500, False, False)],
        )

    def test_page_click_to_upload_forwards_options_and_files(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        result = page.click_to_upload(
            "t:input",
            ["/tmp/first.txt", "/tmp/second.txt"],
            timeout=1.5,
            by_js=True,
        )

        self.assertTrue(result)
        self.assertEqual(
            inner.click_to_upload_calls,
            [("t:input", ["/tmp/first.txt", "/tmp/second.txt"], 1500, True)],
        )

    def test_page_click_to_upload_timeout_none_uses_owner_timeout(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        page.timeout = 2.5

        result = page.click_to_upload("t:input", ["/tmp/first.txt"])

        self.assertTrue(result)
        self.assertEqual(
            inner.click_to_upload_calls,
            [("t:input", ["/tmp/first.txt"], 2500, False)],
        )

    def test_page_click_for_new_tab_wraps_page_and_forwards_options(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        page.browser = object()
        page.timeout = 2.5

        new_page = page.click_for_new_tab("t:a", timeout=None, by_js=True)

        self.assertIsInstance(new_page, compat.Page)
        assert isinstance(new_page, compat.Page)
        self.assertIs(new_page.browser, page.browser)
        self.assertEqual(new_page.timeout, 2.5)
        self.assertEqual(inner.click_for_new_tab_calls, [("t:a", 2500, True)])

    def test_page_click_for_new_tab_returns_false_when_wait_times_out(self) -> None:
        inner = _FakeInner()
        inner.click_for_new_tab_result = None
        page = compat.Page(inner)

        new_page = page.click_for_new_tab("t:a")

        self.assertFalse(new_page)
        self.assertEqual(inner.click_for_new_tab_calls, [("t:a", 10000, False)])

    def test_page_click_middle_wraps_page_and_uses_owner_timeout(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        page.browser = object()
        page.timeout = 2.5

        new_page = page.click_middle("t:a")

        self.assertIsInstance(new_page, compat.Page)
        assert isinstance(new_page, compat.Page)
        self.assertIs(new_page.browser, page.browser)
        self.assertEqual(new_page.timeout, 2.5)
        self.assertEqual(inner.click_middle_calls, [("t:a", 2500, True)])

    def test_page_click_middle_get_tab_false_returns_none(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        result = page.click_middle("t:a", get_tab=False)

        self.assertIsNone(result)
        self.assertEqual(inner.click_middle_calls, [("t:a", 10000, False)])

    def test_element_click_to_download_requires_owner(self) -> None:
        with self.assertRaises(RuntimeError):
            compat.Element(_FakeElementInner()).click_to_download()

    def test_element_click_to_upload_requires_owner(self) -> None:
        with self.assertRaises(RuntimeError):
            compat.Element(_FakeElementInner()).click_to_upload("/tmp/first.txt")

    def test_element_click_for_new_tab_requires_owner(self) -> None:
        with self.assertRaises(RuntimeError):
            compat.Element(_FakeElementInner()).click_for_new_tab()

    def test_element_click_middle_get_tab_true_requires_owner(self) -> None:
        with self.assertRaises(RuntimeError):
            compat.Element(_FakeElementInner()).click_middle()

    def test_element_click_middle_get_tab_false_uses_inner_click(self) -> None:
        element = compat.Element(_FakeElementInner())

        result = element.click_middle(get_tab=False)

        self.assertIsNone(result)
        self.assertEqual(element._inner.click_middle_calls, 1)

    def test_page_element_click_to_upload_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:input").click_to_upload("/tmp/first.txt")

        self.assertEqual(len(inner.click_to_upload_calls), 1)
        locator, files, timeout_ms, by_js = inner.click_to_upload_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-upload-marker="openpage-click-upload-'))
        self.assertEqual(files, ["/tmp/first.txt"])
        self.assertEqual((timeout_ms, by_js), (10000, False))
        self.assertEqual(inner.last_wait_for, ("t:input", 10000))
        self.assertEqual(len(inner.element_inner.run_js_calls), 2)
        self.assertIn("setAttribute", inner.element_inner.run_js_calls[0])
        self.assertIn("removeAttribute", inner.element_inner.run_js_calls[1])

    def test_page_element_click_for_new_tab_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        page.browser = object()

        new_page = page.ele("t:a").click_for_new_tab(timeout=1.5, by_js=True)

        self.assertIsInstance(new_page, compat.Page)
        self.assertEqual(len(inner.click_for_new_tab_calls), 1)
        locator, timeout_ms, by_js = inner.click_for_new_tab_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-new-tab-marker="openpage-click-new-tab-'))
        self.assertEqual((timeout_ms, by_js), (1500, True))
        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(len(inner.element_inner.run_js_calls), 2)

    def test_page_element_click_middle_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        page.browser = object()

        new_page = page.ele("t:a").click_middle()

        self.assertIsInstance(new_page, compat.Page)
        self.assertEqual(len(inner.click_middle_calls), 1)
        locator, timeout_ms, get_tab = inner.click_middle_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-middle-marker="openpage-click-middle-'))
        self.assertEqual((timeout_ms, get_tab), (10000, True))
        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(len(inner.element_inner.run_js_calls), 2)

    def test_page_element_click_middle_get_tab_false_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        result = page.ele("t:a").click_middle(get_tab=False)

        self.assertIsNone(result)
        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(len(inner.click_middle_calls), 1)
        locator, timeout_ms, get_tab = inner.click_middle_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-middle-marker="openpage-click-middle-'))
        self.assertEqual((timeout_ms, get_tab), (10000, False))

    def test_page_element_click_to_download_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        mission = page.ele("t:a").click_to_download(new_tab=True)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(len(inner.click_to_download_calls), 1)
        locator, save_path, rename, suffix, suffix_specified, timeout_ms, by_js, new_tab = (
            inner.click_to_download_calls[0]
        )
        self.assertTrue(locator.startswith('css:[data-openpage-click-download-marker="openpage-click-download-'))
        self.assertEqual((save_path, rename, suffix, suffix_specified, timeout_ms, by_js, new_tab), (None, None, None, False, 10000, False, True))
        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(len(inner.element_inner.run_js_calls), 2)
        self.assertIn("setAttribute", inner.element_inner.run_js_calls[0])
        self.assertIn("removeAttribute", inner.element_inner.run_js_calls[1])

    def test_page_element_click_proxy_still_clicks_directly(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:a").click()

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(inner.element_inner.click_calls, 1)

    def test_element_click_proxy_at_forwards_offsets_button_and_count(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.click.at(12.5, -8.0, button="right", count=2)

        self.assertEqual(
            element._inner.click_at_calls,
            [(12.5, -8.0, "right", 2)],
        )

    def test_page_element_click_proxy_at_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:a").click.at(offset_x=5.0)

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(
            inner.element_inner.click_at_calls,
            [(5.0, None, "left", 1)],
        )

    def test_element_click_proxy_multi_forwards_times(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.click.multi(3)

        self.assertEqual(element._inner.click_multi_calls, [3])

    def test_page_element_click_proxy_multi_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:a").click.multi()

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(inner.element_inner.click_multi_calls, [2])

    def test_element_click_proxy_left_forwards_to_inner(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.click.left()

        self.assertEqual(element._inner.click_left_calls, 1)

    def test_page_element_click_proxy_left_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:a").click.left()

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(inner.element_inner.click_left_calls, 1)

    def test_element_click_proxy_middle_get_tab_false_forwards_to_inner(self) -> None:
        element = compat.Element(_FakeElementInner())

        result = element.click.middle(get_tab=False)

        self.assertIsNone(result)
        self.assertEqual(element._inner.click_middle_calls, 1)

    def test_page_element_click_proxy_middle_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        page.browser = object()

        new_page = page.ele("t:a").click.middle()

        self.assertIsInstance(new_page, compat.Page)
        self.assertEqual(len(inner.click_middle_calls), 1)
        locator, timeout_ms, get_tab = inner.click_middle_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-middle-marker="openpage-click-middle-'))
        self.assertEqual((timeout_ms, get_tab), (10000, True))

    def test_page_element_click_proxy_middle_get_tab_false_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        result = page.ele("t:a").click.middle(get_tab=False)

        self.assertIsNone(result)
        self.assertEqual(len(inner.click_middle_calls), 1)
        locator, timeout_ms, get_tab = inner.click_middle_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-middle-marker="openpage-click-middle-'))
        self.assertEqual((timeout_ms, get_tab), (10000, False))

    def test_element_click_proxy_right_forwards_to_inner(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.click.right()

        self.assertEqual(element._inner.click_right_calls, 1)

    def test_page_element_click_proxy_right_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:a").click.right()

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(inner.element_inner.click_right_calls, 1)

    def test_element_input_forwards_clear_and_by_js(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.input("openpage", clear=True, by_js=True)

        self.assertEqual(element._inner.input_calls, [("openpage", True, True)])

    def test_page_element_input_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:input").input("openpage", clear=True)

        self.assertEqual(inner.last_wait_for, ("t:input", 10000))
        self.assertEqual(inner.element_inner.input_calls, [("openpage", True, False)])

    def test_element_input_keys_flattens_nested_sequences(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.input((compat.Keys.CTRL_A, compat.Keys.DEL))

        self.assertEqual(
            element._inner.input_keys_calls,
            [([compat.Keys.CTRL_COMM, "a", "Delete"], False, False)],
        )

    def test_page_element_input_keys_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:input").input((compat.Keys.CTRL_A, compat.Keys.DEL), clear=True)

        self.assertEqual(inner.last_wait_for, ("t:input", 10000))
        self.assertEqual(
            inner.element_inner.input_keys_calls,
            [([compat.Keys.CTRL_COMM, "a", "Delete"], True, False)],
        )

    def test_element_wait_stop_moving_forwards_timeout_to_inner(self) -> None:
        element = compat.Element(_FakeElementInner())

        result = element.wait.stop_moving(timeout=1.25)

        self.assertIs(result, element)
        self.assertEqual(element._inner.wait_until_stop_moving_calls, [1250])

    def test_element_focus_forwards_to_inner(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.focus()

        self.assertEqual(element._inner.focus_calls, 1)

    def test_page_element_focus_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:a").focus()

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(inner.element_inner.focus_calls, 1)

    def test_element_hover_forwards_offsets_to_inner(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.hover(offset_x=5.0, offset_y=-3.0)

        self.assertEqual(element._inner.hover_calls, [(5.0, -3.0)])

    def test_page_element_hover_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:a").hover()

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(inner.element_inner.hover_calls, [(None, None)])

    def test_element_drag_forwards_args_to_inner(self) -> None:
        element = compat.Element(_FakeElementInner())

        element.drag(5.0, -3.0, 1.25)

        self.assertEqual(element._inner.drag_calls, [(5.0, -3.0, 1.25)])

    def test_page_element_drag_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:a").drag(5.0, -3.0, 1.25)

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(inner.element_inner.drag_calls, [(5.0, -3.0, 1.25)])

    def test_element_drag_to_unwraps_target_element(self) -> None:
        element = compat.Element(_FakeElementInner())
        target = compat.Element(_FakeElementInner())

        element.drag_to(target, 1.25)

        self.assertEqual(element._inner.drag_to_calls, [(target._inner, 1.25)])

    def test_page_element_drag_to_uses_wrapped_element(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        target = compat.Element(_FakeElementInner())

        page.ele("t:a").drag_to(target, 1.25)

        self.assertEqual(inner.last_wait_for, ("t:a", 10000))
        self.assertEqual(inner.element_inner.drag_to_calls, [(target._inner, 1.25)])

    def test_page_element_click_proxy_to_upload_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        page.ele("t:input").click.to_upload("/tmp/first.txt", by_js=True)

        self.assertEqual(len(inner.click_to_upload_calls), 1)
        locator, files, timeout_ms, by_js = inner.click_to_upload_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-upload-marker="openpage-click-upload-'))
        self.assertEqual(files, ["/tmp/first.txt"])
        self.assertEqual((timeout_ms, by_js), (10000, True))

    def test_page_element_click_proxy_for_new_tab_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        page.browser = object()

        new_page = page.ele("t:a").click.for_new_tab(timeout=1.5)

        self.assertIsInstance(new_page, compat.Page)
        self.assertEqual(len(inner.click_for_new_tab_calls), 1)
        locator, timeout_ms, by_js = inner.click_for_new_tab_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-new-tab-marker="openpage-click-new-tab-'))
        self.assertEqual((timeout_ms, by_js), (1500, False))

    def test_page_element_click_proxy_to_download_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        mission = page.ele("t:a").click.to_download(new_tab=True)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(len(inner.click_to_download_calls), 1)
        locator, save_path, rename, suffix, suffix_specified, timeout_ms, by_js, new_tab = (
            inner.click_to_download_calls[0]
        )
        self.assertTrue(locator.startswith('css:[data-openpage-click-download-marker="openpage-click-download-'))
        self.assertEqual((save_path, rename, suffix, suffix_specified, timeout_ms, by_js, new_tab), (None, None, None, False, 10000, False, True))

    def test_page_click_to_download_returns_false_when_wait_times_out(self) -> None:
        inner = _FakeInner()
        inner.click_to_download_result = None
        page = compat.Page(inner)

        mission = page.click_to_download("t:a", timeout=None)

        self.assertFalse(mission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", None, None, None, False, 10000, False, False)],
        )

    def test_page_click_to_download_forwards_by_js(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        mission = page.click_to_download("t:a", by_js=True)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", None, None, None, False, 10000, True, False)],
        )

    def test_page_click_to_download_timeout_none_uses_owner_timeout(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)
        page.timeout = 2.5

        mission = page.click_to_download("t:a", timeout=None)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", None, None, None, False, 2500, False, False)],
        )

    def test_page_click_to_download_forwards_new_tab(self) -> None:
        inner = _FakeInner()
        page = compat.Page(inner)

        mission = page.click_to_download("t:a", new_tab=True)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", None, None, None, False, 10000, False, True)],
        )

    def test_webpage_click_to_download_wraps_mission_and_forwards_options(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner

        mission = page.click_to_download(
            "t:a",
            save_path="/tmp/dl",
            rename="renamed",
            suffix="txt",
            timeout=1.5,
        )

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", "/tmp/dl", "renamed", "txt", True, 1500, False, False)],
        )

    def test_webpage_click_to_upload_forwards_options_and_files(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 3.5

        result = page.click_to_upload(
            "t:input",
            ["/tmp/first.txt", "/tmp/second.txt"],
            timeout=None,
            by_js=True,
        )

        self.assertTrue(result)
        self.assertEqual(
            inner.click_to_upload_calls,
            [("t:input", ["/tmp/first.txt", "/tmp/second.txt"], 3500, True)],
        )

    def test_webpage_click_for_new_tab_wraps_page_and_forwards_options(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 3.5

        new_page = page.click_for_new_tab("t:a", timeout=None, by_js=True)

        self.assertIsInstance(new_page, compat.Page)
        assert isinstance(new_page, compat.Page)
        self.assertEqual(new_page.timeout, 3.5)
        self.assertEqual(inner.click_for_new_tab_calls, [("t:a", 3500, True)])

    def test_webpage_click_for_new_tab_returns_false_when_wait_times_out(self) -> None:
        inner = _FakeInner()
        inner.click_for_new_tab_result = None
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner

        new_page = page.click_for_new_tab("t:a")

        self.assertFalse(new_page)
        self.assertEqual(inner.click_for_new_tab_calls, [("t:a", 10000, False)])

    def test_webpage_click_middle_wraps_page_and_uses_owner_timeout(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 3.5

        new_page = page.click_middle("t:a")

        self.assertIsInstance(new_page, compat.Page)
        assert isinstance(new_page, compat.Page)
        self.assertEqual(new_page.timeout, 3.5)
        self.assertEqual(inner.click_middle_calls, [("t:a", 3500, True)])

    def test_webpage_element_click_to_download_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner

        with mock.patch.object(compat._openpage_rs, "Element", _FakeElementInner, create=True):
            mission = page.ele("t:a").click_to_download(by_js=True)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(len(inner.click_to_download_calls), 1)
        locator, save_path, rename, suffix, suffix_specified, timeout_ms, by_js, new_tab = (
            inner.click_to_download_calls[0]
        )
        self.assertTrue(locator.startswith('css:[data-openpage-click-download-marker="openpage-click-download-'))
        self.assertEqual((save_path, rename, suffix, suffix_specified, timeout_ms, by_js, new_tab), (None, None, None, False, 10000, True, False))
        self.assertEqual(inner.last_find, "t:a")
        self.assertEqual(len(inner.element_inner.run_js_calls), 2)

    def test_webpage_element_click_to_upload_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 4.5

        with mock.patch.object(compat._openpage_rs, "Element", _FakeElementInner, create=True):
            page.ele("t:input").click_to_upload("/tmp/first.txt", by_js=True)

        self.assertEqual(len(inner.click_to_upload_calls), 1)
        locator, files, timeout_ms, by_js = inner.click_to_upload_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-upload-marker="openpage-click-upload-'))
        self.assertEqual(files, ["/tmp/first.txt"])
        self.assertEqual((timeout_ms, by_js), (4500, True))
        self.assertEqual(inner.last_find, "t:input")
        self.assertEqual(len(inner.element_inner.run_js_calls), 2)

    def test_webpage_element_click_for_new_tab_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 4.5

        with mock.patch.object(compat._openpage_rs, "Element", _FakeElementInner, create=True):
            new_page = page.ele("t:a").click_for_new_tab(by_js=True)

        self.assertIsInstance(new_page, compat.Page)
        self.assertEqual(len(inner.click_for_new_tab_calls), 1)
        locator, timeout_ms, by_js = inner.click_for_new_tab_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-new-tab-marker="openpage-click-new-tab-'))
        self.assertEqual((timeout_ms, by_js), (4500, True))
        self.assertEqual(inner.last_find, "t:a")
        self.assertEqual(len(inner.element_inner.run_js_calls), 2)

    def test_webpage_element_click_middle_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 4.5

        with mock.patch.object(compat._openpage_rs, "Element", _FakeElementInner, create=True):
            new_page = page.ele("t:a").click.middle()

        self.assertIsInstance(new_page, compat.Page)
        self.assertEqual(len(inner.click_middle_calls), 1)
        locator, timeout_ms, get_tab = inner.click_middle_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-middle-marker="openpage-click-middle-'))
        self.assertEqual((timeout_ms, get_tab), (4500, True))
        self.assertEqual(inner.last_find, "t:a")
        self.assertEqual(len(inner.element_inner.run_js_calls), 2)

    def test_webpage_element_click_proxy_to_download_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner

        with mock.patch.object(compat._openpage_rs, "Element", _FakeElementInner, create=True):
            mission = page.ele("t:a").click.to_download(by_js=True)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(len(inner.click_to_download_calls), 1)
        locator, save_path, rename, suffix, suffix_specified, timeout_ms, by_js, new_tab = (
            inner.click_to_download_calls[0]
        )
        self.assertTrue(locator.startswith('css:[data-openpage-click-download-marker="openpage-click-download-'))
        self.assertEqual((save_path, rename, suffix, suffix_specified, timeout_ms, by_js, new_tab), (None, None, None, False, 10000, True, False))

    def test_webpage_element_click_proxy_to_upload_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 4.5

        with mock.patch.object(compat._openpage_rs, "Element", _FakeElementInner, create=True):
            page.ele("t:input").click.to_upload("/tmp/first.txt")

        self.assertEqual(len(inner.click_to_upload_calls), 1)
        locator, files, timeout_ms, by_js = inner.click_to_upload_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-upload-marker="openpage-click-upload-'))
        self.assertEqual(files, ["/tmp/first.txt"])
        self.assertEqual((timeout_ms, by_js), (4500, False))

    def test_webpage_element_click_proxy_for_new_tab_delegates_via_marker(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 4.5

        with mock.patch.object(compat._openpage_rs, "Element", _FakeElementInner, create=True):
            new_page = page.ele("t:a").click.for_new_tab()

        self.assertIsInstance(new_page, compat.Page)
        self.assertEqual(len(inner.click_for_new_tab_calls), 1)
        locator, timeout_ms, by_js = inner.click_for_new_tab_calls[0]
        self.assertTrue(locator.startswith('css:[data-openpage-click-new-tab-marker="openpage-click-new-tab-'))
        self.assertEqual((timeout_ms, by_js), (4500, False))

    def test_webpage_click_to_download_returns_false_when_wait_times_out(self) -> None:
        inner = _FakeInner()
        inner.click_to_download_result = None
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner

        mission = page.click_to_download("t:a", timeout=None)

        self.assertFalse(mission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", None, None, None, False, 10000, False, False)],
        )

    def test_webpage_click_to_download_forwards_by_js(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner

        mission = page.click_to_download("t:a", by_js=True)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", None, None, None, False, 10000, True, False)],
        )

    def test_webpage_click_to_download_timeout_none_uses_owner_timeout(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner
        page.timeout = 3.5

        mission = page.click_to_download("t:a", timeout=None)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", None, None, None, False, 3500, False, False)],
        )

    def test_webpage_click_to_download_forwards_new_tab(self) -> None:
        inner = _FakeInner()
        page = compat.WebPage.__new__(compat.WebPage)
        page._inner = inner

        mission = page.click_to_download("t:a", new_tab=True)

        self.assertIsInstance(mission, compat.DownloadMission)
        self.assertEqual(
            inner.click_to_download_calls,
            [("t:a", None, None, None, False, 10000, False, True)],
        )

    def test_download_mission_wait_uses_dp_default_cancel_if_timeout_true(self) -> None:
        inner = _FakeMissionInner()

        result = compat.DownloadMission(inner).wait(show=False, timeout=1.5)

        self.assertEqual(result, "/tmp/downloads/openpage.txt")
        self.assertEqual(inner.wait_calls, [(False, 1500, True)])

    def test_page_wait_downloads_done_timeout_none_retries_until_true(self) -> None:
        owner = _FakeOwner()
        owner._inner.done_results = [False, True]

        done = compat.PageWait(owner).downloads_done(timeout=None, cancel_if_timeout=False)

        self.assertTrue(done)
        self.assertEqual(
            owner._inner.done_calls,
            [(60000, False), (60000, False)],
        )

    def test_page_wait_downloads_done_uses_dp_default_cancel_if_timeout_true(self) -> None:
        owner = _FakeOwner()
        owner._inner.done_results = [False]

        done = compat.PageWait(owner).downloads_done(timeout=1.5)

        self.assertFalse(done)
        self.assertEqual(owner._inner.done_calls, [(1500, True)])

    def test_webpage_wait_downloads_done_timeout_none_retries_until_true(self) -> None:
        owner = _FakeOwner()
        owner._inner.done_results = [False, True]

        done = compat.WebPageWait(owner).downloads_done(timeout=None, cancel_if_timeout=False)

        self.assertTrue(done)
        self.assertEqual(
            owner._inner.done_calls,
            [(60000, False), (60000, False)],
        )

    def test_webpage_wait_downloads_done_uses_dp_default_cancel_if_timeout_true(self) -> None:
        owner = _FakeOwner()
        owner._inner.done_results = [False]

        done = compat.WebPageWait(owner).downloads_done(timeout=1.5)

        self.assertFalse(done)
        self.assertEqual(owner._inner.done_calls, [(1500, True)])

    def test_browser_wait_downloads_done_timeout_none_retries_until_true(self) -> None:
        owner = _FakeOwner()
        owner._inner.done_results = [False, False, True]

        done = compat.BrowserWait(owner).downloads_done(timeout=None, cancel_if_timeout=False)

        self.assertTrue(done)
        self.assertEqual(
            owner._inner.done_calls,
            [(60000, False), (60000, False), (60000, False)],
        )

    def test_browser_wait_downloads_done_uses_dp_default_cancel_if_timeout_true(self) -> None:
        owner = _FakeOwner()
        owner._inner.done_results = [False]

        done = compat.BrowserWait(owner).downloads_done(timeout=1.5)

        self.assertFalse(done)
        self.assertEqual(owner._inner.done_calls, [(1500, True)])


if __name__ == "__main__":
    unittest.main()
