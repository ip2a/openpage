from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
from .response import ListenerResponse
from .request import ListenerRequest
from .failure import ListenerFailInfo
from ..options import SessionOptions
_UNSET = object()
def _normalize_listener_values(values):
    if values is None or values is True: return None
    if isinstance(values, str): return [values]
    return list(values)

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
