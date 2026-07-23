from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs

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
