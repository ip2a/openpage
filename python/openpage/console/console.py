from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
import json

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
