"""Configuration hierarchy for blazeweb.

All knobs live under ``ClientConfig``. Pydantic-settings auto-loads from env
(``BLAZEWEB_<field>`` top-level, ``BLAZEWEB_<section>__<field>`` for nested).

    blazeweb.Client()                                          # defaults + env
    blazeweb.Client(config=ClientConfig(...))                  # structured
    blazeweb.Client(viewport=(1920, 1080), concurrency=32)     # flat kwargs
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class ViewportConfig(BaseModel):
    """Browser viewport dimensions."""

    model_config = ConfigDict(extra="forbid")

    width: int = Field(1200, ge=1, le=16384)
    height: int = Field(800, ge=1, le=16384)
    device_scale_factor: float = Field(1.0, gt=0.0, le=4.0)
    mobile: bool = False


class NetworkConfig(BaseModel):
    """HTTP headers, proxy, throttling, URL blocking."""

    model_config = ConfigDict(extra="forbid")

    user_agent: str | None = None
    proxy: str | None = None
    """``http://host:port`` or ``socks5://host:port`` — passed as a Chrome CLI flag."""

    extra_headers: dict[str, str] = Field(default_factory=dict)
    ignore_https_errors: bool = False
    """Pass ``--ignore-certificate-errors`` to Chrome."""

    block_urls: list[str] = Field(default_factory=list)
    """URLPattern strings (e.g. ``*://*.doubleclick.net/*``) to drop at the
    network layer. Applied via ``Network.setBlockedURLs`` per pooled page."""

    disable_cache: bool = False
    offline: bool = False
    latency_ms: float | None = None
    download_bps: int | None = None
    upload_bps: int | None = None

    @field_validator("block_urls")
    @classmethod
    def _no_empty_patterns(cls, v: list[str]) -> list[str]:
        return [p for p in v if p.strip()]


class EmulationConfig(BaseModel):
    """Browser-side locale / timezone / geolocation / color-scheme emulation."""

    model_config = ConfigDict(extra="forbid")

    locale: str | None = None
    timezone: str | None = None
    """IANA timezone (e.g. ``America/New_York``)."""

    geolocation: tuple[float, float] | None = None
    """``(latitude, longitude)``."""

    prefers_color_scheme: Literal["light", "dark"] | None = None
    javascript_enabled: bool = True


class TimeoutConfig(BaseModel):
    """Per-operation time limits (ms)."""

    model_config = ConfigDict(extra="forbid")

    navigation_ms: int = Field(30000, ge=100)
    launch_ms: int = Field(15000, ge=500)
    screenshot_ms: int = Field(5000, ge=100)


class ChromeConfig(BaseModel):
    """Chrome launch options."""

    model_config = ConfigDict(extra="forbid")

    path: str | None = None
    """Override the resolved binary. Default: bundled → env → system."""

    args: list[str] = Field(default_factory=list)
    user_data_dir: str | None = None
    """None → ephemeral tempdir per launch; a path → persistent profile."""

    headless: bool = True


class ClientConfig(BaseSettings):
    """Top-level Client configuration. Loads ``BLAZEWEB_*`` env vars."""

    model_config = SettingsConfigDict(
        env_prefix="BLAZEWEB_",
        env_nested_delimiter="__",
        extra="forbid",
    )

    concurrency: int = Field(16, ge=1, le=512)
    """Max in-flight pages. Excess threads queue on an internal Semaphore."""

    wait_until: Literal["load", "domcontentloaded"] = "load"
    """Which lifecycle event returns control to the caller.

    - ``"load"`` (default) — window.onload; most complete, matches
      Playwright/Puppeteer.
    - ``"domcontentloaded"`` — parser done, may miss post-DCL SPA mutations.
      Faster on tracker-heavy pages, marginal on most. Falls back to load
      for tiny documents where DCL never fires.
    """

    wait_after_ms: int = Field(0, ge=0, le=60000)
    """Post-lifecycle-event settle (ms). Useful for SPAs that hydrate
    async after ``wait_until`` fires."""

    viewport: ViewportConfig = Field(default_factory=ViewportConfig)
    network: NetworkConfig = Field(default_factory=NetworkConfig)
    emulation: EmulationConfig = Field(default_factory=EmulationConfig)
    timeout: TimeoutConfig = Field(default_factory=TimeoutConfig)
    chrome: ChromeConfig = Field(default_factory=ChromeConfig)

    @classmethod
    def from_flat(cls, **kwargs: Any) -> ClientConfig:
        """Build a ClientConfig from flat kwargs. Powers the
        ``Client(viewport=(w,h), user_agent=..., concurrency=16)`` shortcut."""
        # Maps flat kwarg → (sub_config_name, field_name).
        flat_map: dict[str, tuple[str, str]] = {
            "device_scale_factor": ("viewport", "device_scale_factor"),
            "mobile": ("viewport", "mobile"),
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
            "locale": ("emulation", "locale"),
            "timezone": ("emulation", "timezone"),
            "geolocation": ("emulation", "geolocation"),
            "prefers_color_scheme": ("emulation", "prefers_color_scheme"),
            "javascript_enabled": ("emulation", "javascript_enabled"),
            "navigation_timeout_ms": ("timeout", "navigation_ms"),
            "launch_timeout_ms": ("timeout", "launch_ms"),
            "screenshot_timeout_ms": ("timeout", "screenshot_ms"),
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

        # viewport=(w,h) tuple shortcut.
        if "viewport" in kwargs:
            v = kwargs.pop("viewport")
            if isinstance(v, tuple) and len(v) == 2:
                nested["viewport"]["width"] = int(v[0])
                nested["viewport"]["height"] = int(v[1])
            elif isinstance(v, ViewportConfig):
                top["viewport"] = v
            else:
                raise TypeError(
                    f"viewport must be (width, height) or ViewportConfig, "
                    f"got {type(v).__name__}"
                )

        for top_field in ("concurrency", "wait_until", "wait_after_ms"):
            if top_field in kwargs:
                top[top_field] = kwargs.pop(top_field)

        for k, val in kwargs.items():
            if k not in flat_map:
                raise TypeError(f"unknown ClientConfig kwarg: {k!r}")
            sub, field = flat_map[k]
            nested[sub][field] = val

        # Build sub-configs only for sections the user actually touched; rest
        # fall through to defaults + env.
        cls_map = {
            "viewport": ViewportConfig,
            "network": NetworkConfig,
            "emulation": EmulationConfig,
            "timeout": TimeoutConfig,
            "chrome": ChromeConfig,
        }
        for sub_name, sub_kw in nested.items():
            if sub_kw and sub_name not in top:
                top[sub_name] = cls_map[sub_name](**sub_kw)

        return cls(**top)


class FetchConfig(BaseModel):
    """Per-call override for ``Client.fetch()`` / ``fetch_all()``.

    Unset fields fall through to the Client's base config.
    """

    model_config = ConfigDict(extra="forbid")

    extra_headers: dict[str, str] = Field(default_factory=dict)
    """Merged on top of the Client's base ``network.extra_headers``."""

    timeout_ms: int | None = Field(None, ge=100)
    wait_until: Literal["domcontentloaded", "load"] | None = None
    wait_after_ms: int | None = Field(None, ge=0, le=60000)


class ScreenshotConfig(BaseModel):
    """Per-call override for ``Client.screenshot()``."""

    model_config = ConfigDict(extra="forbid")

    viewport: tuple[int, int] | None = None
    full_page: bool = False
    """Scroll and capture beyond the viewport height."""

    timeout_ms: int | None = Field(None, ge=100)
    extra_headers: dict[str, str] = Field(default_factory=dict)

    format: Literal["png", "jpeg", "webp"] = "png"
    quality: int | None = Field(None, ge=0, le=100)
    """0-100 for jpeg/webp. Ignored by png."""

    wait_until: Literal["domcontentloaded", "load"] | None = None
    wait_after_ms: int | None = Field(None, ge=0, le=60000)


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
