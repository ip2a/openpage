from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
from .listener import _normalize_listener_values

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
