from __future__ import annotations

from .._native.openpage_rs import openpage_rs as _openpage_rs

class SessionElement:
    def __init__(self, inner: _openpage_rs.SessionElement) -> None:
        self._inner = inner

    @property
    def tag(self) -> str:
        return self._inner.tag()

    @property
    def text(self) -> str | None:
        return self._inner.text()

    @property
    def html(self) -> str | None:
        return self._inner.html()

    @property
    def inner_html(self) -> str | None:
        return self._inner.inner_html()

    @property
    def raw_text(self) -> str | None:
        return self._inner.raw_text()

    @property
    def attrs(self) -> dict[str, str]:
        return dict(self._inner.attrs())

    def attr(self, name: str) -> str | None:
        return self._inner.attr(name)

    def ele(self, locator: str) -> "SessionElement":
        return SessionElement(self._inner.find(locator))

    def eles(self, locator: str) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.find_all(locator)]

    def child(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.child(locator, index))

    def parent(self) -> "SessionElement":
        return SessionElement(self._inner.parent())

    def children(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.children(locator)]

    def prev(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.prev(locator, index))

    def next(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.next(locator, index))

    def before(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.before(locator, index))

    def after(self, locator: str | None = None, index: int = 1) -> "SessionElement":
        return SessionElement(self._inner.after(locator, index))

    def prevs(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.prevs(locator)]

    def nexts(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.nexts(locator)]

    def befores(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.befores(locator)]

    def afters(self, locator: str | None = None) -> list["SessionElement"]:
        return [SessionElement(item) for item in self._inner.afters(locator)]

    def s_ele(self, locator: str | None = None) -> "SessionElement":
        if locator is None:
            return self
        return self.ele(locator)

    def s_eles(self, locator: str) -> list["SessionElement"]:
        return self.eles(locator)
