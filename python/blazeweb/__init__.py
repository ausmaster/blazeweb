"""blazeweb - A Rust-powered HTML + JavaScript execution engine."""

from __future__ import annotations

from blazeweb._blazeweb import Client as _Client
from blazeweb._blazeweb import fetch as _fetch
from blazeweb._blazeweb import render as _render

__all__ = ["render", "fetch", "RenderResult", "Client"]


class RenderResult(str):
    """Result of rendering HTML with JavaScript execution.

    Subclasses ``str`` so it works everywhere a string does (lxml, regex, etc.).
    Additionally exposes ``.html`` and ``.errors`` attributes.
    """

    errors: list[str]

    def __new__(cls, html: str, errors: list[str] | None = None) -> RenderResult:
        instance = super().__new__(cls, html)
        instance.errors = errors or []
        return instance

    @property
    def html(self) -> str:
        return str(self)

    def __repr__(self) -> str:
        trunc = str(self)[:60] + "..." if len(self) > 60 else str(self)
        if self.errors:
            return f"RenderResult(html='{trunc}', errors=[{len(self.errors)}])"
        return f"RenderResult(html='{trunc}')"


def render(
    html: bytes | str,
    *,
    base_url: str | None = None,
) -> RenderResult:
    """Render HTML, executing any inline and external JavaScript.

    Args:
        html: The HTML document to render. If str, encoded to UTF-8.
        base_url: Base URL for resolving relative script src attributes.

    Returns:
        RenderResult (str subclass) with `.html` and `.errors` attributes.
    """
    if isinstance(html, str):
        html = html.encode("utf-8")

    raw = _render(html, base_url=base_url)
    return RenderResult(raw.html, raw.errors)


def fetch(url: str) -> RenderResult:
    """Fetch a URL and render it with JavaScript execution.

    Fetches the HTML document at the given URL, then parses and executes
    any JavaScript. The final URL after redirects is used as the base URL
    for resolving relative resource paths.

    Args:
        url: The URL to fetch and render.

    Returns:
        RenderResult (str subclass) with ``.html`` and ``.errors`` attributes.
    """
    raw = _fetch(url)
    return RenderResult(raw.html, raw.errors)


class Client:
    """HTTP client with per-instance script cache for external script fetches.

    Each Client maintains its own cache. Cache behavior is controllable at
    the class level and per-render call.

    Args:
        cache: Master cache toggle (default True).
        cache_read: Whether to read from cache (default True).
        cache_write: Whether to write to cache (default True).
    """

    def __init__(
        self,
        *,
        cache: bool = True,
        cache_read: bool = True,
        cache_write: bool = True,
    ) -> None:
        self._inner = _Client(
            cache=cache, cache_read=cache_read, cache_write=cache_write,
        )

    def render(
        self,
        html: bytes | str,
        *,
        base_url: str | None = None,
        cache: bool | None = None,
        cache_read: bool | None = None,
        cache_write: bool | None = None,
    ) -> RenderResult:
        """Render HTML with JavaScript execution, using the script cache.

        Per-render kwargs override class-level settings.
        ``cache=False`` disables both read and write for this call.

        Args:
            html: The HTML document to render. If str, encoded to UTF-8.
            base_url: Base URL for resolving relative script src attributes.
            cache: Override master cache toggle for this call.
            cache_read: Override cache read for this call.
            cache_write: Override cache write for this call.

        Returns:
            RenderResult (str subclass) with `.html` and `.errors` attributes.
        """
        if isinstance(html, str):
            html = html.encode("utf-8")

        kwargs: dict = {"base_url": base_url}
        if cache is not None:
            kwargs["cache"] = cache
        if cache_read is not None:
            kwargs["cache_read"] = cache_read
        if cache_write is not None:
            kwargs["cache_write"] = cache_write

        raw = self._inner.render(html, **kwargs)
        return RenderResult(raw.html, raw.errors)

    def fetch(
        self,
        url: str,
        *,
        cache: bool | None = None,
        cache_read: bool | None = None,
        cache_write: bool | None = None,
    ) -> RenderResult:
        """Fetch a URL and render it, using the script cache.

        Per-fetch kwargs override class-level settings.
        ``cache=False`` disables both read and write for this call.

        Args:
            url: The URL to fetch and render.
            cache: Override master cache toggle for this call.
            cache_read: Override cache read for this call.
            cache_write: Override cache write for this call.

        Returns:
            RenderResult (str subclass) with ``.html`` and ``.errors`` attributes.
        """
        kwargs: dict = {}
        if cache is not None:
            kwargs["cache"] = cache
        if cache_read is not None:
            kwargs["cache_read"] = cache_read
        if cache_write is not None:
            kwargs["cache_write"] = cache_write

        raw = self._inner.fetch(url, **kwargs)
        return RenderResult(raw.html, raw.errors)

    def clear_cache(self) -> None:
        """Flush all cached scripts."""
        self._inner.clear_cache()

    @property
    def cache_size(self) -> int:
        """Number of scripts currently cached."""
        return self._inner.cache_size

    @property
    def cache(self) -> bool:
        """Master cache toggle."""
        return self._inner.cache

    @cache.setter
    def cache(self, value: bool) -> None:
        self._inner.cache = value

    @property
    def cache_read(self) -> bool:
        """Cache read toggle."""
        return self._inner.cache_read

    @cache_read.setter
    def cache_read(self, value: bool) -> None:
        self._inner.cache_read = value

    @property
    def cache_write(self) -> bool:
        """Cache write toggle."""
        return self._inner.cache_write

    @cache_write.setter
    def cache_write(self, value: bool) -> None:
        self._inner.cache_write = value
