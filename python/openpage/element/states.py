from __future__ import annotations


class ElementStates:
    def __init__(self, element: Element) -> None:
        self._element = element

    @property
    def is_selected(self) -> bool:
        return self._element._inner.is_selected()

    @property
    def is_checked(self) -> bool:
        return self._element._inner.is_checked()

    @property
    def is_displayed(self) -> bool:
        return self._element._inner.is_displayed()

    @property
    def is_enabled(self) -> bool:
        return self._element._inner.is_enabled()

    @property
    def is_alive(self) -> bool:
        return self._element._inner.is_alive()

    @property
    def has_rect(self) -> list[tuple[float, float]] | bool:
        return self._element._inner.has_rect() or False

    @property
    def is_in_viewport(self) -> bool:
        return self._element._inner.is_in_viewport()

    @property
    def is_whole_in_viewport(self) -> bool:
        return self._element._inner.is_whole_in_viewport()

    @property
    def is_covered(self) -> bool:
        return self._element._inner.is_covered()

    @property
    def is_clickable(self) -> bool:
        return self._element._inner.is_clickable()
