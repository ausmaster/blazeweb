"""blazeclient - A Rust-powered HTML + JavaScript execution engine."""

from __future__ import annotations

from blazeclient._blazeclient import render as _render

__all__ = ["render"]


def render(
    html: bytes | str,
    *,
    base_url: str | None = None,
) -> str:
    """Render HTML, executing any inline and external JavaScript.

    Args:
        html: The HTML document to render. If str, encoded to UTF-8.
        base_url: Base URL for resolving relative script src attributes.

    Returns:
        The final HTML string after JavaScript execution.
    """
    if isinstance(html, str):
        html = html.encode("utf-8")

    return _render(html, base_url=base_url)
