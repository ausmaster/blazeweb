"""Async Session API for interactive (stateful) browser automation.

A ``Session`` is blazeweb's alternative to the one-shot ``Client.fetch()``
flow: you get a long-lived chromium page, drive it (navigate, click, fill,
evaluate JS), and observe what fires (console buffer, DOM state). Unlike
``fetch()``, a Session is async — every CDP-touching method is a coroutine.

The public entry point is ``Client.session(**kwargs)`` (async context
manager). This module hosts the ergonomics; the heavy lifting is in
``blazeweb._blazeweb._SessionInner`` / ``_LiveElementInner``.

Usage::

    import asyncio, blazeweb

    async def main():
        with blazeweb.Client() as c:
            async with c.session(viewport=(1280, 720)) as s:
                await s.goto("https://example.com")
                print(await s.content())

    asyncio.run(main())
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from blazeweb._blazeweb import _LiveElementInner, _SessionInner


@dataclass(frozen=True)
class ConsoleMessage:
    """One page-side ``console.*`` event.

    Collected automatically while a ``Session`` is open. Read via
    ``session.console_messages`` (sync; returns a snapshot list). Reset with
    ``session.clear_console()``.
    """

    type: str
    """``log`` / ``info`` / ``warning`` / ``error`` / ``debug`` / ``trace``."""

    text: str
    """The message body (already stringified by Chrome)."""

    timestamp: float
    """``time.time()`` when the event was appended to the buffer."""


class LiveElement:
    """Handle to a live DOM element bound to an open ``Session``.

    Methods are async; the element may become stale mid-session if the page
    navigates — calls after detachment will raise.

    **Behavior compared to Playwright (v1 limitations).** ``click()`` and
    ``fill()`` are intentionally minimal compared to Playwright's
    implementations in ``packages/playwright-core/src/server/dom.ts``:

    * ``click()`` scrolls into view + dispatches one mouseDown/Up at the
      element's center. No auto-wait for visibility/enabled, no hit-test
      for intercepted clicks, no retry-on-intercepted. Fine for form
      submits / link clicks; may miss dynamic modals or transient overlays.
    * ``fill()`` sets ``element.value`` via JS and dispatches ``input`` /
      ``change``. Works for text/email/password/textarea. Does NOT use
      real keystrokes (so ``type=number``, ``type=date``, and
      ``contenteditable`` may behave incorrectly).
    * ``evaluate()`` uses CDP ``Runtime.callFunctionOn`` with
      ``returnByValue: true`` (matches Playwright's
      ``CRExecutionContext.evaluateWithArguments``). Return values must
      be JSON-compatible.
    """

    __slots__ = ("_inner",)

    def __init__(self, inner: _LiveElementInner) -> None:
        self._inner = inner

    # --- interact ---------------------------------------------------------

    async def click(self, *, timeout_ms: int = 5_000) -> None:
        """Dispatch a trusted mouse click on this element."""
        await self._inner.click(timeout_ms=timeout_ms)

    async def fill(self, text: str, *, timeout_ms: int = 5_000) -> None:
        """Focus this element and type ``text`` into it (clears first)."""
        await self._inner.fill(text, timeout_ms=timeout_ms)

    # --- read -------------------------------------------------------------

    async def inner_text(self) -> str:
        """Rendered text content (matches DOM ``.innerText``)."""
        return await self._inner.inner_text()

    async def get_attribute(self, name: str) -> str | None:
        """HTML attribute value, or ``None`` if unset."""
        return await self._inner.get_attribute(name)

    async def evaluate(self, js: str) -> Any:
        """Evaluate ``js`` with the element bound as ``el`` / ``this``.
        Return value must be JSON-compatible."""
        return await self._inner.evaluate(js)


class Session:
    """Stateful chromium page handle, yielded by ``Client.session()``.

    Use as an async context manager::

        async with client.session() as s:
            await s.goto(url)

    Entering allocates a fresh chromium page and wires console / request-
    blocking listeners. Exiting closes the page and releases the pool
    permit. Sessions count against ``Client(concurrency=N)`` — opening
    more than N concurrent sessions queues the new ones asynchronously.
    """

    __slots__ = ("_inner",)

    def __init__(self, inner: _SessionInner) -> None:
        self._inner = inner

    async def __aenter__(self) -> Session:
        await self._inner.open()
        return self

    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> bool:
        await self._inner.close()
        return False  # don't suppress exceptions

    # --- navigation -------------------------------------------------------

    async def goto(
        self,
        url: str,
        *,
        timeout_ms: int = 30_000,
        referer: str | None = None,
        wait_until: str = "load",
    ) -> None:
        """Navigate to ``url``. Resolves when the chosen lifecycle event
        fires. ``wait_until`` is ``"load"`` | ``"domcontentloaded"``."""
        await self._inner.goto(
            url,
            timeout_ms=timeout_ms,
            referer=referer,
            wait_until=wait_until,
        )

    async def sleep(self, ms: int) -> None:
        """Async sleep inside the tokio runtime — mirrors Playwright's
        ``page.wait_for_timeout(ms)``."""
        await self._inner.sleep(ms)

    # --- content / state --------------------------------------------------

    async def content(self) -> str:
        """Current rendered HTML (post-JS)."""
        return await self._inner.content()

    @property
    def url(self) -> str:
        """Current document URL (updates after navigation / click flows)."""
        return self._inner.url

    # --- query / wait -----------------------------------------------------

    async def query(self, selector: str) -> LiveElement | None:
        """First element matching ``selector`` (CSS), or ``None``."""
        inner = await self._inner.query(selector)
        return LiveElement(inner) if inner is not None else None

    async def query_all(self, selector: str) -> list[LiveElement]:
        """All elements matching ``selector`` (CSS)."""
        inners = await self._inner.query_all(selector)
        return [LiveElement(i) for i in inners]

    async def wait_for_selector(
        self, selector: str, *, timeout_ms: int = 5_000
    ) -> LiveElement:
        """Wait up to ``timeout_ms`` for an element matching ``selector``.
        Raises ``TimeoutError`` on expiry."""
        inner = await self._inner.wait_for_selector(
            selector, timeout_ms=timeout_ms
        )
        return LiveElement(inner)

    # --- JS -----------------------------------------------------------------

    async def evaluate(self, js: str) -> Any:
        """Evaluate ``js`` in the page's main-world context. Return value
        must be JSON-compatible (dict / list / str / int / float / bool /
        None)."""
        return await self._inner.evaluate(js)

    async def add_init_script(self, js: str) -> None:
        """Register ``js`` to run before any page script on every navigation.
        Wraps CDP's ``Page.addScriptToEvaluateOnNewDocument``. Distinct from
        the Client-level ``config.scripts.on_new_document`` — this script
        only affects THIS session's page."""
        await self._inner.add_init_script(js)

    # --- console buffer ---------------------------------------------------

    @property
    def console_messages(self) -> list[ConsoleMessage]:
        """Snapshot list of ``ConsoleMessage``s captured since Session open
        (or last ``clear_console()``). Safe to iterate while new events land."""
        raw = self._inner.console_messages
        return [ConsoleMessage(type=t, text=x, timestamp=ts) for (t, x, ts) in raw]

    def clear_console(self) -> None:
        """Reset the console buffer."""
        self._inner.clear_console()

    # --- request blocking -------------------------------------------------

    async def block_resources(self, resource_types: list[str]) -> None:
        """Block subsequent requests by CDP ``resourceType`` (e.g.
        ``["image", "stylesheet", "font", "media"]``). Passing ``[]`` clears."""
        await self._inner.block_resources(resource_types)

    async def block_urls(self, patterns: list[str]) -> None:
        """Block subsequent requests whose URL matches any pattern (glob-
        style, e.g. ``*://*.doubleclick.net/*``). Passing ``[]`` clears."""
        await self._inner.block_urls(patterns)

    async def block_navigation(self, enabled: bool) -> None:
        """If ``True``, block navigation requests (useful to trap click
        loops that would otherwise follow links mid-scan)."""
        await self._inner.block_navigation(enabled)


__all__ = ["ConsoleMessage", "LiveElement", "Session"]
