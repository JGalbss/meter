"""SDK error type."""

from __future__ import annotations


class MeterError(Exception):
    """An error returned by the meter engine (carries the HTTP status and engine error code)."""

    def __init__(self, status: int, code: str, message: str) -> None:
        super().__init__(message)
        self.status = status
        self.code = code
