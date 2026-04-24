"""Pydantic config hierarchy — construction, validation, env loading, flat kwargs."""

from __future__ import annotations

import os

import pytest

import blazeweb
from blazeweb import (
    ChromeConfig,
    ClientConfig,
    EmulationConfig,
    FetchConfig,
    NetworkConfig,
    ScreenshotConfig,
    TimeoutConfig,
    ViewportConfig,
)


class TestDefaults:
    def test_default_values(self):
        c = ClientConfig()
        assert c.concurrency == 16
        assert c.viewport.width == 1200
        assert c.viewport.height == 800
        assert c.viewport.device_scale_factor == 1.0
        assert c.viewport.mobile is False
        assert c.network.user_agent is None
        assert c.network.extra_headers == {}
        assert c.emulation.javascript_enabled is True
        assert c.timeout.navigation_ms == 30_000
        assert c.chrome.headless is True


class TestExplicitStructured:
    def test_all_sub_configs(self):
        c = ClientConfig(
            concurrency=32,
            viewport=ViewportConfig(width=1920, height=1080, device_scale_factor=2.0),
            network=NetworkConfig(
                user_agent="Bot/1.0",
                extra_headers={"X-A": "1"},
                block_urls=["*doubleclick*"],
                ignore_https_errors=True,
            ),
            emulation=EmulationConfig(locale="ja-JP", timezone="Asia/Tokyo"),
            timeout=TimeoutConfig(navigation_ms=60_000),
            chrome=ChromeConfig(args=["--mute-audio"]),
        )
        assert c.concurrency == 32
        assert c.viewport.width == 1920
        assert c.network.user_agent == "Bot/1.0"
        assert c.emulation.locale == "ja-JP"
        assert c.timeout.navigation_ms == 60_000
        assert c.chrome.args == ["--mute-audio"]


class TestFlatKwargs:
    def test_viewport_tuple_shortcut(self):
        c = ClientConfig.from_flat(viewport=(1920, 1080))
        assert c.viewport.width == 1920
        assert c.viewport.height == 1080

    def test_network_fields(self):
        c = ClientConfig.from_flat(
            user_agent="UA",
            proxy="http://proxy:8080",
            extra_headers={"X": "1"},
            ignore_https_errors=True,
            block_urls=["*ad*"],
        )
        assert c.network.user_agent == "UA"
        assert c.network.proxy == "http://proxy:8080"
        assert c.network.ignore_https_errors is True
        assert c.network.block_urls == ["*ad*"]

    def test_emulation_fields(self):
        c = ClientConfig.from_flat(locale="en-GB", timezone="Europe/London", geolocation=(51.5, -0.13))
        assert c.emulation.locale == "en-GB"
        assert c.emulation.timezone == "Europe/London"
        assert c.emulation.geolocation == (51.5, -0.13)

    def test_timeout_fields(self):
        c = ClientConfig.from_flat(navigation_timeout_ms=60000)
        assert c.timeout.navigation_ms == 60000

    def test_chrome_fields(self):
        c = ClientConfig.from_flat(chrome_args=["--mute-audio"], headless=False)
        assert c.chrome.args == ["--mute-audio"]
        assert c.chrome.headless is False

    def test_unknown_kwarg_raises(self):
        with pytest.raises(TypeError, match="unknown ClientConfig kwarg"):
            ClientConfig.from_flat(nonsense_key=1)


class TestValidation:
    def test_viewport_bounds(self):
        with pytest.raises(Exception):  # pydantic ValidationError
            ViewportConfig(width=0)
        with pytest.raises(Exception):
            ViewportConfig(width=-1)

    def test_extra_forbidden(self):
        """extra='forbid' catches typos."""
        with pytest.raises(Exception):
            NetworkConfig(usr_agent="oops")  # typo

    def test_prefers_color_scheme_enum(self):
        EmulationConfig(prefers_color_scheme="dark")  # ok
        EmulationConfig(prefers_color_scheme="light")  # ok
        EmulationConfig(prefers_color_scheme=None)  # ok
        with pytest.raises(Exception):
            EmulationConfig(prefers_color_scheme="sepia")


class TestEnvLoading:
    """BLAZEWEB_* env vars auto-load via pydantic-settings."""

    def test_top_level_int(self, monkeypatch):
        monkeypatch.setenv("BLAZEWEB_CONCURRENCY", "42")
        c = ClientConfig()
        assert c.concurrency == 42

    def test_nested_via_double_underscore(self, monkeypatch):
        monkeypatch.setenv("BLAZEWEB_VIEWPORT__WIDTH", "2560")
        monkeypatch.setenv("BLAZEWEB_NETWORK__USER_AGENT", "envUA")
        c = ClientConfig()
        assert c.viewport.width == 2560
        assert c.network.user_agent == "envUA"

    def test_constructor_overrides_env(self, monkeypatch):
        monkeypatch.setenv("BLAZEWEB_CONCURRENCY", "99")
        c = ClientConfig(concurrency=7)
        assert c.concurrency == 7


class TestSerialization:
    def test_model_dump_round_trip(self):
        c1 = ClientConfig(
            concurrency=32,
            viewport=ViewportConfig(width=1920, height=1080),
            network=NetworkConfig(user_agent="x"),
        )
        d = c1.model_dump()
        c2 = ClientConfig.model_validate(d)
        assert c2.concurrency == 32
        assert c2.viewport.width == 1920
        assert c2.network.user_agent == "x"

    def test_json_round_trip(self):
        c1 = ClientConfig(network=NetworkConfig(extra_headers={"X": "1"}))
        s = c1.model_dump_json()
        c2 = ClientConfig.model_validate_json(s)
        assert c2.network.extra_headers == {"X": "1"}

    def test_model_copy_with_update(self):
        c1 = ClientConfig(concurrency=8)
        c2 = c1.model_copy(update={"concurrency": 16})
        assert c1.concurrency == 8
        assert c2.concurrency == 16


class TestPerCallConfigs:
    def test_fetch_config_defaults(self):
        fc = FetchConfig()
        assert fc.extra_headers == {}
        assert fc.timeout_ms is None

    def test_screenshot_config_defaults(self):
        sc = ScreenshotConfig()
        assert sc.viewport is None
        assert sc.full_page is False


class TestClientConstructors:
    """Client ctor accepts config= or flat kwargs, not both."""

    def test_defaults(self):
        c = blazeweb.Client()
        assert c.config.concurrency == 16
        c.close()

    def test_explicit_config(self):
        cfg = ClientConfig(concurrency=8, network=NetworkConfig(user_agent="x"))
        c = blazeweb.Client(config=cfg)
        assert c.config.concurrency == 8
        assert c.config.network.user_agent == "x"
        c.close()

    def test_flat_kwargs(self):
        c = blazeweb.Client(concurrency=8, user_agent="x", locale="ja-JP")
        assert c.config.concurrency == 8
        assert c.config.network.user_agent == "x"
        assert c.config.emulation.locale == "ja-JP"
        c.close()

    def test_both_raises(self):
        with pytest.raises(TypeError):
            blazeweb.Client(config=ClientConfig(), user_agent="x")

    def test_positional_args_raises(self):
        with pytest.raises(TypeError):
            blazeweb.Client("whatever")  # positional not allowed
