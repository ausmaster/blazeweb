"""CDP error context: timeout / evaluate-script errors carry diagnostic
context in their messages so consumers can identify which script index
threw, which lifecycle event was awaited, and which URL was navigating.

Today's bare ``RuntimeError("CDP: Request timed out.")`` carries zero
diagnostic context; debugging takes multiple probe rounds. The fix
extends ``BlazeError::NavigationTimeout`` and adds
``BlazeError::PostLoadScript`` to surface this metadata in the error
message that reaches Python.
"""

from __future__ import annotations

import base64

import blazeweb
import pytest
from pytest_httpserver import HTTPServer


def _data_url(html: bytes) -> str:
    return "data:text/html;base64," + base64.b64encode(html).decode()


def test_navigation_timeout_includes_lifecycle_context(httpserver: HTTPServer) -> None:
    """A navigation that never reaches the lifecycle event must surface the
    URL, the awaited lifecycle stage, and the timeout duration in the
    RuntimeError message — not just ``"Request timed out"``."""
    # Server hangs on the request: receive the connection but never respond.
    # pytest-httpserver doesn't natively expose "block forever"; we simulate
    # by responding with content_type we'll never finish loading. Easier:
    # use a bound port that accepts but never responds.
    import socket

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(("127.0.0.1", 0))
    sock.listen(1)
    port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}/hangs"

    try:
        with blazeweb.Client() as c, pytest.raises(RuntimeError) as exc:
            c.fetch(url, timeout_ms=1500)
        msg = str(exc.value)
        # Must mention the URL.
        assert url in msg, f"expected URL in error: {msg!r}"
        # Must mention the lifecycle target (default Load).
        assert "load" in msg.lower(), f"expected lifecycle name in error: {msg!r}"
        # Must mention the timeout duration.
        assert "1500" in msg, f"expected timeout ms in error: {msg!r}"
    finally:
        sock.close()


def test_post_load_script_error_includes_script_index() -> None:
    """A post_load_script that throws must surface its index in the error."""
    url = _data_url(b"<html><body>x</body></html>")
    with blazeweb.Client() as c, pytest.raises(RuntimeError) as exc:
        c.fetch(
            url,
            post_load_scripts=[
                "1 + 1",
                "throw new Error('SECOND_THREW')",
                "2 + 2",
            ],
        )
    msg = str(exc.value)
    # Index of the throwing script (1) must appear in the message.
    assert "post_load_scripts[1]" in msg, f"expected script index in error: {msg!r}"
    # Original error message must still propagate.
    assert "SECOND_THREW" in msg, f"expected JS error text in error: {msg!r}"


def test_post_load_script_first_index_zero() -> None:
    """Make sure 0 is used (not 1) for the first script index."""
    url = _data_url(b"<html><body>x</body></html>")
    with blazeweb.Client() as c, pytest.raises(RuntimeError) as exc:
        c.fetch(url, post_load_scripts=["throw new Error('FIRST')"])
    msg = str(exc.value)
    assert "post_load_scripts[0]" in msg, f"expected 0-indexed script: {msg!r}"
    assert "FIRST" in msg
