"""Interactive async Session API — coverage for every primitive DOMino needs.

The Session lives behind a pyo3-async-runtimes bridge: every CDP-touching
method returns a Python awaitable. All tests here are ``async def`` and
rely on ``pytest-asyncio`` (module-level ``pytestmark``). Most tests use
``pytest-httpserver`` to avoid hitting real websites, keeping the suite
fast and hermetic. A single benchmark-marked test exercises the Alert_fire
detector flow end-to-end against a ``data:`` URL.
"""

from __future__ import annotations

import blazeweb
import pytest

pytestmark = pytest.mark.asyncio


# ---------------------------------------------------------------------------
# Lifecycle
# ---------------------------------------------------------------------------


async def test_context_manager_opens_and_closes():
    with blazeweb.Client() as c:
        async with c.session() as s:
            assert isinstance(s, blazeweb.Session)
        # __aexit__ returned; idempotent re-close is safe.
        async with c.session() as s2:
            await s2.sleep(1)


async def test_session_counts_against_concurrency():
    """A Client with concurrency=1 allows only one Session at a time; a
    second session() opens only after the first closes (no deadlock)."""
    import asyncio

    with blazeweb.Client(concurrency=1) as c:
        async with c.session() as _:
            # Second session waits on the semaphore — kick it off and check
            # that it blocks on __aenter__ until we close the outer one.
            second_ctx = c.session()
            gate = asyncio.create_task(second_ctx.__aenter__())
            # Give the task a chance to run; should still be pending.
            await asyncio.sleep(0.1)
            assert not gate.done(), "second session opened while first held permit"
        # First is closed now — the gate resolves.
        second_session = await gate
        await second_session.__aexit__(None, None, None)


# ---------------------------------------------------------------------------
# Navigation / content / url / sleep
# ---------------------------------------------------------------------------


async def test_goto_and_content(httpserver):
    httpserver.expect_request("/").respond_with_data(
        "<html><body><h1>test-marker</h1></body></html>",
        content_type="text/html",
    )
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto(httpserver.url_for("/"))
            html = await s.content()
            assert "test-marker" in html
            assert s.url.startswith(httpserver.url_for("/"))


async def test_goto_timeout_raises():
    """A 1 ms timeout against a real network load reliably times out."""
    with blazeweb.Client() as c:
        async with c.session() as s:
            with pytest.raises(RuntimeError, match="timeout"):
                await s.goto("https://example.com/", timeout_ms=1)


async def test_wait_until_domcontentloaded(httpserver):
    httpserver.expect_request("/dcl").respond_with_data(
        "<html><body>dcl-test</body></html>",
        content_type="text/html",
    )
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto(httpserver.url_for("/dcl"), wait_until="domcontentloaded")
            assert "dcl-test" in await s.content()


async def test_sleep_waits():
    import time

    with blazeweb.Client() as c:
        async with c.session() as s:
            t0 = time.perf_counter()
            await s.sleep(150)
            assert (time.perf_counter() - t0) * 1000 >= 140


# ---------------------------------------------------------------------------
# JS evaluate
# ---------------------------------------------------------------------------


async def test_evaluate_primitives():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto("data:text/html,<html><body></body></html>",
                         wait_until="domcontentloaded")
            assert await s.evaluate("1 + 2") == 3
            assert await s.evaluate("'hello'") == "hello"
            assert await s.evaluate("true") is True
            assert await s.evaluate("null") is None
            assert await s.evaluate("undefined") is None


async def test_evaluate_objects():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto("data:text/html,<body>", wait_until="domcontentloaded")
            assert await s.evaluate("[1, 2, 3]") == [1, 2, 3]
            assert await s.evaluate("({a: 1, b: 'x'})") == {"a": 1, "b": "x"}
            nested = await s.evaluate("({items: [{id: 1}, {id: 2}]})")
            assert nested == {"items": [{"id": 1}, {"id": 2}]}


async def test_evaluate_prototype_pollution_pattern():
    """Mirrors DOMino's Prototype_pollution detector."""
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto("data:text/html,<body>", wait_until="domcontentloaded")
            await s.evaluate("Object.prototype.__bwTestPoll = 'yes'")
            result = await s.evaluate(
                "Object.prototype.hasOwnProperty('__bwTestPoll') "
                "? Object.prototype['__bwTestPoll'] : undefined"
            )
            assert result == "yes"


# ---------------------------------------------------------------------------
# Init script + console buffer (Alert_fire-style flow)
# ---------------------------------------------------------------------------


async def test_init_script_fires_before_page_script():
    """Mirrors DOMino's Alert_fire detector. Inject an alert hook BEFORE
    navigation, then visit a page that triggers alert() — the hook
    converts to console.log, which lands in the console buffer."""
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.add_init_script(
                "window.alert = m => console.log('ALERT CALLED: ' + m);"
            )
            await s.goto("data:text/html,<script>alert('payload-xyz')</script>",
                         wait_until="domcontentloaded")
            await s.sleep(100)
            alerts = [m for m in s.console_messages if "ALERT CALLED" in m.text]
            assert alerts, f"no alert captured; got {[m.text for m in s.console_messages]}"
            assert "payload-xyz" in alerts[0].text
            assert alerts[0].type == "log"


async def test_console_buffer_captures_all_levels():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto(
                "data:text/html,<script>"
                "console.log('l');console.warn('w');console.error('e');"
                "console.info('i');console.debug('d');"
                "</script>",
                wait_until="domcontentloaded",
            )
            await s.sleep(100)
            by_type = {m.type: m.text for m in s.console_messages}
            assert by_type.get("log") == "l"
            assert by_type.get("warning") == "w"
            assert by_type.get("error") == "e"
            assert by_type.get("info") == "i"
            assert by_type.get("debug") == "d"


async def test_clear_console_resets_buffer():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto("data:text/html,<script>console.log('hi')</script>",
                         wait_until="domcontentloaded")
            await s.sleep(50)
            assert s.console_messages
            s.clear_console()
            assert s.console_messages == []


# ---------------------------------------------------------------------------
# Query + LiveElement
# ---------------------------------------------------------------------------


async def test_query_returns_live_element():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto("data:text/html,<p class='hello'>hi there</p>",
                         wait_until="domcontentloaded")
            el = await s.query("p.hello")
            assert el is not None
            assert isinstance(el, blazeweb.LiveElement)
            assert await el.inner_text() == "hi there"
            assert await el.get_attribute("class") == "hello"


async def test_query_miss_returns_none():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto("data:text/html,<body>", wait_until="domcontentloaded")
            assert await s.query(".does-not-exist") is None


async def test_query_all_returns_list():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto(
                "data:text/html,<ul><li>a</li><li>b</li><li>c</li></ul>",
                wait_until="domcontentloaded",
            )
            items = await s.query_all("li")
            assert len(items) == 3
            texts = [await el.inner_text() for el in items]
            assert texts == ["a", "b", "c"]


async def test_live_element_fill_dispatches_events():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto(
                "data:text/html,"
                "<input id='in'>"
                "<script>"
                "document.getElementById('in').addEventListener("
                "'input', e => document.title = 'input:' + e.target.value);"
                "</script>",
                wait_until="domcontentloaded",
            )
            inp = await s.query("#in")
            await inp.fill("TYPED")
            title = await s.evaluate("document.title")
            assert title == "input:TYPED"


async def test_live_element_evaluate():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto(
                "data:text/html,<a href='https://ex.com/a' data-v='42'>link</a>",
                wait_until="domcontentloaded",
            )
            a = await s.query("a")
            result = await a.evaluate(
                "function() { return {href: this.href, data: this.dataset.v}; }"
            )
            assert result["href"] == "https://ex.com/a"
            assert result["data"] == "42"


# ---------------------------------------------------------------------------
# wait_for_selector
# ---------------------------------------------------------------------------


async def test_wait_for_selector_hits():
    """Element appears asynchronously; wait_for_selector returns it."""
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto(
                "data:text/html,<div id='r'></div>"
                "<script>setTimeout(() => {"
                "let el = document.createElement('span');"
                "el.id = 'late'; el.textContent = 'found'; "
                "document.getElementById('r').appendChild(el); }, 200);"
                "</script>",
                wait_until="domcontentloaded",
            )
            el = await s.wait_for_selector("#late", timeout_ms=2000)
            assert await el.inner_text() == "found"


async def test_wait_for_selector_timeout_raises():
    with blazeweb.Client() as c:
        async with c.session() as s:
            await s.goto("data:text/html,<body>", wait_until="domcontentloaded")
            with pytest.raises(TimeoutError, match="timed out"):
                await s.wait_for_selector(".never-appears", timeout_ms=100)


# ---------------------------------------------------------------------------
# Block filters
# ---------------------------------------------------------------------------


async def test_block_resources_blocks_images():
    HTML = (
        "<img src='https://upload.wikimedia.org/wikipedia/commons/thumb/"
        "8/8a/Pixel-Art-Landscape.png/200px-Pixel-Art-Landscape.png' "
        "onerror=\"document.title='IMG_BLOCKED'\" "
        "onload=\"document.title='IMG_LOADED'\">"
    )
    with blazeweb.Client() as c:
        async with c.session(block_resources=["image"]) as s:
            await s.goto(f"data:text/html,{HTML}")
            await s.sleep(300)
            assert (await s.evaluate("document.title")) == "IMG_BLOCKED"


async def test_block_urls_substring_match():
    HTML = (
        "<img src='https://doubleclick.net/px.gif' "
        "onerror=\"document.title='BLOCKED'\" "
        "onload=\"document.title='LOADED'\">"
    )
    with blazeweb.Client() as c:
        async with c.session(block_urls=["doubleclick"]) as s:
            await s.goto(f"data:text/html,{HTML}")
            await s.sleep(300)
            assert (await s.evaluate("document.title")) == "BLOCKED"


async def test_block_navigation_setter_requires_opt_in():
    """Without any initial block-* kwarg, runtime setters raise."""
    with blazeweb.Client() as c:
        async with c.session() as s:
            with pytest.raises(RuntimeError, match="Fetch interception"):
                await s.block_navigation(True)


async def test_block_navigation_toggles_at_runtime(httpserver):
    """Opt in via block_navigation=False at creation, then flip to True.
    Real HTTP navigation (data: URIs aren't intercepted by Fetch)."""
    httpserver.expect_request("/start").respond_with_data(
        "<html><body>start</body></html>", content_type="text/html"
    )
    httpserver.expect_request("/after").respond_with_data(
        "<html><body>after</body></html>", content_type="text/html"
    )
    httpserver.expect_request("/blocked").respond_with_data(
        "<html><body>blocked</body></html>", content_type="text/html"
    )
    with blazeweb.Client() as c:
        async with c.session(block_navigation=False) as s:
            await s.goto(httpserver.url_for("/start"), wait_until="domcontentloaded")
            assert "start" in await s.content()
            await s.goto(httpserver.url_for("/after"), wait_until="domcontentloaded")
            assert "after" in await s.content()
            # Flip on — subsequent navigation fails because Fetch.failRequest
            # aborts the Document request before the response commits.
            await s.block_navigation(True)
            with pytest.raises(RuntimeError):
                await s.goto(
                    httpserver.url_for("/blocked"),
                    wait_until="domcontentloaded",
                    timeout_ms=1500,
                )
