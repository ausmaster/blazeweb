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
from typing import Any, Iterable, Literal

from blazeweb._blazeweb import (
    Client as _RustClient,
    Dom as Dom,
    Element as Element,
    _FetchOutput,
    _RenderOutput,
)
from blazeweb.config import (
    ChromeConfig,
    ClientConfig,
    EmulationConfig,
    FetchConfig,
    NetworkConfig,
    ScreenshotConfig,
    TimeoutConfig,
    ViewportConfig,
)

__all__ = [
    # Module-level convenience
    "fetch",
    "screenshot",
    "fetch_all",
    # Classes
    "Client",
    "Dom",
    "Element",
    "FetchResult",
    "RenderResult",
    # Configs (re-exported from blazeweb.config)
    "ClientConfig",
    "FetchConfig",
    "ScreenshotConfig",
    "ViewportConfig",
    "NetworkConfig",
    "EmulationConfig",
    "TimeoutConfig",
    "ChromeConfig",
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


class RenderResult(str):
    """Fully-rendered post-JS HTML. Subclasses ``str`` (lxml, regex, BS4 work).

    Adds:
      - ``.errors`` — list[str] of console errors and load errors
      - ``.final_url`` — URL after any redirects
      - ``.status_code`` — final HTTP status
      - ``.elapsed_s`` — end-to-end page-visit time (seconds)
      - ``.dom`` — Rust-side HTML query (lazy; CSS selectors + BS4-like find)
    """

    errors: list[str]
    final_url: str
    status_code: int
    elapsed_s: float

    def __new__(
        cls,
        html: str,
        *,
        errors: list[str] | None = None,
        final_url: str = "",
        status_code: int = 0,
        elapsed_s: float = 0.0,
        _raw: _RenderOutput | None = None,
    ) -> RenderResult:
        instance = super().__new__(cls, html)
        instance.errors = errors or []
        instance.final_url = final_url
        instance.status_code = status_code
        instance.elapsed_s = elapsed_s
        instance._raw = _raw  # type: ignore[attr-defined]
        instance._dom = None  # type: ignore[attr-defined]
        return instance

    @property
    def html(self) -> str:
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
        # _FetchOutput has `.make_dom()` just like _RenderOutput — duck-typing
        # lets RenderResult.dom use either raw object.
        self.html = RenderResult(
            raw.html,
            errors=raw.errors,
            final_url=raw.final_url,
            status_code=raw.status_code,
            elapsed_s=raw.elapsed_s,
            _raw=raw,  # type: ignore[arg-type]
        )
        self.png = bytes(raw.png)

    @property
    def errors(self) -> list[str]:
        return self._raw.errors

    @property
    def final_url(self) -> str:
        return self._raw.final_url

    @property
    def status_code(self) -> int:
        return self._raw.status_code

    @property
    def elapsed_s(self) -> float:
        return self._raw.elapsed_s

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


class Client:
    """Long-lived chromium connection. Thread-safe — N Python threads may call
    ``fetch()``/``screenshot()``/``batch()`` concurrently; an internal Semaphore
    caps in-flight work at ``concurrency``.
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
        self._rust = _RustClient(config.model_dump())

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
        raw = self._rust.fetch(url, fc.model_dump())
        return RenderResult(
            raw.html,
            errors=raw.errors,
            final_url=raw.final_url,
            status_code=raw.status_code,
            elapsed_s=raw.elapsed_s,
            _raw=raw,
        )

    def screenshot(
        self,
        url: str,
        *,
        config: ScreenshotConfig | None = None,
        **overrides: Any,
    ) -> bytes:
        """Fetch URL, return PNG screenshot."""
        if config is None and not overrides:
            sc = ScreenshotConfig()
        elif config is not None and not overrides:
            sc = config
        else:
            data = config.model_dump() if config else {}
            for k, v in overrides.items():
                if k not in {"viewport", "full_page", "timeout_ms", "extra_headers"}:
                    raise TypeError(f"unknown screenshot kwarg: {k!r}")
                data[k] = v
            sc = ScreenshotConfig.model_validate(data)
        return bytes(self._rust.screenshot(url, sc.model_dump()))

    def fetch_all(
        self,
        url: str,
        *,
        config: FetchConfig | None = None,
        full_page: bool = False,
        **overrides: Any,
    ) -> FetchResult:
        """Fetch URL, return both HTML and PNG from one page visit."""
        fc = _merge_fetch_config(config, overrides)
        sc = ScreenshotConfig(full_page=full_page)
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
        raws = self._rust.batch(list(urls), capture, fc.model_dump())
        if capture == "html":
            return [
                RenderResult(
                    r.html,
                    errors=r.errors,
                    final_url=r.final_url,
                    status_code=r.status_code,
                    elapsed_s=r.elapsed_s,
                    _raw=r,
                )
                for r in raws
            ]
        if capture == "png":
            return [bytes(b) for b in raws]
        return [FetchResult(r) for r in raws]

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
    url: str, *, config: FetchConfig | None = None, full_page: bool = False, **overrides: Any
) -> FetchResult:
    """Fetch URL → HTML + PNG. Uses a shared default Client."""
    return _get_default_client().fetch_all(
        url, config=config, full_page=full_page, **overrides
    )


# ----------------------------------------------------------------------------
# Helpers — _merge_fetch_config is used by fetch() and fetch_all(), keep it.
# ----------------------------------------------------------------------------


def _merge_fetch_config(base: FetchConfig | None, overrides: dict[str, Any]) -> FetchConfig:
    if base is None and not overrides:
        return FetchConfig()
    data: dict[str, Any] = base.model_dump() if base else {}
    for k, v in overrides.items():
        if k not in {"extra_headers", "timeout_ms"}:
            raise TypeError(f"unknown fetch kwarg: {k!r}")
        data[k] = v
    return FetchConfig.model_validate(data)
