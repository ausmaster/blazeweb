"""Performance benchmark: blazeweb Client (with cache) vs headless Chromium.

For each site we fetch the raw HTML once, then time both engines on the same
input.  blazeweb uses a Client with script cache — a cold pass populates
the cache, then a warm pass measures cached performance.  Chromium uses
page.set_content() (parse + execute, no external script fetch).
"""

from __future__ import annotations

import re
import sys
import time
from dataclasses import dataclass, field

import pytest

pytest.importorskip("playwright.sync_api")

import blazeweb  # noqa: E402

from test_real_sites import SITES  # noqa: E402

pytestmark = [pytest.mark.benchmark]

# Sites that trigger V8 ICU debug abort — skip for in-process timing
V8_ICU_CRASHERS = {"https://www.discord.com", "https://vimeo.com", "https://www.uber.com"}

BENCH_SITES = [s for s in SITES if s not in V8_ICU_CRASHERS]


@dataclass
class BenchResult:
    url: str
    html_bytes: int
    bc_cold_ms: float
    bc_warm_ms: float
    chrome_ms: float
    skipped: bool = False
    skip_reason: str = ""

    @property
    def speedup_warm(self) -> float:
        return self.chrome_ms / self.bc_warm_ms if self.bc_warm_ms > 0 else 0

    @property
    def speedup_cold(self) -> float:
        return self.chrome_ms / self.bc_cold_ms if self.bc_cold_ms > 0 else 0

    @property
    def site_name(self) -> str:
        return re.sub(r"https?://(www\.)?", "", self.url).rstrip("/")


_results: list[BenchResult] = []

# Module-scoped Client — cache accumulates across all sites
_client: blazeweb.Client | None = None


@pytest.fixture(scope="module")
def bench_browser():
    from playwright.sync_api import sync_playwright
    p = sync_playwright().start()
    browser = p.chromium.launch(headless=True)
    yield browser
    browser.close()
    p.stop()


@pytest.fixture(scope="module")
def bc_client():
    """Module-scoped blazeweb Client with warm cache."""
    global _client
    _client = blazeweb.Client()
    # Warm up V8 platform
    _client.render("<html><body></body></html>")
    return _client


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

        def handle_response(response, _rh=[None]):
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
def test_bench_site(url, bench_browser, html_cache, bc_client):
    """Benchmark blazeweb Client (cold + warm) vs Chromium on one site."""
    if url not in html_cache:
        r = BenchResult(url=url, html_bytes=0, bc_cold_ms=0, bc_warm_ms=0,
                        chrome_ms=0, skipped=True, skip_reason="fetch failed")
        _results.append(r)
        pytest.skip(f"No HTML cached for {url}")

    html = html_cache[url]

    # ── blazeweb COLD: first render, fetches + caches external scripts ──
    t0 = time.perf_counter()
    try:
        bc_client.render(html, base_url=url)
    except Exception:
        pass
    bc_cold_ms = (time.perf_counter() - t0) * 1000

    # ── blazeweb WARM: second render, cache hits ────────────────────────
    t0 = time.perf_counter()
    try:
        bc_client.render(html, base_url=url)
    except Exception:
        pass
    bc_warm_ms = (time.perf_counter() - t0) * 1000

    # ── Chromium timing ────────────────────────────────────────────────────
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

    r = BenchResult(url=url, html_bytes=len(html), bc_cold_ms=bc_cold_ms,
                    bc_warm_ms=bc_warm_ms, chrome_ms=chrome_ms)
    _results.append(r)


def test_benchmark_summary():
    """Print the benchmark comparison table."""
    valid = [r for r in _results if not r.skipped and r.bc_warm_ms > 0 and r.chrome_ms > 0]
    if not valid:
        pytest.skip("No benchmark results")

    warm_faster = [r for r in valid if r.bc_warm_ms < r.chrome_ms]
    cold_faster = [r for r in valid if r.bc_cold_ms < r.chrome_ms]
    total_cold = sum(r.bc_cold_ms for r in valid)
    total_warm = sum(r.bc_warm_ms for r in valid)
    total_ch = sum(r.chrome_ms for r in valid)
    median_warm = sorted(r.speedup_warm for r in valid)[len(valid) // 2]

    hdr = (
        f"\n{'='*115}\n"
        f"  BLAZEWEB CLIENT vs CHROMIUM — Performance Benchmark ({len(valid)} sites)\n"
        f"{'='*115}\n"
        f"\n"
        f"  blazeweb (warm cache) faster: {len(warm_faster)}/{len(valid)} sites\n"
        f"  blazeweb (cold, no cache) faster: {len(cold_faster)}/{len(valid)} sites\n"
        f"\n"
        f"  Total time:  cold {total_cold/1000:.2f}s  |  warm {total_warm/1000:.2f}s  |  "
        f"Chromium {total_ch/1000:.2f}s\n"
        f"  Warm vs Chromium: {total_ch/total_warm:.1f}x overall  |  "
        f"median {median_warm:.1f}x per-site\n"
        f"\n"
        f"  Cache entries: {_client.cache_size if _client else 0}\n"
    )
    print(hdr, file=sys.stderr)

    print(
        f"  {'Site':<40} {'HTML':>6} {'cold':>9} {'warm':>9} {'chrome':>9} {'warm/chr':>9}",
        file=sys.stderr,
    )
    print(f"  {'-'*40} {'-'*6} {'-'*9} {'-'*9} {'-'*9} {'-'*9}", file=sys.stderr)

    for r in sorted(valid, key=lambda r: r.speedup_warm, reverse=True):
        name = r.site_name
        if len(name) > 39:
            name = name[:36] + "..."
        kb = f"{r.html_bytes / 1024:.0f}KB"
        marker = "<<<" if r.speedup_warm >= 5 else ("<<" if r.speedup_warm >= 2 else ("<" if r.speedup_warm > 1 else ""))
        print(
            f"  {name:<40} {kb:>6} {r.bc_cold_ms:>7.0f}ms {r.bc_warm_ms:>7.0f}ms "
            f"{r.chrome_ms:>7.0f}ms {r.speedup_warm:>7.1f}x {marker}",
            file=sys.stderr,
        )

    print(f"\n{'='*115}", file=sys.stderr)
