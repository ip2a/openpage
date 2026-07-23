from __future__ import annotations


class LoadModeSetter:
    def __init__(self, owner: Any) -> None:
        self._owner = owner

    def normal(self) -> None:
        self._owner._inner.set_load_mode("normal")

    def eager(self) -> None:
        self._owner._inner.set_load_mode("eager")

    def none(self) -> None:
        self._owner._inner.set_load_mode("none")


class BrowserSetter:
    def __init__(self, browser: Browser) -> None:
        self._browser = browser
        self._load_mode: LoadModeSetter | None = None

    @property
    def load_mode(self) -> LoadModeSetter:
        if self._load_mode is None:
            self._load_mode = LoadModeSetter(self._browser)
        return self._load_mode
