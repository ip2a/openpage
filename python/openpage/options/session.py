from __future__ import annotations


class SessionOptions:
    timeout_secs: int = 15
    user_agent: str | None = None

    def set_timeout(self, timeout_secs: int) -> "SessionOptions":
        self.timeout_secs = timeout_secs
        return self

    def set_user_agent(self, user_agent: str) -> "SessionOptions":
        self.user_agent = user_agent
        return self
