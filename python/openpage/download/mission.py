from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs

class DownloadMission:
    def __init__(self, inner: _openpage_rs.DownloadMission) -> None:
        self._inner = inner

    def __repr__(self) -> str:
        return (
            f'<DownloadMission guid="{self.guid}" state="{self.state}" '
            f'name="{self.suggested_filename}">'
        )

    @property
    def id(self) -> str:
        return self._inner.id()

    @property
    def guid(self) -> str:
        return self._inner.guid()

    @property
    def tab_id(self) -> str:
        return self._inner.tab_id()

    @property
    def url(self) -> str:
        return self._inner.url()

    @property
    def folder(self) -> str:
        return self._inner.folder()

    @property
    def name(self) -> str:
        return self._inner.name()

    @property
    def suggested_filename(self) -> str:
        return self._inner.suggested_filename()

    @property
    def tmp_path(self) -> str:
        return self._inner.tmp_path()

    @property
    def state(self) -> str:
        return self._inner.state()

    @property
    def received_bytes(self) -> int:
        return self._inner.received_bytes()

    @property
    def total_bytes(self) -> int | None:
        return self._inner.total_bytes()

    @property
    def rate(self) -> float | None:
        return self._inner.rate()

    @property
    def final_path(self) -> str | None:
        return self._inner.final_path()

    @property
    def is_done(self) -> bool:
        return self._inner.is_done()

    def wait(
        self,
        show: bool = True,
        timeout: float | None = None,
        cancel_if_timeout: bool = True,
    ) -> str | bool:
        timeout_ms = None if timeout is None else int(timeout * 1000)
        result = self._inner.wait(show, timeout_ms, cancel_if_timeout)
        return False if result is None else result

    def cancel(self) -> None:
        self._inner.cancel()
