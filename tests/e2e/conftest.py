"""Shared helpers for E2E tests.

These are plain functions, not fixtures. Import directly from this module
or use `from conftest import text_of, render` in test files within this directory.
"""

import re
import blazeweb


def text_of(html: str, element_id: str) -> str:
    """Extract text content of an element by id from rendered HTML."""
    pattern = rf'id="{re.escape(element_id)}"[^>]*>([^<]*)<'
    m = re.search(pattern, html)
    return m.group(1) if m else ""


def render(html: str) -> str:
    """Shortcut: render HTML and return the output string."""
    return blazeweb.render(html)
