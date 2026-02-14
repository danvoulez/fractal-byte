"""UBL SDK error types."""

from __future__ import annotations

from typing import Any


class UBLError(Exception):
    """Base error for UBL API calls."""

    def __init__(self, message: str, status: int, body: Any = None):
        super().__init__(message)
        self.status = status
        self.body = body


class UBLConflictError(UBLError):
    """409 — duplicate execution (idempotency)."""

    def __init__(self, body: Any = None):
        super().__init__("Duplicate execution (idempotency)", 409, body)


class UBLAuthError(UBLError):
    """401/403 — authentication or authorization failure."""

    def __init__(self, status: int, body: Any = None):
        msg = "Unauthorized" if status == 401 else "Forbidden"
        super().__init__(msg, status, body)


class UBLRateLimitError(UBLError):
    """429 — rate limited."""

    def __init__(self, body: Any = None, retry_after: str | None = None):
        super().__init__("Rate limited", 429, body)
        self.retry_after: int | None = int(retry_after) if retry_after else None
