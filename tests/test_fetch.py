"""Tests for blazeweb.fetch() — URL-based rendering API."""

import blazeweb

# httpbin.org has a valid cert chain trusted by webpki-roots.
# example.com uses an outdated cross-signed chain with a root removed from Mozilla's store.
HTTPS_URL = "https://httpbin.org/html"
HTTP_URL = "http://example.com"


class TestFetchTopLevel:
    """Tests for the top-level blazeweb.fetch() function."""

    def test_fetch_https(self):
        """fetch() works with HTTPS URLs."""
        result = blazeweb.fetch(HTTPS_URL)
        assert isinstance(result, blazeweb.RenderResult)
        assert len(result) > 0
        assert "<html" in result.lower() or "<!doctype" in result.lower() or "<h1>" in result.lower()

    def test_fetch_http(self):
        """fetch() works with HTTP URLs."""
        result = blazeweb.fetch(HTTP_URL)
        assert isinstance(result, blazeweb.RenderResult)
        assert "Example Domain" in result

    def test_fetch_has_errors_attr(self):
        """Result has an errors attribute (list)."""
        result = blazeweb.fetch(HTTP_URL)
        assert isinstance(result.errors, list)

    def test_fetch_html_property(self):
        """Result.html returns the HTML string."""
        result = blazeweb.fetch(HTTP_URL)
        assert result.html == str(result)

    def test_fetch_is_string(self):
        """RenderResult is a str subclass — works with string ops."""
        result = blazeweb.fetch(HTTP_URL)
        assert isinstance(result, str)

    def test_fetch_invalid_url(self):
        """Garbage URL raises RuntimeError."""
        try:
            blazeweb.fetch("not-a-url")
            assert False, "Should have raised"
        except RuntimeError as e:
            assert "invalid URL" in str(e).lower() or "url" in str(e).lower()

    def test_fetch_nonexistent_domain(self):
        """Non-existent domain raises RuntimeError."""
        try:
            blazeweb.fetch("https://this-domain-does-not-exist-blazeweb-test.invalid")
            assert False, "Should have raised"
        except RuntimeError:
            pass  # Any RuntimeError is fine

    def test_fetch_404(self):
        """HTTP 404 raises RuntimeError."""
        try:
            blazeweb.fetch("https://httpbin.org/status/404")
            assert False, "Should have raised"
        except RuntimeError as e:
            assert "404" in str(e)


class TestFetchClient:
    """Tests for Client.fetch() method."""

    def test_client_fetch_basic(self):
        """Client.fetch() returns a RenderResult."""
        client = blazeweb.Client()
        result = client.fetch(HTTP_URL)
        assert isinstance(result, blazeweb.RenderResult)
        assert "Example Domain" in result

    def test_client_fetch_https(self):
        """Client.fetch() works with HTTPS."""
        client = blazeweb.Client()
        result = client.fetch(HTTPS_URL)
        assert isinstance(result, blazeweb.RenderResult)
        assert len(result) > 0

    def test_client_fetch_caching(self):
        """Second fetch of same URL uses cached scripts."""
        client = blazeweb.Client()
        result1 = client.fetch(HTTP_URL)
        result2 = client.fetch(HTTP_URL)
        # Both should succeed and produce HTML
        assert len(result1) > 0
        assert len(result2) > 0

    def test_client_fetch_cache_disabled(self):
        """cache=False disables caching for the call."""
        client = blazeweb.Client()
        result = client.fetch(HTTP_URL, cache=False)
        assert isinstance(result, blazeweb.RenderResult)
        assert len(result) > 0

    def test_client_fetch_invalid_url(self):
        """Client.fetch() with bad URL raises RuntimeError."""
        client = blazeweb.Client()
        try:
            client.fetch("not-a-url")
            assert False, "Should have raised"
        except RuntimeError:
            pass


class TestFetchInModule:
    """Verify fetch is properly exported."""

    def test_fetch_in_all(self):
        """fetch is listed in __all__."""
        assert "fetch" in blazeweb.__all__

    def test_fetch_callable(self):
        """fetch is callable at module level."""
        assert callable(blazeweb.fetch)
