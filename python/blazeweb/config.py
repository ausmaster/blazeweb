"""Configuration hierarchy for blazeweb.

Every knob that can be tuned lives in a sub-config under ``ClientConfig``.
Built on Pydantic Settings, so every option is ALSO loadable from the
environment with the ``BLAZEWEB_`` prefix and ``__`` nested delimiter::

    BLAZEWEB_CONCURRENCY=32
    BLAZEWEB_VIEWPORT__WIDTH=1920
    BLAZEWEB_NETWORK__USER_AGENT='Mozilla/5.0 ...'
    BLAZEWEB_CHROME__PATH=/usr/bin/chromium-browser

Typical usage::

    import blazeweb

    # Defaults + env
    client = blazeweb.Client()

    # Explicit structured config
    cfg = blazeweb.ClientConfig(
        concurrency=32,
        viewport=blazeweb.ViewportConfig(width=1920, height=1080),
        network=blazeweb.NetworkConfig(user_agent="Mozilla/5.0 ..."),
    )
    client = blazeweb.Client(config=cfg)

    # Flat kwargs shortcut (constructs ClientConfig under the hood)
    client = blazeweb.Client(viewport=(1920, 1080), concurrency=32)
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


# ----------------------------------------------------------------------------
# Viewport / display emulation
# ----------------------------------------------------------------------------


class ViewportConfig(BaseModel):
    """Browser viewport dimensions."""

    model_config = ConfigDict(extra="forbid")

    width: int = Field(1200, ge=1, le=16384)
    height: int = Field(800, ge=1, le=16384)
    device_scale_factor: float = Field(1.0, gt=0.0, le=4.0)
    mobile: bool = False


# ----------------------------------------------------------------------------
# Network
# ----------------------------------------------------------------------------


class NetworkConfig(BaseModel):
    """HTTP headers, proxy, network throttling, URL blocking."""

    model_config = ConfigDict(extra="forbid")

    user_agent: str | None = None
    """Override Chrome's UA. None leaves Chrome's default."""

    proxy: str | None = None
    """e.g. ``"http://host:port"``, ``"socks5://host:port"``. Applied as Chrome CLI flag."""

    extra_headers: dict[str, str] = Field(default_factory=dict)
    """Added to every request. Useful for auth tokens, session markers."""

    ignore_https_errors: bool = False
    """Pass ``--ignore-certificate-errors`` to Chrome. Lets you hit sites with bad certs."""

    block_urls: list[str] = Field(default_factory=list)
    """Glob patterns matched against request URLs; matches are dropped.
    Useful to block ads/tracking: ``["*doubleclick*", "*.googletagmanager.com/*"]``."""

    disable_cache: bool = False
    """``Network.setCacheDisabled(True)``. Fresh fetches every time."""

    offline: bool = False
    """``Network.emulateNetworkConditions(offline=True)``. Page sees network failure."""

    latency_ms: float | None = None
    """Additional latency per request (ms), on top of network natural latency."""

    download_bps: int | None = None
    """Simulated download throughput (bytes/s). None leaves uncapped."""

    upload_bps: int | None = None
    """Simulated upload throughput (bytes/s). None leaves uncapped."""

    @field_validator("block_urls")
    @classmethod
    def _no_empty_patterns(cls, v: list[str]) -> list[str]:
        return [p for p in v if p.strip()]


# ----------------------------------------------------------------------------
# Emulation (locale, timezone, geolocation, color scheme)
# ----------------------------------------------------------------------------


class EmulationConfig(BaseModel):
    """Browser-side emulation — what the page *thinks* the environment is."""

    model_config = ConfigDict(extra="forbid")

    locale: str | None = None
    """e.g. ``"en-US"``, ``"ja-JP"``. Applied via ``Emulation.setLocaleOverride``."""

    timezone: str | None = None
    """IANA timezone e.g. ``"America/New_York"``. Applied via ``Emulation.setTimezoneOverride``."""

    geolocation: tuple[float, float] | None = None
    """``(latitude, longitude)``. Applied via ``Emulation.setGeolocationOverride``."""

    prefers_color_scheme: Literal["light", "dark"] | None = None
    """CSS ``prefers-color-scheme`` media query response."""

    javascript_enabled: bool = True
    """When False, ``Emulation.setScriptExecutionDisabled(True)`` — no JS runs.
    For perf comparison vs requests/httpx (same-no-JS baseline)."""


# ----------------------------------------------------------------------------
# Timeouts
# ----------------------------------------------------------------------------


class TimeoutConfig(BaseModel):
    """Per-operation time limits (ms)."""

    model_config = ConfigDict(extra="forbid")

    navigation_ms: int = Field(30000, ge=100)
    """Cap on a single URL navigation (goto + wait-for-load)."""

    launch_ms: int = Field(15000, ge=500)
    """Cap on Chrome process startup + CDP attach."""

    screenshot_ms: int = Field(5000, ge=100)
    """Cap on the PNG capture step after navigation completes."""


# ----------------------------------------------------------------------------
# Chrome process options
# ----------------------------------------------------------------------------


class ChromeConfig(BaseModel):
    """How we launch the Chrome binary itself."""

    model_config = ConfigDict(extra="forbid")

    path: str | None = None
    """Override resolved binary. Default: bundled ``chrome-headless-shell`` → env → system."""

    args: list[str] = Field(default_factory=list)
    """Extra CLI flags appended to the launch line.
    Example: ``["--disable-blink-features=AutomationControlled"]``."""

    user_data_dir: str | None = None
    """Chrome profile dir. None → ephemeral tempdir, clean each launch.
    Set to a persistent path to retain cookies/localStorage between runs."""

    headless: bool = True
    """False only for interactive debugging (requires a real display)."""


# ----------------------------------------------------------------------------
# Top-level client config (reads env)
# ----------------------------------------------------------------------------


class ClientConfig(BaseSettings):
    """Top-level Client configuration. All knobs live under nested sub-configs.

    Loads from environment by default — e.g. ``BLAZEWEB_CONCURRENCY=32``
    or ``BLAZEWEB_VIEWPORT__WIDTH=1920`` — and merges with constructor kwargs.
    """

    model_config = SettingsConfigDict(
        env_prefix="BLAZEWEB_",
        env_nested_delimiter="__",
        extra="forbid",
    )

    concurrency: int = Field(16, ge=1, le=512)
    """Max in-flight pages across all Python threads calling this Client.
    Excess calls block on an internal Semaphore rather than over-subscribing Chrome."""

    wait_until: Literal["load", "domcontentloaded"] = "load"
    """Lifecycle event to wait for before capturing:

    - ``"load"`` (default) — waits for ``window.onload``: all subresources
      downloaded, deferred scripts executed, SPAs hydrated. Matches
      Playwright/Puppeteer convention. Most complete and correct for general
      scraping. On tracker-heavy pages adds time for trackers to finish.
    - ``"domcontentloaded"`` — returns as soon as the HTML parser has finished
      building the DOM. Faster on pages with slow third-party subresources, but
      may return before async-rendered SPA content is present. Opt-in for speed
      on lean/static sites where you know DCL is sufficient.

    Falls back to ``load`` automatically for edge cases where DCL doesn't fire
    (very short / empty documents)."""

    wait_after_ms: int = Field(0, ge=0, le=60000)
    """Extra sleep after the chosen lifecycle event (ms). Default 0. Useful for
    SPAs that mutate the DOM via async JS AFTER DCL/load — e.g. set
    ``wait_after_ms=500`` to let React-style frameworks finish rendering."""

    viewport: ViewportConfig = Field(default_factory=ViewportConfig)
    network: NetworkConfig = Field(default_factory=NetworkConfig)
    emulation: EmulationConfig = Field(default_factory=EmulationConfig)
    timeout: TimeoutConfig = Field(default_factory=TimeoutConfig)
    chrome: ChromeConfig = Field(default_factory=ChromeConfig)

    @classmethod
    def from_flat(cls, **kwargs: Any) -> ClientConfig:
        """Build a ClientConfig from flat kwargs, dispatching each to the right sub-config.

        This powers the ``Client(viewport=(w,h), user_agent=..., concurrency=16)``
        shortcut: users don't have to assemble sub-configs explicitly.
        """
        # Map each known flat kwarg to its (sub_config_name, field_name) target.
        flat_map: dict[str, tuple[str, str]] = {
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

        nested: dict[str, dict[str, Any]] = {
            "viewport": {},
            "network": {},
            "emulation": {},
            "timeout": {},
            "chrome": {},
        }
        top: dict[str, Any] = {}

        # `viewport=(w, h)` shortcut
        if "viewport" in kwargs:
            v = kwargs.pop("viewport")
            if isinstance(v, tuple) and len(v) == 2:
                nested["viewport"]["width"] = int(v[0])
                nested["viewport"]["height"] = int(v[1])
            elif isinstance(v, ViewportConfig):
                top["viewport"] = v
            else:
                raise TypeError(
                    f"viewport must be (width, height) or ViewportConfig, got {type(v).__name__}"
                )

        if "concurrency" in kwargs:
            top["concurrency"] = kwargs.pop("concurrency")

        if "wait_until" in kwargs:
            top["wait_until"] = kwargs.pop("wait_until")

        if "wait_after_ms" in kwargs:
            top["wait_after_ms"] = kwargs.pop("wait_after_ms")

        for k, val in kwargs.items():
            if k in flat_map:
                sub, field = flat_map[k]
                nested[sub][field] = val
            else:
                raise TypeError(f"unknown ClientConfig kwarg: {k!r}")

        # Build sub-configs only if users actually supplied anything (else let
        # defaults + env win).
        for sub_name, sub_kw in nested.items():
            if sub_kw and sub_name not in top:
                cls_map = {
                    "viewport": ViewportConfig,
                    "network": NetworkConfig,
                    "emulation": EmulationConfig,
                    "timeout": TimeoutConfig,
                    "chrome": ChromeConfig,
                }
                top[sub_name] = cls_map[sub_name](**sub_kw)

        return cls(**top)


# ----------------------------------------------------------------------------
# Per-call overrides
# ----------------------------------------------------------------------------


class FetchConfig(BaseModel):
    """Per-call overrides for ``Client.fetch()`` / ``Client.fetch_all()``.

    Applied on top of the Client's base config for a single call. Unset fields
    fall through to the Client's defaults.
    """

    model_config = ConfigDict(extra="forbid")

    extra_headers: dict[str, str] = Field(default_factory=dict)
    """Merged into (and overriding) the Client's base extra_headers for this call."""

    timeout_ms: int | None = Field(None, ge=100)
    """Overrides the Client's ``timeout.navigation_ms`` for this call."""

    wait_until: Literal["domcontentloaded", "load"] | None = None
    """Overrides the Client's ``wait_until`` for this call. ``None`` inherits."""

    wait_after_ms: int | None = Field(None, ge=0, le=60000)
    """Overrides the Client's ``wait_after_ms`` for this call. ``None`` inherits."""


class ScreenshotConfig(BaseModel):
    """Per-call overrides for ``Client.screenshot()``."""

    model_config = ConfigDict(extra="forbid")

    viewport: tuple[int, int] | None = None
    """Overrides the Client's viewport for this screenshot."""

    full_page: bool = False
    """When True, scroll the viewport down and capture the full page height."""

    timeout_ms: int | None = Field(None, ge=100)

    extra_headers: dict[str, str] = Field(default_factory=dict)

    format: Literal["png", "jpeg", "webp"] = "png"
    """Image encoding. ``png`` is lossless (default); ``jpeg`` / ``webp`` take ``quality``."""

    quality: int | None = Field(None, ge=0, le=100)
    """0-100 quality for ``jpeg`` / ``webp``. Ignored by PNG. None → chromium default."""

    wait_until: Literal["domcontentloaded", "load"] | None = None
    """Overrides the Client's ``wait_until`` for this screenshot. ``None`` inherits."""

    wait_after_ms: int | None = Field(None, ge=0, le=60000)
    """Overrides the Client's ``wait_after_ms`` for this screenshot. ``None`` inherits."""


__all__ = [
    "ChromeConfig",
    "ClientConfig",
    "EmulationConfig",
    "FetchConfig",
    "NetworkConfig",
    "ScreenshotConfig",
    "TimeoutConfig",
    "ViewportConfig",
]
