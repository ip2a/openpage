from __future__ import annotations


class ChromiumOptions:
    browser_path: str | None = None
    download_path: str | None = None
    download_file_exists_mode: str = "rename"
    load_mode: str = "normal"
    headless_mode: bool = True
    user_data_path: str | None = None
    width: int = 1280
    height: int = 900
    no_sandbox_mode: bool = False

    def set_browser_path(self, path: str) -> "ChromiumOptions":
        self.browser_path = path
        return self

    def set_user_data_path(self, path: str) -> "ChromiumOptions":
        self.user_data_path = path
        return self

    def set_download_path(self, path: str) -> "ChromiumOptions":
        self.download_path = path
        return self

    def set_file_exists(self, mode: str) -> "ChromiumOptions":
        self.download_file_exists_mode = mode
        return self

    def set_load_mode(self, value: str) -> "ChromiumOptions":
        self.load_mode = value
        return self

    def headless(self, on_off: bool = True) -> "ChromiumOptions":
        self.headless_mode = on_off
        return self

    def set_window_size(self, width: int, height: int) -> "ChromiumOptions":
        self.width = width
        self.height = height
        return self

    def no_sandbox(self, on_off: bool = True) -> "ChromiumOptions":
        self.no_sandbox_mode = on_off
        return self
