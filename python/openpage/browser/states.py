from __future__ import annotations


class BrowserStates:
    def __init__(self, browser: Browser) -> None:
        self._browser = browser

    @property
    def is_alive(self) -> bool:
        return self._browser._inner.is_alive()

    @property
    def is_headless(self) -> bool:
        return self._browser._inner.is_headless()

    @property
    def is_existed(self) -> bool:
        return self._browser._inner.is_existed()

    @property
    def is_incognito(self) -> bool:
        return self._browser._inner.is_incognito()
