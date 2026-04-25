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


class UserAgentBrandVersion(BaseModel):
    """One entry in the ``Sec-CH-UA`` brand list.

    Matches CDP's ``Emulation.UserAgentBrandVersion``.
    """

    model_config = ConfigDict(extra="forbid")

    brand: str
    version: str


class UserAgentMetadata(BaseModel):
    """Structured client-hint metadata.

    Matches CDP's ``Emulation.UserAgentMetadata`` shape, sent via
    ``Network.setUserAgentOverride`` alongside the plain UA header.

    Must be consistent with ``NetworkConfig.user_agent`` — a UA string that
    says Chrome 131 on Windows paired with ``brands=[{brand:"Firefox",…}]``
    is itself a fingerprintable tell.
    """

    model_config = ConfigDict(extra="forbid")

    brands: list[UserAgentBrandVersion] | None = None
    """Entries emitted in ``Sec-CH-UA``. E.g. ``[{"brand":"Google Chrome",
    "version":"131"}, {"brand":"Chromium","version":"131"},
    {"brand":"Not_A Brand","version":"24"}]``."""

    full_version_list: list[UserAgentBrandVersion] | None = None
    """Entries emitted in ``Sec-CH-UA-Full-Version-List``. Usually brand +
    full x.y.z.w version."""

    platform: str
    """e.g. ``"Windows"``, ``"Linux"``, ``"macOS"``. Emitted as
    ``Sec-CH-UA-Platform``."""

    platform_version: str
    """e.g. ``"10.0.0"`` for Windows 10, ``"14.2.1"`` for macOS."""

    architecture: str
    """CPU architecture, e.g. ``"x86"`` or ``"arm"``."""

    model: str
    """Device model — desktops send empty string."""

    mobile: bool
    """Whether the UA should be treated as mobile (sets ``Sec-CH-UA-Mobile``)."""

    bitness: str | None = None
    """e.g. ``"64"``. Emitted as ``Sec-CH-UA-Bitness``."""

    wow64: bool = False
    """Emitted as ``Sec-CH-UA-WoW64``. Only meaningful on 32-bit Windows."""

    form_factors: list[str] | None = None
    """e.g. ``["Desktop"]``, ``["Mobile"]``, ``["Tablet"]``. Emitted as
    ``Sec-CH-UA-Form-Factors``."""


class NetworkConfig(BaseModel):
    """HTTP headers, proxy, throttling, URL blocking."""

    model_config = ConfigDict(extra="forbid")

    user_agent: str | None = None

    user_agent_metadata: UserAgentMetadata | None = None
    """Structured ``Sec-CH-UA-*`` client-hint metadata. Paired with
    ``user_agent`` for consistent browser impersonation — without it, sites
    that compare the UA string against the client-hint brands see a
    mismatch. See :class:`UserAgentMetadata`."""

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


class ScriptsConfig(BaseModel):
    """Declarative JavaScript injection for the Client's pooled pages.

    All entries are registered via CDP's
    ``Page.addScriptToEvaluateOnNewDocument``. Timing variants are
    implemented by wrapping the source inside a synchronous event-listener
    registration in an ``on_new_document`` wrapper.

    Known limitations (CDP-level, not blazeweb's):

    * Scripts do NOT propagate into cross-origin iframes. Anti-bot scripts
      that run inside a cross-origin iframe (e.g. Cloudflare Turnstile) are
      unaffected.
    * Scripts do NOT run in Service Workers / Shared Workers.
    * Runtime changes to this config affect only *new* pool pages. Pages
      already in the pool keep their original registrations — close the
      Client and open a fresh one to re-apply everywhere.
    """

    model_config = ConfigDict(extra="forbid")

    on_new_document: list[str] = Field(default_factory=list)
    """Runs before any page script, on every navigation commit. The canonical
    CDP primitive (``Page.addScriptToEvaluateOnNewDocument``)."""

    on_dom_content_loaded: list[str] = Field(default_factory=list)
    """Runs when ``DOMContentLoaded`` fires. Sugar — implemented as
    ``document.addEventListener('DOMContentLoaded', ...)`` inside an
    on-new-document wrapper."""

    on_load: list[str] = Field(default_factory=list)
    """Runs when ``window.load`` fires. Sugar — implemented as
    ``window.addEventListener('load', ...)`` inside an on-new-document
    wrapper."""

    isolated_world: list[str] = Field(default_factory=list)
    """Runs in a named isolated JavaScript world (``blazeweb_isolated``)
    where page scripts cannot read or tamper with global state. Use for
    stealth patches that anti-bot JS shouldn't observe."""

    url_scoped: dict[str, list[str]] = Field(default_factory=dict)
    """Scripts gated to URLs containing the key as a substring. Sugar —
    each entry is wrapped as
    ``if (location.href.indexOf('<key>') !== -1) { ... }`` inside an
    on-new-document script. For richer matching (regex/glob), put the
    logic inside the script body and use ``on_new_document`` directly."""


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

    capture_console_level: Literal["all", "warn", "error"] = "error"
    """Level threshold for ``RenderResult.console_messages`` capture.

    - ``"error"`` (default) — only ``console.error`` and uncaught exceptions.
      Matches pre-Phase-1 behavior; minimum overhead.
    - ``"warn"`` — adds ``console.warn``.
    - ``"all"`` — captures every standard ``console.*`` method (log, info,
      warning, error, debug, trace).

    Captured at Client construction. Runtime updates via ``update_config``
    do not re-arm the listeners on already-pooled pages.
    """

    viewport: ViewportConfig = Field(default_factory=ViewportConfig)
    network: NetworkConfig = Field(default_factory=NetworkConfig)
    emulation: EmulationConfig = Field(default_factory=EmulationConfig)
    scripts: ScriptsConfig = Field(default_factory=ScriptsConfig)
    timeout: TimeoutConfig = Field(default_factory=TimeoutConfig)
    chrome: ChromeConfig = Field(default_factory=ChromeConfig)

    @classmethod
    def from_flat(cls, **kwargs: Any) -> ClientConfig:
        """Build a ClientConfig from flat kwargs.

        Powers the ``Client(viewport=(w,h), user_agent=..., concurrency=16)``
        shortcut.
        """
        # Maps flat kwarg → (sub_config_name, field_name).
        flat_map: dict[str, tuple[str, str]] = {
            "device_scale_factor": ("viewport", "device_scale_factor"),
            "mobile": ("viewport", "mobile"),
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

        # scripts={...} passes through as a whole sub-config (pydantic coerces
        # the dict to ScriptsConfig, or accepts a ScriptsConfig directly).
        if "scripts" in kwargs:
            top["scripts"] = kwargs.pop("scripts")

        for top_field in (
            "concurrency",
            "wait_until",
            "wait_after_ms",
            "capture_console_level",
        ):
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

    scripts: list[str] = Field(default_factory=list)
    """JavaScript snippets to register via
    ``Page.addScriptToEvaluateOnNewDocument`` BEFORE navigation. Each
    string is a complete script body; it runs before any page-side script.
    Stacks on top of any Client-level ``scripts.on_new_document``. Removed
    after capture so they don't leak to subsequent fetches on the same
    pooled tab."""

    block_urls: list[str] = Field(default_factory=list)
    """URL patterns to block at the network layer for this call. Additive
    over the Client's base ``network.block_urls`` — both apply. Pattern
    syntax matches CDP ``Network.setBlockedURLs`` (supports ``*``
    wildcards). Restored to the Client-level base list after capture so
    the per-call block doesn't leak to subsequent fetches on the same
    pooled tab."""

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
    "ScriptsConfig",
    "TimeoutConfig",
    "UserAgentBrandVersion",
    "UserAgentMetadata",
    "ViewportConfig",
]
