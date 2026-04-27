"""Per-fetch Referer header: chromium's URL loader rejects cross-origin
``Referer`` set via ``Network.setExtraHTTPHeaders`` with
``ERR_BLOCKED_BY_CLIENT`` (W3C Referrer Policy enforcement).

The fix lifts ``Referer`` out of the merged extra_headers map and passes it
as the ``referrer`` parameter on ``Page.navigate`` instead, which is the
documented path for setting the navigation referrer at the browser level.

Tests verify both wire-level (server-captured request header) and JS-level
(``document.referrer``) propagation.
"""

from __future__ import annotations

import blazeweb
import pytest
from pytest_httpserver import HTTPServer


def _serve_referrer_echo(httpserver: HTTPServer) -> str:
    httpserver.expect_request("/").respond_with_data(
        "<html><body><span id='ref'></span><script>"
        "document.getElementById('ref').textContent = document.referrer;"
        "</script></body></html>",
        content_type="text/html",
    )
    return httpserver.url_for("/")


def test_cross_origin_referer_succeeds_via_navigate(httpserver: HTTPServer) -> None:
    url = _serve_referrer_echo(httpserver)
    with blazeweb.Client() as c:
        r = c.fetch(url, extra_headers={"Referer": "http://foo.bar/X"})
    assert r.status_code == 200
    # Wire-level: server saw the cross-origin Referer.
    requests = [req for req, _ in httpserver.log if req.path == "/"]
    assert len(requests) >= 1
    referer = requests[-1].headers.get("Referer")
    assert referer == "http://foo.bar/X", f"expected cross-origin Referer; got {referer!r}"


def test_referer_appears_in_document_referrer_js(httpserver: HTTPServer) -> None:
    url = _serve_referrer_echo(httpserver)
    with blazeweb.Client() as c:
        r = c.fetch(
            url,
            extra_headers={"Referer": "http://foo.bar/PAGE"},
            wait_after_ms=100,
        )
    assert "http://foo.bar/PAGE" in r, f"document.referrer not in HTML: {r[:300]!r}"


def test_same_origin_referer_still_works(httpserver: HTTPServer) -> None:
    """Same-origin Referer worked before the fix; must still work after."""
    url = _serve_referrer_echo(httpserver)
    same_origin = httpserver.url_for("/sibling")
    with blazeweb.Client() as c:
        r = c.fetch(url, extra_headers={"Referer": same_origin})
    assert r.status_code == 200
    requests = [req for req, _ in httpserver.log if req.path == "/"]
    referer = requests[-1].headers.get("Referer")
    assert referer == same_origin


def test_referer_case_insensitive(httpserver: HTTPServer) -> None:
    """Lower-case ``referer`` (common in Python dicts) routes through the same path."""
    url = _serve_referrer_echo(httpserver)
    with blazeweb.Client() as c:
        r = c.fetch(url, extra_headers={"referer": "http://foo.bar/lc"})
    assert r.status_code == 200
    requests = [req for req, _ in httpserver.log if req.path == "/"]
    referer = requests[-1].headers.get("Referer")
    assert referer == "http://foo.bar/lc"


@pytest.mark.asyncio
async def test_async_client_referer_parity(httpserver: HTTPServer) -> None:
    url = _serve_referrer_echo(httpserver)
    async with blazeweb.AsyncClient() as ac:
        r = await ac.fetch(url, extra_headers={"Referer": "http://foo.bar/async"})
    assert r.status_code == 200
    requests = [req for req, _ in httpserver.log if req.path == "/"]
    referer = requests[-1].headers.get("Referer")
    assert referer == "http://foo.bar/async"
