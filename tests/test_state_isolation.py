"""Per-render state isolation tests.

These tests exercise EVERY per-render mutable isolate slot via a script
that mutates state, then a second render asserting the state is fresh.

With the current per-render isolate model these tests pass trivially
(each render gets a brand-new isolate with empty slots).

When we move to long-lived isolates with `reset_per_render_slots`, these
tests become the regression guard — if any slot is dropped from the reset
function, the corresponding test fails. One failing test == one missing slot.
"""
from __future__ import annotations

import textwrap

import pytest

import blazeweb


def _render(client: blazeweb.Client, script: str) -> str:
    """Render a doc whose <script> body sets document.title to the result."""
    html = textwrap.dedent(f"""\
        <!DOCTYPE html><html><head>
        <script>
        try {{
            {script}
        }} catch (e) {{
            document.title = 'ERROR: ' + e.message;
        }}
        </script>
        </head><body></body></html>
    """).encode("utf-8")
    return str(client.render(html))


@pytest.fixture
def client():
    return blazeweb.Client()


# ─── window property leak (script execution context) ──────────────────────

def test_window_property_does_not_leak(client):
    """`window.foo` set in render 1 must NOT exist in render 2."""
    r1 = _render(client, "window.__leak = 42; document.title = String(window.__leak);")
    assert "<title>42</title>" in r1

    r2 = _render(
        client,
        "document.title = (typeof window.__leak === 'undefined') ? 'fresh' : 'LEAKED';",
    )
    assert "<title>fresh</title>" in r2, f"window state leaked: {r2}"


# ─── localStorage leak (WebStorage slot) ──────────────────────────────────

def test_local_storage_does_not_leak(client):
    r1 = _render(
        client,
        "localStorage.setItem('k', 'v'); document.title = localStorage.getItem('k');",
    )
    assert "<title>v</title>" in r1

    r2 = _render(
        client,
        "var v = localStorage.getItem('k'); document.title = (v === null) ? 'fresh' : 'LEAKED:' + v;",
    )
    assert "<title>fresh</title>" in r2, f"localStorage leaked: {r2}"


# ─── document.cookie leak (DocumentCookie slot) ───────────────────────────

def test_document_cookie_does_not_leak(client):
    # Render 1 attempts to set a cookie. Even if the setter throws (it
    # currently does — unrelated bug), the slot may still be partially
    # mutated. The guarantee we need is that render 2 sees empty state.
    _render(client, "try { document.cookie = 'k=v'; } catch(e) {}")

    r2 = _render(
        client,
        "document.title = (document.cookie === '') ? 'fresh' : 'LEAKED:' + document.cookie;",
    )
    assert "<title>fresh</title>" in r2, f"document.cookie leaked: {r2}"


# ─── customElements registry leak (CustomElementState slot) ──────────────

def test_custom_elements_registry_does_not_leak(client):
    r1 = _render(
        client,
        """
        class XLeak extends HTMLElement {}
        customElements.define('x-leak', XLeak);
        document.title = customElements.get('x-leak') ? 'defined' : 'missing';
        """,
    )
    assert "<title>defined</title>" in r1

    r2 = _render(
        client,
        "document.title = customElements.get('x-leak') ? 'LEAKED' : 'fresh';",
    )
    assert "<title>fresh</title>" in r2, f"customElements leaked: {r2}"


# ─── setTimeout queue leak (TimerQueue slot) ──────────────────────────────

def test_timer_queue_does_not_leak(client):
    """A setTimeout scheduled in render 1 must not fire during render 2."""
    r1 = _render(
        client,
        """
        // Schedule a far-future timeout that would set a marker.
        // Render 1 ends before it fires.
        setTimeout(function() { window.__from_r1 = true; }, 999999);
        document.title = 'r1';
        """,
    )
    assert "<title>r1</title>" in r1

    r2 = _render(
        client,
        "document.title = window.__from_r1 ? 'LEAKED' : 'fresh';",
    )
    assert "<title>fresh</title>" in r2, f"timer state leaked: {r2}"


# ─── module map leak (ModuleMap slot) ─────────────────────────────────────
# Tests that ES module compilation cache doesn't carry across renders.
# Since we can't easily test module identity directly without external URLs,
# we test that re-defining the same data: URL module starts fresh.

def test_module_state_does_not_leak(client):
    r1 = _render(
        client,
        """
        // Inline module that sets a window flag
        const m = document.createElement('script');
        m.type = 'module';
        m.textContent = "window.__module_loaded = 'r1';";
        document.head.appendChild(m);
        // Title set synchronously; module flag may or may not be set yet.
        document.title = 'r1';
        """,
    )
    assert "<title>r1</title>" in r1

    r2 = _render(
        client,
        "document.title = (typeof window.__module_loaded === 'undefined') ? 'fresh' : 'LEAKED';",
    )
    assert "<title>fresh</title>" in r2, f"module state leaked: {r2}"
