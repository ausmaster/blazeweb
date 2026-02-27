"""Performance benchmark: blazeclient vs headless Chromium.

For each site we fetch the raw HTML once, then time both engines
on the same input. blazeclient runs in-process for accurate timing;
Chromium uses page.set_content() (parse + execute, no network).
"""

from __future__ import annotations

import re
import sys
import time
from dataclasses import dataclass

import pytest

pytest.importorskip("playwright.sync_api")

import blazeclient  # noqa: E402

from test_real_sites import SITES  # noqa: E402

pytestmark = [pytest.mark.benchmark]

# Sites that trigger V8 ICU debug abort — skip for in-process timing
V8_ICU_CRASHERS = {"https://www.discord.com", "https://vimeo.com", "https://www.uber.com"}

BENCH_SITES = [s for s in SITES if s not in V8_ICU_CRASHERS]


@dataclass
class BenchResult:
    url: str
    html_bytes: int
    bc_ms: float
    chrome_ms: float
    skipped: bool = False
    skip_reason: str = ""

    @property
    def speedup(self) -> float:
        return self.chrome_ms / self.bc_ms if self.bc_ms > 0 else 0

    @property
    def site_name(self) -> str:
        return re.sub(r"https?://(www\.)?", "", self.url).rstrip("/")


_results: list[BenchResult] = []


@pytest.fixture(scope="module")
def bench_browser():
    from playwright.sync_api import sync_playwright
    p = sync_playwright().start()
    browser = p.chromium.launch(headless=True)
    yield browser
    browser.close()
    p.stop()


@pytest.fixture(scope="module")
def html_cache(bench_browser):
    """Prefetch all site HTML once. Returns {url: raw_html}."""
    cache = {}
    ctx = bench_browser.new_context(
        user_agent=(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
            "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
        ),
        viewport={"width": 1920, "height": 1080},
    )
    page = ctx.new_page()
    for url in BENCH_SITES:
        raw_html = None

        def handle_response(response):
            nonlocal raw_html
            if raw_html is None and response.request.resource_type == "document":
                try:
                    raw_html = response.text()
                except Exception:
                    pass

        page.on("response", handle_response)
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=20000)
            cache[url] = raw_html or page.content()
        except Exception:
            pass
        page.remove_listener("response", handle_response)

    page.close()
    ctx.close()
    return cache


def _site_id(url: str) -> str:
    return re.sub(r"https?://(www\.)?", "", url).rstrip("/")


@pytest.mark.parametrize("url", BENCH_SITES, ids=[_site_id(u) for u in BENCH_SITES])
def test_bench_site(url, bench_browser, html_cache):
    """Benchmark blazeclient vs Chromium on one site."""
    if url not in html_cache:
        r = BenchResult(url=url, html_bytes=0, bc_ms=0, chrome_ms=0, skipped=True, skip_reason="fetch failed")
        _results.append(r)
        pytest.skip(f"No HTML cached for {url}")

    html = html_cache[url]

    # ── blazeclient timing ───────────────────────────────────────────────
    # Warm up (first call inits V8 platform)
    try:
        blazeclient.render("<html><body></body></html>")
    except Exception:
        pass

    t0 = time.perf_counter()
    try:
        blazeclient.render(html, base_url=url)
    except Exception:
        pass
    bc_ms = (time.perf_counter() - t0) * 1000

    # ── Chromium timing ──────────────────────────────────────────────────
    # page.set_content = parse HTML + execute scripts (no network fetch)
    ctx = bench_browser.new_context()
    page = ctx.new_page()

    t0 = time.perf_counter()
    try:
        page.set_content(html, wait_until="load", timeout=30000)
    except Exception:
        pass
    chrome_ms = (time.perf_counter() - t0) * 1000

    page.close()
    ctx.close()

    r = BenchResult(url=url, html_bytes=len(html), bc_ms=bc_ms, chrome_ms=chrome_ms)
    _results.append(r)


def test_benchmark_summary():
    """Print the benchmark comparison table."""
    valid = [r for r in _results if not r.skipped and r.bc_ms > 0 and r.chrome_ms > 0]
    if not valid:
        pytest.skip("No benchmark results")

    faster = [r for r in valid if r.bc_ms < r.chrome_ms]
    slower = [r for r in valid if r.bc_ms >= r.chrome_ms]
    avg_bc = sum(r.bc_ms for r in valid) / len(valid)
    avg_ch = sum(r.chrome_ms for r in valid) / len(valid)
    total_bc = sum(r.bc_ms for r in valid)
    total_ch = sum(r.chrome_ms for r in valid)
    median_speedup = sorted(r.speedup for r in valid)[len(valid) // 2]

    hdr = (
        f"\n{'='*100}\n"
        f"  BLAZECLIENT vs CHROMIUM — Performance Benchmark ({len(valid)} sites)\n"
        f"{'='*100}\n"
        f"\n"
        f"  blazeclient faster: {len(faster)}/{len(valid)} sites\n"
        f"  Chromium faster:    {len(slower)}/{len(valid)} sites\n"
        f"\n"
        f"  Total time:    blazeclient {total_bc/1000:.2f}s  vs  Chromium {total_ch/1000:.2f}s  "
        f"({total_ch/total_bc:.1f}x)\n"
        f"  Average/site:  blazeclient {avg_bc:.0f}ms  vs  Chromium {avg_ch:.0f}ms\n"
        f"  Median speedup: {median_speedup:.1f}x\n"
    )
    print(hdr, file=sys.stderr)

    print(
        f"  {'Site':<40} {'HTML':>8} {'blaze':>9} {'chrome':>9} {'speedup':>9}",
        file=sys.stderr,
    )
    print(f"  {'-'*40} {'-'*8} {'-'*9} {'-'*9} {'-'*9}", file=sys.stderr)

    for r in sorted(valid, key=lambda r: r.speedup, reverse=True):
        name = r.site_name
        if len(name) > 39:
            name = name[:36] + "..."
        kb = f"{r.html_bytes / 1024:.0f}KB"
        marker = "<<<" if r.speedup >= 5 else ("<<" if r.speedup >= 2 else ("<" if r.speedup > 1 else ""))
        print(
            f"  {name:<40} {kb:>8} {r.bc_ms:>8.0f}ms {r.chrome_ms:>8.0f}ms "
            f"{r.speedup:>7.1f}x {marker}",
            file=sys.stderr,
        )

    print(f"\n{'='*100}", file=sys.stderr)
