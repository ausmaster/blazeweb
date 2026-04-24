"""Logging configuration for blazeweb.

Both the Python side AND the Rust side emit structured log records at the
standard levels: ``trace`` (Rust only) < ``debug`` < ``info`` < ``warn`` < ``error``.
Python's stdlib ``logging`` module doesn't have a TRACE level — Rust's ``trace``
calls map to our Python DEBUG when seen at the boundary, but in practice you
see Rust trace directly on stderr, not through Python logging.

Control:
    - ``BLAZEWEB_LOG`` env var, read at module import time, sets BOTH sides.
      Examples:
         BLAZEWEB_LOG=debug
         BLAZEWEB_LOG=trace                  # Rust + Python both max verbose
         BLAZEWEB_LOG='blazeweb::engine=trace,blazeweb::pool=debug,warn'
    - ``blazeweb.set_log_level("debug")`` — change at runtime (both sides).
      Note: Rust's env_logger filter-string syntax (``module::target=level``)
      can NOT be applied post-init via this function — it only sets a single
      max_level. For per-target filters, use ``BLAZEWEB_LOG`` before import.

Output format (both sides):
    HH:MM:SS.mmm [LEVEL] target: message

Rust and Python share stderr, so timestamps interleave correctly.
"""

from __future__ import annotations

import logging
import os
from typing import Union

# The top-level blazeweb logger. All Python-side log calls live under this
# hierarchy (``blazeweb.client``, ``blazeweb.config``, etc.).
logger = logging.getLogger("blazeweb")

_LEVEL_MAP = {
    "trace": logging.DEBUG,  # stdlib has no TRACE — map to DEBUG on the Python side
    "debug": logging.DEBUG,
    "info": logging.INFO,
    "warn": logging.WARNING,
    "warning": logging.WARNING,
    "error": logging.ERROR,
    "off": logging.CRITICAL + 1,
}


def _parse_level(level: Union[str, int]) -> int:
    if isinstance(level, int):
        return level
    first = level.split(",")[0].split("=")[-1].strip().lower()
    return _LEVEL_MAP.get(first, logging.WARNING)


def configure(level: Union[str, int, None] = None) -> None:
    """Configure blazeweb's Python-side logging.

    Called automatically at module import with the BLAZEWEB_LOG env var value
    (or "warn" if unset). Call again any time to change the level.

    If the root ``logging`` config has no handlers yet, installs a sensible
    default format with millisecond timestamps. If you've already configured
    ``logging`` yourself, we only adjust the ``blazeweb`` logger's level.
    """
    if level is None:
        level = os.environ.get("BLAZEWEB_LOG", "warn")
    numeric = _parse_level(level)

    if not logging.getLogger().handlers:
        logging.basicConfig(
            format="%(asctime)s.%(msecs)03d [%(levelname)s] %(name)s: %(message)s",
            datefmt="%H:%M:%S",
            level=logging.WARNING,
        )
    logger.setLevel(numeric)


def set_log_level(level: Union[str, int]) -> None:
    """Set the log level on BOTH Python and Rust sides at once.

    Accepts ``"trace"`` / ``"debug"`` / ``"info"`` / ``"warn"`` / ``"error"`` /
    ``"off"`` (case-insensitive) or a stdlib logging level int.

    Rust's set_max_level only accepts a single level — if you've pre-configured
    ``BLAZEWEB_LOG`` with per-module filters, calling this REPLACES that with a
    single global level.
    """
    configure(level)
    # Mirror on the Rust side.
    rust_level = level if isinstance(level, str) else {
        logging.DEBUG: "debug",
        logging.INFO: "info",
        logging.WARNING: "warn",
        logging.ERROR: "error",
    }.get(level, "warn")
    from blazeweb import _blazeweb

    _blazeweb._set_rust_log_level(str(rust_level).lower())


__all__ = ["logger", "configure", "set_log_level"]
