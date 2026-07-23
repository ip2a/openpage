from __future__ import annotations


class ElementWait:
    def __init__(self, element: Element) -> None:
        self._element = element

    def displayed(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_displayed(int(timeout * 1000)) else False

    def hidden(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_hidden(int(timeout * 1000)) else False

    def enabled(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_enabled(int(timeout * 1000)) else False

    def disabled(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_disabled(int(timeout * 1000)) else False

    def deleted(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_deleted(int(timeout * 1000)) else False

    def clickable(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_clickable(int(timeout * 1000)) else False

    def has_rect(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_has_rect(int(timeout * 1000)) else False

    def covered(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_covered(int(timeout * 1000)) else False

    def not_covered(self, timeout: float = 10.0) -> "Element | bool":
        return self._element if self._element._inner.wait_until_not_covered(int(timeout * 1000)) else False

    def disabled_or_deleted(self, timeout: float = 10.0) -> "Element | bool":
        return (
            self._element
            if self._element._inner.wait_until_disabled_or_deleted(int(timeout * 1000))
            else False
        )

    def stop_moving(self, timeout: float = 10.0) -> "Element | bool":
        return (
            self._element
            if self._element._inner.wait_until_stop_moving(int(timeout * 1000))
            else False
        )
