from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs
import json
from ..options import SessionOptions
from ..element import SessionElement

class SessionPage:
    def __init__(self, session_or_options: SessionOptions | None = None) -> None:
        options = session_or_options or SessionOptions()
        self._inner = _openpage_rs.SessionPage.create(
            timeout_secs=options.timeout_secs,
            user_agent=options.user_agent,
        )

    def get(self, url: str) -> bool:
        return self._inner.get(url)

    def post(self, url: str, payload: dict[str, Any] | None = None) -> bool:
        payload_json = json.dumps(payload) if payload is not None else None
        return self._inner.post_json(url, payload_json)

    @property
    def url(self) -> str | None:
        return self._inner.url()

    @property
    def status_code(self) -> int | None:
        return self._inner.status_code()

    @property
    def raw_data(self) -> bytes:
        return self._inner.raw_data()

    @property
    def encoding(self) -> str | None:
        return self._inner.encoding()

    @property
    def html(self) -> str:
        return self._inner.html()

    @property
    def json(self) -> Any | None:
        raw = self._inner.json()
        return json.loads(raw) if raw is not None else None

    @property
    def title(self) -> str | None:
        return self._inner.title()

    @property
    def user_agent(self) -> str | None:
        return self._inner.user_agent()

    def set_user_agent(self, user_agent: str | None) -> None:
        self._inner.set_user_agent(user_agent)

    def cookies(self) -> list[dict[str, str | None]]:
        return [
            {"name": name, "value": value, "domain": domain}
            for name, value, domain in self._inner.cookies()
        ]

    def ele(self, locator: str) -> "SessionElement":
        return SessionElement(self._inner.find(locator))

    def eles(self, locator: str) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.find_all(locator)]

    def s_ele(self, locator: str | None = None) -> "SessionElement":
        if locator is None:
            return SessionElement(self._inner.root())
        return self.ele(locator)

    def s_eles(self, locator: str) -> list["SessionElement"]:
        return self.eles(locator)

    def _cookie_header(self, url: str) -> str | None:
        return self._inner.cookie_header(url)

    def _set_cookie_header(self, url: str, cookie_header: str) -> None:
        self._inner.set_cookie_header(url, cookie_header)
