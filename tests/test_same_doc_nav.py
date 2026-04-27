"""Same-document navigation lifecycle: hash-only / query+hash navs that
chromium treats as same-document do not fire ``Page.loadEventFired`` or
``Page.domContentEventFired``. blazeweb must subscribe to
``Page.navigatedWithinDocument`` and unblock the lifecycle wait on it.

Without the fix, the second hash fetch on a shared pool tab times out; with
the fix, every hash fetch completes promptly. ``concurrency=1`` forces all
fetches onto the same tab so the pool can't accidentally route around the
issue.
"""

from __future__ import annotations

import time

import blazeweb
import pytest
from pytest_httpserver import HTTPServer


def _serve_simple(httpserver: HTTPServer) -> str:
    httpserver.expect_request("/").respond_with_data(
        "<html><body><h1>same-doc test</h1></body></html>",
        content_type="text/html",
    )
    return httpserver.url_for("/")


def test_hash_only_nav_completes_without_timeout(httpserver: HTTPServer) -> None:
    base = _serve_simple(httpserver)
    with blazeweb.Client(concurrency=1) as c:
        t0 = time.perf_counter()
        r1 = c.fetch(base)
        r2 = c.fetch(base + "#abc")
        r3 = c.fetch(base + "#xyz")
        elapsed = time.perf_counter() - t0
    assert r1.status_code == 200
    assert r2.status_code == 200
    assert r3.status_code == 200
    # Three fetches on a shared tab should be fast; ≥10s would only happen
    # if a same-doc nav timed out (default nav timeout 30s).
    assert elapsed < 8.0, f"3 hash fetches took {elapsed:.1f}s — same-doc nav likely timed out"


def test_query_only_nav_then_hash_change(httpserver: HTTPServer) -> None:
    base = _serve_simple(httpserver)
    with blazeweb.Client(concurrency=1) as c:
        r1 = c.fetch(base + "?q=1")
        r2 = c.fetch(base + "?q=1#abc")  # same-doc from r1
    assert r1.status_code == 200
    assert r2.status_code == 200


@pytest.mark.asyncio
async def test_async_client_parity(httpserver: HTTPServer) -> None:
    base = _serve_simple(httpserver)
    async with blazeweb.AsyncClient(concurrency=1) as ac:
        r1 = await ac.fetch(base)
        r2 = await ac.fetch(base + "#a")
        r3 = await ac.fetch(base + "#b")
    assert r1.status_code == 200
    assert r2.status_code == 200
    assert r3.status_code == 200


def test_dcl_mode_same_doc_nav(httpserver: HTTPServer) -> None:
    """``wait_until='domcontentloaded'`` mode also handles same-doc navs."""
    base = _serve_simple(httpserver)
    with blazeweb.Client(concurrency=1, wait_until="domcontentloaded") as c:
        r1 = c.fetch(base)
        r2 = c.fetch(base + "#abc")
    assert r1.status_code == 200
    assert r2.status_code == 200
