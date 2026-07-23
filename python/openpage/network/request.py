from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
_UNSET = object()

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
