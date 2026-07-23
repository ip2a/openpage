from __future__ import annotations


class PageStates:
    def __init__(self, page: Page) -> None:
        self._page = page

    @property
    def ready_state(self) -> str:
        return self._page._inner.ready_state()

    @property
    def is_loading(self) -> bool:
        return self._page._inner.is_loading()

    @property
    def is_alive(self) -> bool:
        return self._page._inner.is_alive()

    @property
    def is_headless(self) -> bool:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("is_headless is only available on browser-backed pages")
        return browser._inner.is_headless()

    @property
    def has_alert(self) -> bool:
        return self._page._inner.has_alert()

    @property
    def is_existed(self) -> bool:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("is_existed is only available on browser-backed pages")
        return browser._inner.is_existed()

    @property
    def is_incognito(self) -> bool:
        browser = getattr(self._page, "browser", None)
        if browser is None:
            raise RuntimeError("is_incognito is only available on browser-backed pages")
        return browser._inner.is_incognito()


class WebPageStates:
    def __init__(self, page: WebPage) -> None:
        self._page = page

    @property
    def is_alive(self) -> bool:
        return self._page._inner.is_alive()

    @property
    def is_loading(self) -> bool:
        return self._page._inner.is_loading()

    @property
    def ready_state(self) -> str | None:
        return self._page._inner.ready_state()

    @property
    def is_headless(self) -> bool:
        return self._page._inner.is_headless()

    @property
    def has_alert(self) -> bool:
        return self._page._inner.has_alert()

    @property
    def is_existed(self) -> bool:
        return self._page._inner.is_existed()

    @property
    def is_incognito(self) -> bool:
        return self._page._inner.is_incognito()
