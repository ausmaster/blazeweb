"""blazeweb — URL → fully-rendered HTML (and/or screenshot) for Python.

Powered by Chromium via CDP. Under the hood it's a Rust/tokio-driven
chromiumoxide client speaking CDP directly to a bundled chrome-headless-shell.

Typical usage::

    import blazeweb

    # One-shot (uses a shared, process-wide default Client)
    html = blazeweb.fetch("https://example.com")
    png  = blazeweb.screenshot("https://example.com")
    both = blazeweb.fetch_all("https://example.com")

    # Explicit Client for batch / tuning
    with blazeweb.Client(concurrency=16) as client:
        for result in client.batch(urls, capture="both"):
            title = result.html.dom.title()
            ...

All HTML search (``.dom.query()``, ``.dom.find()``, etc.) runs in Rust for
speed; no Python HTML parsing round-trip.
"""

from __future__ import annotations

import os
import threading
from collections.abc import Iterable
from dataclasses import dataclass
from typing import Any, Literal, Protocol

from pydantic import BaseModel as _BaseModel

from blazeweb._blazeweb import (
    Client as _RustClient,
    Dom as Dom,
    Element as Element,
    _FetchOutput,
    _RenderOutput,
)
from blazeweb._logging import configure as _configure_logging, logger, set_log_level
from blazeweb.config import (
    ChromeConfig,
    Click,
    ClientConfig,
    EmulationConfig,
    FetchConfig,
    Fill,
    Hover,
    NetworkConfig,
    ScreenshotConfig,
    ScriptsConfig,
    TimeoutConfig,
    UserAgentBrandVersion,
    UserAgentMetadata,
    ViewportConfig,
    Wait,
)

# Configure Python-side logging at import from BLAZEWEB_LOG (defaults "warn").
# The Rust side reads the same env var at PyO3 module init.
_configure_logging()

_client_log = logger.getChild("client")

__all__ = [
    # Module-level convenience — sync
    "fetch",
    "screenshot",
    "fetch_all",
    # Module-level convenience — async
    "afetch",
    "ascreenshot",
    "afetch_all",
    # Classes
    "AsyncClient",
    "Click",
    "Client",
    "ConsoleMessage",
    "Dom",
    "Element",
    "FetchResult",
    "Fill",
    "Hover",
    "RenderResult",
    "Wait",
    # Configs (re-exported from blazeweb.config)
    "ClientConfig",
    "FetchConfig",
    "ScreenshotConfig",
    "ScriptsConfig",
    "ViewportConfig",
    "NetworkConfig",
    "EmulationConfig",
    "TimeoutConfig",
    "ChromeConfig",
    "UserAgentBrandVersion",
    "UserAgentMetadata",
    # Logging
    "logger",
    "set_log_level",
]


# Ensure the Rust side can locate the bundled chrome binary by pointing at this
# package's installed directory.
os.environ.setdefault(
    "BLAZEWEB_PKG_DIR",
    os.path.dirname(os.path.abspath(__file__)),
)


# ----------------------------------------------------------------------------
# Result types
# ----------------------------------------------------------------------------


@dataclass(frozen=True)
class ConsoleMessage:
    """One ``console.*`` event captured during a page visit.

    Attributes:
        type: The console method that fired —
            ``"log"`` / ``"info"`` / ``"warning"`` / ``"error"`` /
            ``"debug"`` / ``"trace"``.
        text: The rendered message body (chrome stringifies any non-string
            arguments before delivering the event).
        timestamp: ``time.time()`` (seconds since epoch) at the moment the
            event was captured by blazeweb.
    """

    type: Literal["log", "info", "warning", "error", "debug", "trace"]
    text: str
    timestamp: float


def _make_render_result(raw: _RenderOutput | _FetchOutput) -> RenderResult:
    """Build a ``RenderResult`` from a Rust raw output.

    Accepts both ``_RenderOutput`` and ``_FetchOutput`` via duck typing —
    each carries the same ``html`` / ``console_messages`` / ``final_url`` /
    ``status_code`` / ``elapsed_s`` / ``make_dom()`` shape. ``errors`` is
    derived from ``console_messages`` for backward compatibility.
    """
    console_messages = [
        ConsoleMessage(type=m.type, text=m.text, timestamp=m.timestamp)
        for m in raw.console_messages
    ]
    errors = [m.text for m in console_messages if m.type == "error"]
    return RenderResult(
        raw.html,
        errors=errors,
        console_messages=console_messages,
        final_url=raw.final_url,
        status_code=raw.status_code,
        elapsed_s=raw.elapsed_s,
        _raw=raw,
    )


class RenderResult(str):
    """Fully-rendered post-JS HTML. Subclasses ``str`` (lxml, regex, BS4 work).

    Adds:
      - ``.errors`` — list[str] of console errors and load errors
      - ``.console_messages`` — list[ConsoleMessage] captured during the visit
      - ``.final_url`` — URL after any redirects
      - ``.status_code`` — final HTTP status
      - ``.elapsed_s`` — end-to-end page-visit time (seconds)
      - ``.dom`` — Rust-side HTML query (lazy; CSS selectors + BS4-like find)
    """

    errors: list[str]
    console_messages: list[ConsoleMessage]
    final_url: str
    status_code: int
    elapsed_s: float

    def __new__(
        cls,
        html: str,
        *,
        errors: list[str] | None = None,
        console_messages: list[ConsoleMessage] | None = None,
        final_url: str = "",
        status_code: int = 0,
        elapsed_s: float = 0.0,
        _raw: _RenderOutput | None = None,
    ) -> RenderResult:
        """Construct a RenderResult; ``_raw`` is internal (Rust output object)."""
        instance = super().__new__(cls, html)
        instance.errors = errors or []
        instance.console_messages = console_messages or []
        instance.final_url = final_url
        instance.status_code = status_code
        instance.elapsed_s = elapsed_s
        instance._raw = _raw  # type: ignore[attr-defined]
        instance._dom = None  # type: ignore[attr-defined]
        return instance

    @property
    def html(self) -> str:
        """The raw rendered HTML as a plain ``str`` (same as ``str(self)``)."""
        return str(self)

    @property
    def dom(self) -> Dom:
        """Rust-parsed DOM (lazy). First access triggers html5ever parse."""
        if self._dom is None:  # type: ignore[attr-defined]
            raw = self._raw  # type: ignore[attr-defined]
            if raw is None:
                raise AttributeError(
                    "this RenderResult was not produced by blazeweb; .dom unavailable"
                )
            object.__setattr__(self, "_dom", raw.make_dom())
        return self._dom  # type: ignore[attr-defined]

    def __repr__(self) -> str:
        trunc = str(self)[:60] + "…" if len(self) > 60 else str(self)
        parts = [f"html={trunc!r}"]
        if self.final_url:
            parts.append(f"final_url={self.final_url!r}")
        if self.errors:
            parts.append(f"errors=[{len(self.errors)}]")
        return f"RenderResult({', '.join(parts)})"


class FetchResult:
    """HTML + PNG from one page visit. Use when you want both."""

    __slots__ = ("html", "png", "_raw")

    html: RenderResult
    png: bytes
    _raw: _FetchOutput

    def __init__(self, raw: _FetchOutput) -> None:
        self._raw = raw
        self.html = _make_render_result(raw)
        self.png = bytes(raw.png)

    @property
    def errors(self) -> list[str]:
        """Error texts (derived from ``console_messages``)."""
        return self.html.errors

    @property
    def console_messages(self) -> list[ConsoleMessage]:
        """All captured ``console.*`` events, structured."""
        return self.html.console_messages

    @property
    def final_url(self) -> str:
        """URL the browser ended up at, after any redirects."""
        return self._raw.final_url  # type: ignore[no-any-return]

    @property
    def status_code(self) -> int:
        """Final HTTP status code of the main document response."""
        return self._raw.status_code  # type: ignore[no-any-return]

    @property
    def elapsed_s(self) -> float:
        """End-to-end page-visit time in seconds."""
        return self._raw.elapsed_s  # type: ignore[no-any-return]

    def __repr__(self) -> str:
        return (
            f"FetchResult(html=<{len(self.html)} chars>, png=<{len(self.png)} bytes>, "
            f"final_url={self.final_url!r}, elapsed_s={self.elapsed_s:.3f})"
        )


class _RenderOutputShim:
    """Bridges FetchResult's raw into RenderResult.dom — both types have make_dom()."""

    def __init__(self, raw: _FetchOutput) -> None:
        self._raw = raw

    def make_dom(self) -> Dom:
        return self._raw.make_dom()


# ----------------------------------------------------------------------------
# Client
# ----------------------------------------------------------------------------


#: Dotted paths into ClientConfig that can only be set at Client creation.
#: Attempting to change any of these via ``client.update_config(...)`` raises
#: ``ValueError`` — you must construct a new Client.
_LAUNCH_ONLY_FIELDS: tuple[tuple[str, ...], ...] = (
    ("concurrency",),              # Semaphore sized once at launch
    ("chrome", "path"),            # Chrome binary is already exec'd
    ("chrome", "args"),            # Chrome CLI flags fixed at launch
    ("chrome", "user_data_dir"),   # Chrome user-data-dir is per-process
    ("chrome", "headless"),        # ditto
    ("network", "proxy"),          # --proxy-server is a CLI flag
    ("network", "ignore_https_errors"),  # --ignore-certificate-errors is a CLI flag
    ("timeout", "launch_ms"),      # only meaningful before Chrome is up
)


class Client:
    """Long-lived chromium connection backed by a pre-warmed page pool.

    Thread-safe — N Python threads may call ``fetch()``/``screenshot()``/
    ``batch()`` concurrently, capped by ``concurrency``.
    """

    __slots__ = ("_rust", "_config")

    def __init__(
        self,
        *args: Any,
        config: ClientConfig | None = None,
        **kwargs: Any,
    ) -> None:
        if args:
            raise TypeError(
                "Client() takes only keyword args. Pass config=ClientConfig(...) "
                "or flat kwargs like Client(viewport=(w,h), concurrency=N, ...)."
            )
        if config is not None and kwargs:
            raise TypeError("pass either config=... or flat kwargs, not both")

        if config is None:
            config = ClientConfig.from_flat(**kwargs) if kwargs else ClientConfig()

        self._config = config
        _client_log.info(
            "Client init: concurrency=%d viewport=%dx%d",
            config.concurrency,
            config.viewport.width,
            config.viewport.height,
        )
        self._rust = _RustClient(config.model_dump())

    # --- Config introspection + runtime update ---------------------------

    @property
    def config(self) -> _ConfigView:
        """Live-mutable config view.

        ``client.config.network.user_agent = "X"`` at any depth auto-syncs to
        Rust. Launch-only fields raise ``ValueError`` at the assignment line.
        Call ``.snapshot()`` for a detached deep-copy.
        """
        return _ConfigView(self, ())

    def update_config(
        self,
        *args: Any,
        config: ClientConfig | None = None,
        **kwargs: Any,
    ) -> None:
        """Swap in new config (takes effect on next fetch).

        Pass either ``config=ClientConfig(...)`` OR flat kwargs. In-flight
        calls snapshot at start so won't see a torn state. Raises
        ``ValueError`` on any launch-only field change (see
        ``_LAUNCH_ONLY_FIELDS``).
        """
        if args:
            raise TypeError("update_config() takes only keyword args")
        if config is not None and kwargs:
            raise TypeError("pass either config= OR flat kwargs, not both")

        if config is not None:
            new_config = config
        elif kwargs:
            partial = _flat_kwargs_to_partial(kwargs)
            merged = _deep_merge(self._config.model_dump(), partial)
            new_config = ClientConfig.model_validate(merged)
        else:
            return

        self._apply_config(new_config)

    def _apply_config(self, new_config: ClientConfig) -> None:
        """Validate launch-only invariants, push to Rust, store new config.

        Used by both ``update_config()`` and the ``_ConfigView`` attribute proxy.
        """
        old_data = self._config.model_dump()
        new_data = new_config.model_dump()
        for path in _LAUNCH_ONLY_FIELDS:
            if _get_nested(old_data, path) != _get_nested(new_data, path):
                raise ValueError(
                    f"cannot change launch-only field {'.'.join(path)!r} at runtime "
                    f"(was {_get_nested(old_data, path)!r}, "
                    f"requested {_get_nested(new_data, path)!r}). "
                    f"Create a new Client to change this setting."
                )
        self._rust.update_config(new_data)
        self._config = new_config

    # --- Primary API -------------------------------------------------------

    def fetch(
        self,
        url: str,
        *,
        config: FetchConfig | None = None,
        **overrides: Any,
    ) -> RenderResult:
        """Fetch URL, return fully-rendered HTML post-JS."""
        fc = _merge_fetch_config(config, overrides)
        _client_log.debug("fetch: %s", url)
        return _make_render_result(self._rust.fetch(url, fc.model_dump()))

    def screenshot(
        self,
        url: str,
        *,
        config: ScreenshotConfig | None = None,
        **overrides: Any,
    ) -> bytes:
        """Fetch URL, return a screenshot as image bytes (PNG by default)."""
        if config is None and not overrides:
            sc = ScreenshotConfig()
        elif config is not None and not overrides:
            sc = config
        else:
            data = config.model_dump() if config else {}
            for k, v in overrides.items():
                if k not in _SCREENSHOT_KWARGS:
                    raise TypeError(f"unknown screenshot kwarg: {k!r}")
                data[k] = v
            sc = ScreenshotConfig.model_validate(data)
        _client_log.debug("screenshot: %s (format=%s)", url, sc.format)
        return bytes(self._rust.screenshot(url, sc.model_dump()))

    def fetch_all(
        self,
        url: str,
        *,
        config: FetchConfig | None = None,
        full_page: bool = False,
        format: Literal["png", "jpeg", "webp"] = "png",
        quality: int | None = None,
        **overrides: Any,
    ) -> FetchResult:
        """Fetch URL, return HTML + image bytes from one page visit.

        ``format`` picks the image encoding; ``quality`` is 0-100 for jpeg/webp
        (ignored for png). The encoded bytes land on ``FetchResult.png`` (field
        name is historical — it holds whatever format you asked for).
        """
        fc = _merge_fetch_config(config, overrides)
        sc = ScreenshotConfig(full_page=full_page, format=format, quality=quality)
        _client_log.debug("fetch_all: %s (format=%s)", url, format)
        raw = self._rust.fetch_all(url, fc.model_dump(), sc.model_dump())
        return FetchResult(raw)

    def batch(
        self,
        urls: Iterable[str],
        *,
        capture: Literal["html", "png", "both"] = "html",
        config: FetchConfig | None = None,
    ) -> list[RenderResult | FetchResult | bytes]:
        """Run a batch of URLs in parallel (tokio-driven). Returns when all complete.

        Return type depends on ``capture``:
          - "html" → list[RenderResult]
          - "png" → list[bytes]
          - "both" → list[FetchResult]
        """
        fc = config or FetchConfig()
        url_list = list(urls)
        _client_log.info("batch: %d URLs, capture=%s", len(url_list), capture)
        raws = self._rust.batch(url_list, capture, fc.model_dump())
        results: list[RenderResult | FetchResult | bytes]
        if capture == "html":
            results = [_make_render_result(r) for r in raws]
        elif capture == "png":
            results = [bytes(b) for b in raws]
        else:
            results = [FetchResult(r) for r in raws]
        _client_log.debug("batch done: %d results returned", len(results))
        return results

    # --- Private / experimental -------------------------------------------

    def _render(
        self,
        html: bytes | str,
        *,
        base_url: str | None = None,
        config: FetchConfig | None = None,
    ) -> RenderResult:
        """NOT public. Inject raw HTML into chromium via data: URL.

        Niche: most users want ``.fetch(url)``. This is kept because it's cheap
        to implement (data: URL) and might be useful for unit tests.
        """
        if isinstance(html, str):
            html = html.encode("utf-8")
        import base64 as _b64

        data_url = "data:text/html;base64," + _b64.b64encode(html).decode("ascii")
        # TODO: respect base_url — requires document.write or <base>. For v1 we ignore.
        del base_url
        return self.fetch(data_url, config=config)

    # --- Lifecycle ---------------------------------------------------------

    def close(self) -> None:
        """Tear down the chromium process and free pool resources."""
        _client_log.info("Client close")
        self._rust.close()

    def __enter__(self) -> Client:
        return self

    def __exit__(self, *exc: Any) -> None:
        self.close()


# ----------------------------------------------------------------------------
# Module-level convenience (shared default Client, lazy-init, thread-safe)
# ----------------------------------------------------------------------------

_default_client: Client | None = None
_default_client_lock = threading.Lock()


def _get_default_client() -> Client:
    global _default_client
    if _default_client is None:
        with _default_client_lock:
            if _default_client is None:
                _default_client = Client()
    return _default_client


def fetch(url: str, *, config: FetchConfig | None = None, **overrides: Any) -> RenderResult:
    """Fetch URL → fully-rendered HTML. Uses a shared default Client."""
    return _get_default_client().fetch(url, config=config, **overrides)


def screenshot(
    url: str, *, config: ScreenshotConfig | None = None, **overrides: Any
) -> bytes:
    """Fetch URL → PNG bytes. Uses a shared default Client."""
    return _get_default_client().screenshot(url, config=config, **overrides)


def fetch_all(
    url: str,
    *,
    config: FetchConfig | None = None,
    full_page: bool = False,
    format: Literal["png", "jpeg", "webp"] = "png",
    quality: int | None = None,
    **overrides: Any,
) -> FetchResult:
    """Fetch URL → HTML + image bytes. Uses a shared default Client."""
    return _get_default_client().fetch_all(
        url,
        config=config,
        full_page=full_page,
        format=format,
        quality=quality,
        **overrides,
    )


# ----------------------------------------------------------------------------
# AsyncClient — async peer of Client
# ----------------------------------------------------------------------------


class AsyncClient:
    """Async peer of :class:`Client`. Same API, methods return coroutines.

    Use as an async context manager (``async with``) or call :meth:`aclose`
    explicitly. Multiple coroutines on one event loop can ``await`` fetch /
    screenshot calls concurrently — the page-pool semaphore caps in-flight
    pages at ``concurrency``.

    Construction is sync (chromium subprocess spawn briefly blocks the event
    loop). Match :class:`Client`'s signature: pass ``config=ClientConfig(...)``
    or flat kwargs.

    Example:
        >>> import asyncio, blazeweb
        >>>
        >>> async def main():
        ...     async with blazeweb.AsyncClient() as ac:
        ...         result = await ac.fetch("https://example.com")
        ...         print(result.dom.title())
        >>>
        >>> asyncio.run(main())
    """

    __slots__ = ("_rust", "_config")

    def __init__(
        self,
        *args: Any,
        config: ClientConfig | None = None,
        **kwargs: Any,
    ) -> None:
        """Construct an AsyncClient.

        Args:
            *args: Reserved for keyword-only enforcement. Passing any
                positional args raises ``TypeError``.
            config: A pre-built ``ClientConfig``. Mutually exclusive with
                ``**kwargs``.
            **kwargs: Flat config kwargs (``viewport=(w, h)``,
                ``concurrency=N``, ``user_agent=...``, etc.). See
                :meth:`ClientConfig.from_flat`. Mutually exclusive with
                ``config``.

        Raises:
            TypeError: If positional args are passed, or if both ``config``
                and flat kwargs are given.
        """
        if args:
            raise TypeError(
                "AsyncClient() takes only keyword args. Pass config=ClientConfig(...) "
                "or flat kwargs like AsyncClient(viewport=(w,h), concurrency=N, ...)."
            )
        if config is not None and kwargs:
            raise TypeError("pass either config=... or flat kwargs, not both")

        if config is None:
            config = ClientConfig.from_flat(**kwargs) if kwargs else ClientConfig()

        self._config = config
        _client_log.info(
            "AsyncClient init: concurrency=%d viewport=%dx%d",
            config.concurrency,
            config.viewport.width,
            config.viewport.height,
        )
        self._rust = _RustClient(config.model_dump())

    # --- Config introspection + runtime update ---------------------------

    @property
    def config(self) -> _ConfigView:
        """Live-mutable config view.

        ``ac.config.network.user_agent = "X"`` at any depth auto-syncs to
        Rust. Launch-only fields raise ``ValueError`` at the assignment
        line. Call ``.snapshot()`` for a detached deep-copy.
        """
        return _ConfigView(self, ())

    def update_config(
        self,
        *args: Any,
        config: ClientConfig | None = None,
        **kwargs: Any,
    ) -> None:
        """Swap in new config (takes effect on next fetch).

        Sync — config validation only, no IO. Pass ``config=ClientConfig(...)``
        OR flat kwargs. In-flight calls snapshot at start so they don't see
        a torn state.

        Raises:
            ValueError: On any launch-only field change. Create a new
                AsyncClient instead.
            TypeError: If both ``config`` and flat kwargs are given.
        """
        if args:
            raise TypeError("update_config() takes only keyword args")
        if config is not None and kwargs:
            raise TypeError("pass either config= OR flat kwargs, not both")

        if config is not None:
            new_config = config
        elif kwargs:
            partial = _flat_kwargs_to_partial(kwargs)
            merged = _deep_merge(self._config.model_dump(), partial)
            new_config = ClientConfig.model_validate(merged)
        else:
            return

        self._apply_config(new_config)

    def _apply_config(self, new_config: ClientConfig) -> None:
        """Validate launch-only invariants, push to Rust, store new config."""
        old_data = self._config.model_dump()
        new_data = new_config.model_dump()
        for path in _LAUNCH_ONLY_FIELDS:
            if _get_nested(old_data, path) != _get_nested(new_data, path):
                raise ValueError(
                    f"cannot change launch-only field {'.'.join(path)!r} at runtime "
                    f"(was {_get_nested(old_data, path)!r}, "
                    f"requested {_get_nested(new_data, path)!r}). "
                    f"Create a new AsyncClient to change this setting."
                )
        self._rust.update_config(new_data)
        self._config = new_config

    # --- Primary API ------------------------------------------------------

    async def fetch(
        self,
        url: str,
        *,
        config: FetchConfig | None = None,
        **overrides: Any,
    ) -> RenderResult:
        """Fetch URL, return fully-rendered HTML post-JS.

        Args:
            url: The URL to fetch.
            config: A ``FetchConfig`` for this call. Mutually exclusive with
                ``**overrides``.
            **overrides: Per-call overrides
                (``extra_headers``, ``timeout_ms``, ``wait_until``,
                ``wait_after_ms``).

        Returns:
            ``RenderResult`` — a ``str`` subclass holding the rendered HTML
            plus ``.errors`` / ``.final_url`` / ``.status_code`` /
            ``.elapsed_s`` / ``.dom``.

        Raises:
            RuntimeError: On CDP / navigation failures.
        """
        fc = _merge_fetch_config(config, overrides)
        _client_log.debug("afetch: %s", url)
        return _make_render_result(await self._rust.fetch_async(url, fc.model_dump()))

    async def screenshot(
        self,
        url: str,
        *,
        config: ScreenshotConfig | None = None,
        **overrides: Any,
    ) -> bytes:
        """Fetch URL, return a screenshot as image bytes (PNG by default).

        Args:
            url: The URL to fetch.
            config: A ``ScreenshotConfig`` for this call. Mutually exclusive
                with ``**overrides``.
            **overrides: Per-call overrides matching ``ScreenshotConfig``
                fields (``viewport``, ``full_page``, ``format``, ``quality``,
                etc.).

        Returns:
            Image bytes in the requested format.

        Raises:
            TypeError: On unknown screenshot kwarg.
            RuntimeError: On CDP / navigation failures.
        """
        if config is None and not overrides:
            sc = ScreenshotConfig()
        elif config is not None and not overrides:
            sc = config
        else:
            data = config.model_dump() if config else {}
            for k, v in overrides.items():
                if k not in _SCREENSHOT_KWARGS:
                    raise TypeError(f"unknown screenshot kwarg: {k!r}")
                data[k] = v
            sc = ScreenshotConfig.model_validate(data)
        _client_log.debug("ascreenshot: %s (format=%s)", url, sc.format)
        return bytes(await self._rust.screenshot_async(url, sc.model_dump()))

    async def fetch_all(
        self,
        url: str,
        *,
        config: FetchConfig | None = None,
        full_page: bool = False,
        format: Literal["png", "jpeg", "webp"] = "png",
        quality: int | None = None,
        **overrides: Any,
    ) -> FetchResult:
        """Fetch URL, return HTML + image bytes from one page visit.

        Args:
            url: The URL to fetch.
            config: A ``FetchConfig`` for this call.
            full_page: Capture the entire scrollable page, not just the
                viewport.
            format: Image encoding — ``"png"`` (default), ``"jpeg"``,
                or ``"webp"``.
            quality: 0-100 for jpeg/webp; ignored for png.
            **overrides: Per-call ``FetchConfig`` overrides.

        Returns:
            ``FetchResult`` with ``.html`` (RenderResult) and ``.png``
            (image bytes — field name is historical; holds whatever
            ``format`` was requested).

        Raises:
            RuntimeError: On CDP / navigation failures.
        """
        fc = _merge_fetch_config(config, overrides)
        sc = ScreenshotConfig(full_page=full_page, format=format, quality=quality)
        _client_log.debug("afetch_all: %s (format=%s)", url, format)
        raw = await self._rust.fetch_all_async(url, fc.model_dump(), sc.model_dump())
        return FetchResult(raw)

    async def batch(
        self,
        urls: Iterable[str],
        *,
        capture: Literal["html", "png", "both"] = "html",
        config: FetchConfig | None = None,
    ) -> list[RenderResult | FetchResult | bytes]:
        """Run a batch of URLs in parallel (tokio-driven). Awaits all.

        Args:
            urls: Iterable of URLs to fetch.
            capture: ``"html"`` → list[RenderResult], ``"png"`` → list[bytes],
                ``"both"`` → list[FetchResult].
            config: A ``FetchConfig`` applied to every URL in the batch.

        Returns:
            List of results in input order. Per-URL failures are returned as
            stub results (empty html/bytes + ``errors`` populated) rather
            than aborting the batch.

        Raises:
            ValueError: If ``capture`` is not one of the three valid values.
        """
        fc = config or FetchConfig()
        url_list = list(urls)
        _client_log.info("abatch: %d URLs, capture=%s", len(url_list), capture)
        raws = await self._rust.batch_async(url_list, capture, fc.model_dump())
        results: list[RenderResult | FetchResult | bytes]
        if capture == "html":
            results = [_make_render_result(r) for r in raws]
        elif capture == "png":
            results = [bytes(b) for b in raws]
        else:
            results = [FetchResult(r) for r in raws]
        _client_log.debug("abatch done: %d results returned", len(results))
        return results

    # --- Lifecycle --------------------------------------------------------

    async def aclose(self) -> None:
        """Tear down the chromium process and free pool resources.

        Idempotent — calling on an already-closed AsyncClient is a no-op.
        """
        _client_log.info("AsyncClient aclose")
        await self._rust.close_async()

    async def __aenter__(self) -> AsyncClient:
        """Enter the async context manager."""
        return self

    async def __aexit__(self, *exc: Any) -> None:
        """Exit the async context manager — calls :meth:`aclose`."""
        await self.aclose()


# ----------------------------------------------------------------------------
# Module-level async convenience (shared default AsyncClient, lazy-init)
# ----------------------------------------------------------------------------

_default_async_client: AsyncClient | None = None
_default_async_client_lock = threading.Lock()


def _get_default_async_client() -> AsyncClient:
    """Return (or lazily build) the shared module-level AsyncClient."""
    global _default_async_client
    if _default_async_client is None:
        with _default_async_client_lock:
            if _default_async_client is None:
                _default_async_client = AsyncClient()
    return _default_async_client


async def afetch(
    url: str, *, config: FetchConfig | None = None, **overrides: Any
) -> RenderResult:
    """Async fetch URL → fully-rendered HTML. Uses a shared default AsyncClient.

    See :meth:`AsyncClient.fetch` for arguments.
    """
    return await _get_default_async_client().fetch(url, config=config, **overrides)


async def ascreenshot(
    url: str, *, config: ScreenshotConfig | None = None, **overrides: Any
) -> bytes:
    """Async fetch URL → image bytes. Uses a shared default AsyncClient.

    See :meth:`AsyncClient.screenshot` for arguments.
    """
    return await _get_default_async_client().screenshot(url, config=config, **overrides)


async def afetch_all(
    url: str,
    *,
    config: FetchConfig | None = None,
    full_page: bool = False,
    format: Literal["png", "jpeg", "webp"] = "png",
    quality: int | None = None,
    **overrides: Any,
) -> FetchResult:
    """Async fetch URL → HTML + image bytes. Uses a shared default AsyncClient.

    See :meth:`AsyncClient.fetch_all` for arguments.
    """
    return await _get_default_async_client().fetch_all(
        url,
        config=config,
        full_page=full_page,
        format=format,
        quality=quality,
        **overrides,
    )


# ----------------------------------------------------------------------------
# Helpers
# ----------------------------------------------------------------------------


_SCREENSHOT_KWARGS = {
    "viewport",
    "full_page",
    "timeout_ms",
    "extra_headers",
    "format",
    "quality",
    "wait_until",
    "wait_after_ms",
}


def _merge_fetch_config(base: FetchConfig | None, overrides: dict[str, Any]) -> FetchConfig:
    if base is None and not overrides:
        return FetchConfig()
    data: dict[str, Any] = base.model_dump() if base else {}
    for k, v in overrides.items():
        if k not in {
            "actions",
            "block_navigation",
            "block_urls",
            "extra_headers",
            "scripts",
            "timeout_ms",
            "wait_until",
            "wait_after_ms",
        }:
            raise TypeError(f"unknown fetch kwarg: {k!r}")
        data[k] = v
    return FetchConfig.model_validate(data)


# ----------------------------------------------------------------------------
# Live-mutable config view — returned by Client.config / AsyncClient.config
# ----------------------------------------------------------------------------


class _ClientLike(Protocol):
    """Internal: structural type for ``_ConfigView``.

    Both ``Client`` and ``AsyncClient`` satisfy this — they share the
    config-view machinery despite differing in their fetch/screenshot
    return types.
    """

    _config: ClientConfig

    def _apply_config(self, new_config: ClientConfig) -> None: ...


class _ConfigView:
    """Live proxy over a client's config.

    Reads delegate to the pydantic model; writes route through
    ``client._apply_config`` to keep Rust in sync. Only ``_client`` /
    ``_path`` live on instances (``__slots__``).
    """

    __slots__ = ("_client", "_path")

    def __init__(self, client: _ClientLike, path: tuple[str, ...]) -> None:
        object.__setattr__(self, "_client", client)
        object.__setattr__(self, "_path", path)

    def _target(self) -> Any:
        """Walk current pydantic config down ``self._path`` and return the node."""
        cur: Any = self._client._config  # noqa: SLF001
        for p in self._path:
            cur = getattr(cur, p)
        return cur

    def __getattr__(self, name: str) -> Any:
        if name.startswith("_"):
            raise AttributeError(name)
        val = getattr(self._target(), name)
        if isinstance(val, _BaseModel):
            # Nested sub-config — return a view one level deeper so mutations
            # at any depth still route through _apply_config.
            return _ConfigView(self._client, self._path + (name,))
        return val

    def __setattr__(self, name: str, value: Any) -> None:
        if name.startswith("_"):
            object.__setattr__(self, name, value)
            return
        # Build a sparse partial dict for THIS change:
        #   path=("network",), name="user_agent" → {"network": {"user_agent": value}}
        partial: dict[str, Any] = {}
        cur = partial
        for p in self._path:
            cur[p] = {}
            cur = cur[p]
        cur[name] = value
        merged = _deep_merge(self._client._config.model_dump(), partial)  # noqa: SLF001
        new_cfg = ClientConfig.model_validate(merged)
        self._client._apply_config(new_cfg)  # noqa: SLF001

    def __repr__(self) -> str:
        return f"<live config view of {self._target()!r}>"

    def snapshot(self) -> ClientConfig | Any:
        """Detached deep-copy. Sub-views return their sub-config type."""
        return self._target().model_copy(deep=True)

    def model_dump(self, **kw: Any) -> dict[str, Any]:
        # _target() returns the live pydantic model whose typing varies by depth.
        return self._target().model_dump(**kw)  # type: ignore[no-any-return]

    def model_dump_json(self, **kw: Any) -> str:
        return self._target().model_dump_json(**kw)  # type: ignore[no-any-return]


# update_config / Client(**kwargs) build a SPARSE partial dict (unlike
# ClientConfig.from_flat, which fills defaults) — kept as a separate table.
_FLAT_KWARG_MAP: dict[str, tuple[str, ...]] = {
    # Viewport
    "device_scale_factor": ("viewport", "device_scale_factor"),
    "mobile": ("viewport", "mobile"),
    # Network
    "user_agent": ("network", "user_agent"),
    "user_agent_metadata": ("network", "user_agent_metadata"),
    "proxy": ("network", "proxy"),
    "extra_headers": ("network", "extra_headers"),
    "ignore_https_errors": ("network", "ignore_https_errors"),
    "block_urls": ("network", "block_urls"),
    "disable_cache": ("network", "disable_cache"),
    "offline": ("network", "offline"),
    "latency_ms": ("network", "latency_ms"),
    "download_bps": ("network", "download_bps"),
    "upload_bps": ("network", "upload_bps"),
    # Emulation
    "locale": ("emulation", "locale"),
    "timezone": ("emulation", "timezone"),
    "geolocation": ("emulation", "geolocation"),
    "prefers_color_scheme": ("emulation", "prefers_color_scheme"),
    "javascript_enabled": ("emulation", "javascript_enabled"),
    # Timeout
    "navigation_timeout_ms": ("timeout", "navigation_ms"),
    "launch_timeout_ms": ("timeout", "launch_ms"),
    "screenshot_timeout_ms": ("timeout", "screenshot_ms"),
    # Chrome
    "chrome_path": ("chrome", "path"),
    "chrome_args": ("chrome", "args"),
    "user_data_dir": ("chrome", "user_data_dir"),
    "headless": ("chrome", "headless"),
}


def _flat_kwargs_to_partial(kwargs: dict[str, Any]) -> dict[str, Any]:
    """Translate flat kwargs into a sparse nested dict.

    Only mentioned fields appear in the output; defaults are NOT filled in.
    Meant for merging onto an existing config.
    """
    out: dict[str, Any] = {}
    for k, v in kwargs.items():
        if k == "viewport":
            if isinstance(v, tuple) and len(v) == 2:
                out.setdefault("viewport", {})
                out["viewport"]["width"] = int(v[0])
                out["viewport"]["height"] = int(v[1])
            elif isinstance(v, ViewportConfig):
                out["viewport"] = v.model_dump()
            else:
                raise TypeError(
                    f"viewport must be (w,h) or ViewportConfig, got {type(v).__name__}"
                )
            continue
        if k == "scripts":
            if isinstance(v, ScriptsConfig):
                out["scripts"] = v.model_dump()
            elif isinstance(v, dict):
                out["scripts"] = dict(v)
            else:
                raise TypeError(
                    f"scripts must be dict or ScriptsConfig, got {type(v).__name__}"
                )
            continue
        if k == "concurrency":
            out["concurrency"] = v
            continue
        if k == "wait_until":
            out["wait_until"] = v
            continue
        if k == "wait_after_ms":
            out["wait_after_ms"] = v
            continue
        if k == "capture_console_level":
            out["capture_console_level"] = v
            continue
        if k not in _FLAT_KWARG_MAP:
            raise TypeError(f"unknown ClientConfig kwarg: {k!r}")
        sub, field = _FLAT_KWARG_MAP[k]
        out.setdefault(sub, {})
        out[sub][field] = v
    return out


def _deep_merge(base: dict[str, Any], overlay: dict[str, Any]) -> dict[str, Any]:
    """Recursively merge `overlay` into `base`. `overlay` wins where both have a key."""
    out = dict(base)
    for k, v in overlay.items():
        if k in out and isinstance(out[k], dict) and isinstance(v, dict):
            out[k] = _deep_merge(out[k], v)
        else:
            out[k] = v
    return out


def _get_nested(data: dict[str, Any], path: tuple[str, ...]) -> Any:
    """Walk a dotted path into a nested dict. Returns None if any step is missing."""
    cur: Any = data
    for key in path:
        if not isinstance(cur, dict):
            return None
        cur = cur.get(key)
    return cur
