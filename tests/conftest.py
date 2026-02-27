"""Shared test fixtures."""

from __future__ import annotations

import pytest


@pytest.fixture(scope="session")
def browser():
    """Session-scoped Playwright Chromium browser.

    Skips all conformance tests if playwright is not installed.
    """
    pw = pytest.importorskip("playwright.sync_api")
    p = pw.sync_playwright().start()
    browser = p.chromium.launch(headless=True)
    yield browser
    browser.close()
    p.stop()


@pytest.fixture
def page(browser):
    """Function-scoped Playwright page (fresh per test)."""
    page = browser.new_page()
    yield page
    page.close()
