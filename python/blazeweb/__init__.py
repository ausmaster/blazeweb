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

    # --- Config introspection + runtime update ---------------------------

    @property
    def config(self) -> ClientConfig:
        """Snapshot of the Client's current config.

        Returns a **deep copy** — mutating the returned object does NOT affect
        the live client. To change settings, use :meth:`update_config`.
        """
        return self._config.model_copy(deep=True)

    def update_config(
        self,
        *args: Any,
        config: ClientConfig | None = None,
        **kwargs: Any,
    ) -> None:
        """Swap in new runtime-mutable config fields; takes effect on next call.

        Accepts either a full ``config=ClientConfig(...)`` OR flat kwargs which
        merge onto the current config (``client.update_config(user_agent="...",
        locale="ja-JP")``).

        Raises ``ValueError`` if any launch-only field has changed — those
        live in the Chrome process and can't be flipped after launch. Recreate
        the Client for that. See :data:`_LAUNCH_ONLY_FIELDS` for the list.

        Thread-safety: atomic swap. In-flight ``fetch()``/``screenshot()`` calls
        snapshot config at the start, so they won't see a torn state. Batches
        snapshot once at dispatch — mid-batch updates don't re-apply.
        """
        if args:
            raise TypeError("update_config() takes only keyword args")
        if config is not None and kwargs:
            raise TypeError("pass either config= OR flat kwargs, not both")

        if config is not None:
            new_config = config
        elif kwargs:
            # Build a sparse partial from kwargs and merge on top of current.
            partial = _flat_kwargs_to_partial(kwargs)
            merged = _deep_merge(self._config.model_dump(), partial)
            new_config = ClientConfig.model_validate(merged)
        else:
            return  # no-op

        # Guard: reject launch-only field changes.
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


# --- update_config helpers (shared between Client.__init__ kwargs and update_config) ---

# Same mapping as ClientConfig.from_flat's internal flat_map, duplicated here
# because we want a SPARSE partial-dict (no defaults), not a full ClientConfig.
_FLAT_KWARG_MAP: dict[str, tuple[str, ...]] = {
    # Viewport
    "device_scale_factor": ("viewport", "device_scale_factor"),
    "mobile": ("viewport", "mobile"),
    # Network
    "user_agent": ("network", "user_agent"),
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
    """Translate flat kwargs into a sparse nested dict. Only mentioned fields
    appear in the output; defaults are NOT filled in. Meant for merging onto
    an existing config."""
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
        if k == "concurrency":
            out["concurrency"] = v
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
