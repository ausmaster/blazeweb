"""Tests for blazeweb.fetch() and Client.fetch() — URL-based rendering."""

from __future__ import annotations

import blazeweb
import pytest

HTTPS_URL = "https://example.com"
HTTP_URL = "http://example.com"


class TestFetchTopLevel:
    """Module-level blazeweb.fetch() — uses the shared default Client."""

    def test_fetch_https(self):
        result = blazeweb.fetch(HTTPS_URL)
        assert isinstance(result, blazeweb.RenderResult)
        assert len(result) > 0
        assert "Example Domain" in result

    def test_fetch_http(self):
        result = blazeweb.fetch(HTTP_URL)
        assert isinstance(result, blazeweb.RenderResult)
        assert "Example Domain" in result

    def test_fetch_has_errors_attr(self):
        result = blazeweb.fetch(HTTPS_URL)
        assert isinstance(result.errors, list)

    def test_fetch_has_metadata(self):
        result = blazeweb.fetch(HTTPS_URL)
        assert result.final_url.startswith("https://example.com")
        assert result.status_code == 200  # real status from main-doc response
        assert result.elapsed_s > 0

    def test_fetch_404_returns_404_status(self):
        """We capture the main-doc response status, not 200-on-any-navigation."""
        result = blazeweb.fetch("https://httpbin.org/status/404")
        assert result.status_code == 404

    def test_fetch_redirect_status_is_final(self):
        """http → https redirect: status reflects the final resource, not the 301."""
        result = blazeweb.fetch("http://httpbin.org/redirect-to?url=https://example.com")
        assert result.final_url == "https://example.com/"
        assert result.status_code == 200

    def test_fetch_html_property(self):
        result = blazeweb.fetch(HTTPS_URL)
        assert result.html == str(result)

    def test_fetch_is_string(self):
        result = blazeweb.fetch(HTTPS_URL)
        assert isinstance(result, str)

    def test_fetch_invalid_url_raises(self):
        with pytest.raises(RuntimeError):
            blazeweb.fetch("not-a-url")

    def test_fetch_nonexistent_domain_raises(self):
        with pytest.raises(RuntimeError):
            blazeweb.fetch("https://this-domain-does-not-exist-blazeweb-test.invalid")


class TestFetchClient:
    """Client.fetch() — persistent, explicit client."""

    def test_client_fetch_basic(self):
        with blazeweb.Client() as client:
            result = client.fetch(HTTPS_URL)
        assert isinstance(result, blazeweb.RenderResult)
        assert "Example Domain" in result

    def test_client_fetch_reuse(self):
        """Same client, multiple fetches — all work."""
        with blazeweb.Client() as client:
            a = client.fetch(HTTPS_URL)
            b = client.fetch(HTTPS_URL)
        assert len(a) > 0 and len(b) > 0

    def test_client_fetch_invalid_url(self):
        with blazeweb.Client() as client, pytest.raises(RuntimeError):
            client.fetch("not-a-url")


class TestRenderResult:
    """RenderResult is a str subclass with extra metadata + a lazy Rust DOM."""

    def test_is_str_subclass(self):
        result = blazeweb.fetch(HTTPS_URL)
        assert isinstance(result, str)
        # str operations work
        assert result.lower() == str(result).lower()
        assert result[:15] == str(result)[:15]

    def test_dom_lazy_parses_and_queries(self):
        result = blazeweb.fetch(HTTPS_URL)
        # Title query
        title = result.dom.title()
        assert title == "Example Domain"
        # Links query
        links = result.dom.links()
        assert isinstance(links, list)
        # Count query stops at first match
        assert result.dom.exists("h1") is True
        assert result.dom.exists("fakeneverexists") is False

    def test_repr_shape(self):
        result = blazeweb.fetch(HTTPS_URL)
        r = repr(result)
        assert r.startswith("RenderResult(")
