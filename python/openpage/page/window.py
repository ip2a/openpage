from __future__ import annotations


class WindowSetter:
    def __init__(self, page: Page | WebPage) -> None:
        self._page = page

    def max(self) -> None:
        self._page._inner.window_max()

    def mini(self) -> None:
        self._page._inner.window_min()

    def full(self) -> None:
        self._page._inner.window_full()

    def normal(self) -> None:
        self._page._inner.window_normal()

    def hide(self) -> None:
        self._page._inner.window_hide()

    def show(self) -> None:
        self._page._inner.window_show()

    def size(self, width: int | None = None, height: int | None = None) -> None:
        self._page._inner.window_size_set(width, height)

    def location(self, x: int | None = None, y: int | None = None) -> None:
        self._page._inner.window_location_set(x, y)
