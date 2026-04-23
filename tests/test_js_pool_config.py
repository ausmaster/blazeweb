"""Tests for `Client(js_workers=N, js_timeout_ms=M)` configuration.

Verifies the per-Client executor pool size and JS timeout settings actually
take effect.
"""
from __future__ import annotations

import concurrent.futures
import textwrap
import time

import pytest

import blazeweb


# ─── js_workers ───────────────────────────────────────────────────────────

def test_client_js_workers_default_works():
    """Default Client (no js_workers specified) renders correctly."""
    client = blazeweb.Client()
    r = client.render(b"<html><body><script>document.title='ok'</script></body></html>")
    assert "<title>ok</title>" in r


def test_client_js_workers_explicit_one():
    """Explicit js_workers=1 still works."""
    client = blazeweb.Client(js_workers=1)
    r = client.render(b"<html><body><script>document.title='ok'</script></body></html>")
    assert "<title>ok</title>" in r


def test_client_js_workers_eight_handles_concurrent_renders():
    """Client(js_workers=8) handles 32 parallel renders correctly."""
    client = blazeweb.Client(js_workers=8)

    def render(i):
        html = f"<html><body><script>document.title='r{i}'</script></body></html>".encode()
        return i, str(client.render(html))

    with concurrent.futures.ThreadPoolExecutor(max_workers=32) as ex:
        results = list(ex.map(render, range(32)))

    for i, html in results:
        assert f"<title>r{i}</title>" in html, (
            f"render {i} got wrong output: {html[:200]}"
        )


def test_two_clients_have_independent_pools():
    """Two Client instances must have independent executor pools and state."""
    c1 = blazeweb.Client(js_workers=2)
    c2 = blazeweb.Client(js_workers=2)
    # Render in c1, leave window state.
    c1.render(b"<html><body><script>window.__c1 = 'c1';</script></body></html>")
    # Render in c2 — should see no state from c1.
    r = c2.render(
        b"<html><body><script>"
        b"document.title = (typeof window.__c1 === 'undefined') ? 'fresh' : 'LEAKED';"
        b"</script></body></html>"
    )
    assert "<title>fresh</title>" in r, f"state leaked between Clients: {r}"


# ─── js_timeout_ms ────────────────────────────────────────────────────────

def test_client_js_timeout_kills_infinite_loop():
    """Client(js_timeout_ms=500) kills a runaway script within ~1s."""
    client = blazeweb.Client(js_workers=1, js_timeout_ms=500)
    html = b"<html><body><script>while(true){}</script></body></html>"
    t0 = time.time()
    r = client.render(html)
    elapsed = time.time() - t0
    assert elapsed < 2.0, f"timeout didn't fire in time: {elapsed:.2f}s"
    # The render returns; errors should report the termination.
    assert r.errors, f"expected timeout errors, got: {r.errors}"


def test_client_isolate_recovers_after_timeout():
    """After a timeout-killed render, the Client serves the next render."""
    client = blazeweb.Client(js_workers=1, js_timeout_ms=500)
    # Render 1: runaway, gets killed.
    client.render(b"<html><body><script>while(true){}</script></body></html>")
    # Render 2: must succeed.
    r = client.render(b"<html><body><script>document.title='ok'</script></body></html>")
    assert "<title>ok</title>" in r, f"render 2 didn't recover: {r}"
    assert not r.errors, f"render 2 had errors: {r.errors}"


def test_client_default_timeout_is_ten_seconds():
    """Default JS timeout is 10s — a 200ms loop completes well within it."""
    client = blazeweb.Client(js_workers=1)
    html = textwrap.dedent("""\
        <html><body><script>
        var t0 = Date.now();
        while (Date.now() - t0 < 200) {}
        document.title = 'done';
        </script></body></html>
    """).encode()
    r = client.render(html)
    assert "<title>done</title>" in r
    assert not r.errors
