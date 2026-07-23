from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
_UNSET = object()

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
