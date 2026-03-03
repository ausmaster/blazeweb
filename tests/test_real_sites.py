"""Real-website gauntlet: blazeweb vs headless Chromium on production sites.

For each site we:
1. Navigate Chromium to the URL (full render with all resources)
2. Capture the raw page source (before JS) via response interception
3. Feed the raw source to blazeweb.render(html, base_url=url)
4. Assert blazeweb doesn't crash/panic
5. Compare DOM similarity between blazeweb output and Chromium output
6. Report a per-site scorecard and final summary table
"""

from __future__ import annotations

import multiprocessing
import os
import re
import sys
import tempfile
import time
from collections import Counter
from dataclasses import dataclass, field

import pytest

lxml_html = pytest.importorskip("lxml.html")

import blazeweb  # noqa: E402

pytestmark = [pytest.mark.conformance, pytest.mark.real_sites]

# ── Site list ────────────────────────────────────────────────────────────────
# 100 production sites across categories. Ordered by category, not difficulty.

SITES = [
    # ── Big Tech ─────────────────────────────────────────────────────────
    "https://www.google.com",
    "https://www.apple.com",
    "https://www.microsoft.com",
    "https://www.amazon.com",
    "https://www.meta.com",
    "https://www.netflix.com",
    "https://www.nvidia.com",
    "https://www.tesla.com",
    "https://www.intel.com",
    "https://www.ibm.com",
    "https://www.oracle.com",
    "https://www.salesforce.com",
    "https://www.adobe.com",
    # ── Social & Community ───────────────────────────────────────────────
    "https://x.com",
    "https://www.reddit.com",
    "https://www.linkedin.com",
    "https://www.pinterest.com",
    "https://www.tumblr.com",
    "https://www.discord.com",
    "https://www.twitch.tv",
    "https://www.tiktok.com",
    # ── Video & Media ────────────────────────────────────────────────────
    "https://www.youtube.com",
    "https://www.spotify.com",
    "https://www.soundcloud.com",
    "https://vimeo.com",
    # ── Developer & Tech ─────────────────────────────────────────────────
    "https://www.github.com",
    "https://www.gitlab.com",
    "https://www.stackoverflow.com",
    "https://news.ycombinator.com",
    "https://dev.to",
    "https://www.rust-lang.org",
    "https://www.python.org",
    "https://nodejs.org",
    "https://go.dev",
    "https://www.typescriptlang.org",
    "https://www.docker.com",
    "https://www.kubernetes.io",
    "https://www.npmjs.com",
    "https://crates.io",
    "https://pypi.org",
    "https://docs.rs",
    # ── News & Media ─────────────────────────────────────────────────────
    "https://www.nytimes.com",
    "https://www.bbc.com",
    "https://www.cnn.com",
    "https://www.reuters.com",
    "https://www.theguardian.com",
    "https://www.washingtonpost.com",
    "https://www.wsj.com",
    "https://www.bloomberg.com",
    "https://techcrunch.com",
    "https://www.theverge.com",
    "https://arstechnica.com",
    "https://www.wired.com",
    "https://www.vice.com",
    "https://www.aljazeera.com",
    "https://www.bbc.co.uk/news",
    "https://apnews.com",
    # ── Reference & Education ────────────────────────────────────────────
    "https://www.wikipedia.org",
    "https://en.wikipedia.org/wiki/Rust_(programming_language)",
    "https://www.britannica.com",
    "https://www.khanacademy.org",
    "https://www.mit.edu",
    "https://www.stanford.edu",
    "https://www.harvard.edu",
    "https://arxiv.org",
    # ── Infrastructure & Cloud ───────────────────────────────────────────
    "https://www.cloudflare.com",
    "https://aws.amazon.com",
    "https://cloud.google.com",
    "https://azure.microsoft.com",
    "https://www.digitalocean.com",
    "https://www.heroku.com",
    "https://vercel.com",
    "https://www.netlify.com",
    "https://www.fastly.com",
    # ── E-Commerce & Business ────────────────────────────────────────────
    "https://www.ebay.com",
    "https://www.etsy.com",
    "https://www.shopify.com",
    "https://www.stripe.com",
    "https://www.paypal.com",
    "https://www.airbnb.com",
    "https://www.booking.com",
    "https://www.uber.com",
    # ── Government & Org ─────────────────────────────────────────────────
    "https://www.usa.gov",
    "https://www.gov.uk",
    "https://www.un.org",
    "https://www.who.int",
    "https://www.nasa.gov",
    "https://www.eff.org",
    # ── Tools & Productivity ─────────────────────────────────────────────
    "https://www.notion.com",
    "https://www.figma.com",
    "https://www.canva.com",
    "https://www.dropbox.com",
    "https://www.zoom.us",
    "https://www.slack.com",
    "https://www.atlassian.com",
    # ── Mozilla & Standards ──────────────────────────────────────────────
    "https://www.mozilla.org",
    "https://developer.mozilla.org",
    "https://www.w3.org",
    "https://whatwg.org",
    # ── Blogs & Content ──────────────────────────────────────────────────
    "https://www.medium.com",
    "https://www.substack.com",
    "https://www.wordpress.com",
    "https://www.blogger.com",
    "https://www.craigslist.org",
]


# ── Scorecard ────────────────────────────────────────────────────────────────


@dataclass
class SiteScore:
    url: str
    # Stability
    crashed: bool = False
    crash_message: str = ""
    skipped: bool = False
    skip_reason: str = ""
    # Parse
    bc_element_count: int = 0
    chrome_element_count: int = 0
    # Sizes
    bc_html_length: int = 0
    chrome_html_length: int = 0
    # DOM similarity
    matching_tags: int = 0
    total_tags: int = 0
    matching_text_nodes: int = 0
    total_text_nodes: int = 0
    matching_ids: int = 0
    total_ids: int = 0
    # Performance
    render_time_ms: float = 0

    @property
    def tag_score(self) -> float:
        return (self.matching_tags / self.total_tags * 100) if self.total_tags else 0

    @property
    def text_score(self) -> float:
        return (self.matching_text_nodes / self.total_text_nodes * 100) if self.total_text_nodes else 0

    @property
    def id_score(self) -> float:
        return (self.matching_ids / self.total_ids * 100) if self.total_ids else 0

    @property
    def composite_score(self) -> float:
        """Weighted composite: 50% tags, 30% text, 20% IDs."""
        return self.tag_score * 0.5 + self.text_score * 0.3 + self.id_score * 0.2


# ── DOM comparison helpers ───────────────────────────────────────────────────


def count_elements(html_string: str) -> int:
    try:
        doc = lxml_html.document_fromstring(html_string)
        return len(doc.xpath("//*"))
    except Exception:
        return 0


def extract_tag_sequence(html_string: str) -> list[str]:
    try:
        doc = lxml_html.document_fromstring(html_string)
        return [el.tag for el in doc.iter() if isinstance(el.tag, str)]
    except Exception:
        return []


def extract_text_fragments(html_string: str) -> set[str]:
    try:
        doc = lxml_html.document_fromstring(html_string)
        for el in doc.xpath("//script | //style"):
            el.getparent().remove(el)
        fragments = set()
        for el in doc.iter():
            if el.text and el.text.strip():
                normalized = " ".join(el.text.split())
                if len(normalized) >= 3:
                    fragments.add(normalized)
            if el.tail and el.tail.strip():
                normalized = " ".join(el.tail.split())
                if len(normalized) >= 3:
                    fragments.add(normalized)
        return fragments
    except Exception:
        return set()


def extract_ids(html_string: str) -> set[str]:
    try:
        doc = lxml_html.document_fromstring(html_string)
        return {el.get("id") for el in doc.xpath("//*[@id]")}
    except Exception:
        return set()


def compute_tag_similarity(seq1: list[str], seq2: list[str]) -> tuple[int, int]:
    if not seq1 or not seq2:
        return 0, max(len(seq1), len(seq2))
    c1 = Counter(seq1)
    c2 = Counter(seq2)
    all_tags = set(c1) | set(c2)
    matching = sum(min(c1.get(t, 0), c2.get(t, 0)) for t in all_tags)
    total = max(len(seq1), len(seq2))
    return matching, total


def score_site(url: str, bc_html: str, chrome_html: str, render_time_ms: float) -> SiteScore:
    score = SiteScore(url=url, render_time_ms=render_time_ms)
    score.bc_html_length = len(bc_html)
    score.chrome_html_length = len(chrome_html)
    score.bc_element_count = count_elements(bc_html)
    score.chrome_element_count = count_elements(chrome_html)

    bc_tags = extract_tag_sequence(bc_html)
    ch_tags = extract_tag_sequence(chrome_html)
    score.matching_tags, score.total_tags = compute_tag_similarity(bc_tags, ch_tags)

    bc_texts = extract_text_fragments(bc_html)
    ch_texts = extract_text_fragments(chrome_html)
    if ch_texts:
        score.matching_text_nodes = len(bc_texts & ch_texts)
        score.total_text_nodes = len(ch_texts)
    else:
        score.total_text_nodes = len(bc_texts)

    bc_ids = extract_ids(bc_html)
    ch_ids = extract_ids(chrome_html)
    if ch_ids:
        score.matching_ids = len(bc_ids & ch_ids)
        score.total_ids = len(ch_ids)
    else:
        score.total_ids = len(bc_ids)

    return score


# ── Subprocess isolation ─────────────────────────────────────────────────────
# V8 debug builds can abort() on certain pages (e.g. ICU assertions).
# Running blazeweb.render() in a forked child ensures a V8 crash doesn't
# kill the entire test runner.


def _render_worker(html: str, base_url: str, out_path: str, err_path: str):
    """Worker function run in a child process."""
    try:
        t0 = time.perf_counter()
        result = blazeweb.render(html, base_url=base_url)
        elapsed = (time.perf_counter() - t0) * 1000
        with open(out_path, "w") as f:
            f.write(result)
        with open(err_path, "w") as f:
            f.write(f"OK|{elapsed}")
    except Exception as e:
        with open(err_path, "w") as f:
            f.write(f"ERR|{type(e).__name__}: {e}")


def render_isolated(html: str, base_url: str, timeout: float = 60.0) -> tuple[str | None, float, str | None]:
    """Run blazeweb.render() in a subprocess for crash isolation.

    Returns (html_output, render_time_ms, error_message).
    If V8 crashes the child, html_output is None and error_message explains.
    """
    with tempfile.NamedTemporaryFile(mode="w", suffix=".html", delete=False) as out_f, \
         tempfile.NamedTemporaryFile(mode="w", suffix=".err", delete=False) as err_f:
        out_path = out_f.name
        err_path = err_f.name

    proc = multiprocessing.Process(
        target=_render_worker,
        args=(html, base_url, out_path, err_path),
    )
    proc.start()
    proc.join(timeout=timeout)

    if proc.is_alive():
        proc.kill()
        proc.join()
        _cleanup(out_path, err_path)
        return None, 0, f"Timed out after {timeout}s"

    if proc.exitcode != 0:
        _cleanup(out_path, err_path)
        return None, 0, f"Process crashed (exit code {proc.exitcode})"

    # Read results
    try:
        with open(err_path) as f:
            status = f.read()
        if status.startswith("OK|"):
            elapsed = float(status.split("|", 1)[1])
            with open(out_path) as f:
                result = f.read()
            _cleanup(out_path, err_path)
            return result, elapsed, None
        elif status.startswith("ERR|"):
            msg = status.split("|", 1)[1]
            _cleanup(out_path, err_path)
            return None, 0, msg
        else:
            _cleanup(out_path, err_path)
            return None, 0, f"Unknown worker status: {status[:200]}"
    except Exception as e:
        _cleanup(out_path, err_path)
        return None, 0, f"Failed to read worker output: {e}"


def _cleanup(*paths):
    for p in paths:
        try:
            os.unlink(p)
        except OSError:
            pass


# ── Fixtures ─────────────────────────────────────────────────────────────────


@pytest.fixture(scope="module")
def gauntlet_browser():
    pw = pytest.importorskip("playwright.sync_api")
    p = pw.sync_playwright().start()
    browser = p.chromium.launch(headless=True)
    yield browser
    browser.close()
    p.stop()


@pytest.fixture
def gauntlet_context(gauntlet_browser):
    """Fresh browser context per test (isolated cookies/storage)."""
    context = gauntlet_browser.new_context(
        user_agent=(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
            "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
        ),
        viewport={"width": 1920, "height": 1080},
        locale="en-US",
    )
    yield context
    context.close()


# ── Gauntlet tests ───────────────────────────────────────────────────────────

_all_scores: list[SiteScore] = []


def _site_id(url: str) -> str:
    return re.sub(r"https?://(www\.)?", "", url).rstrip("/")


@pytest.mark.parametrize("url", SITES, ids=[_site_id(u) for u in SITES])
def test_real_site(url, gauntlet_context):
    """Render a real production site and compare against Chromium.

    Hard assertions:
    - blazeweb must not crash/panic
    - blazeweb must return non-empty HTML with >0 elements

    Soft reporting:
    - DOM similarity scores (tags, text, IDs)
    - Render time, element counts, HTML sizes
    """
    page = gauntlet_context.new_page()
    score = SiteScore(url=url)

    # ── Step 1: Fetch with Chromium ──────────────────────────────────────
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
    except Exception as e:
        score.skipped = True
        score.skip_reason = str(e)[:200]
        _all_scores.append(score)
        page.close()
        pytest.skip(f"Could not load {url}: {str(e)[:100]}")
        return

    chrome_html = page.content()
    page.close()

    source_html = raw_html or chrome_html

    # ── Step 2: Render with blazeweb (subprocess-isolated) ────────────
    bc_html, render_ms, error = render_isolated(source_html, url)

    if bc_html is None:
        score.crashed = True
        score.crash_message = error or "unknown"
        _all_scores.append(score)
        # Don't raise — the crash was contained. Just report it.
        pytest.fail(f"blazeweb crashed on {url}: {error}", pytrace=False)

    # ── Step 3: Hard assertions ──────────────────────────────────────────
    assert isinstance(bc_html, str), f"render() returned {type(bc_html)}"
    assert len(bc_html) > 0, "render() returned empty string"

    # ── Step 4: Score ────────────────────────────────────────────────────
    score = score_site(url, bc_html, chrome_html, render_ms)
    _all_scores.append(score)

    assert score.bc_element_count > 0, f"blazeweb produced 0 elements for {url}"


def test_gauntlet_summary():
    """Print the final summary table of all site scores."""
    if not _all_scores:
        pytest.skip("No site scores collected")

    skipped = [s for s in _all_scores if s.skipped]
    crashed = [s for s in _all_scores if s.crashed]
    passed = [s for s in _all_scores if not s.crashed and not s.skipped]

    hdr = (
        f"\n{'='*96}\n"
        f"  BLAZEWEB GAUNTLET — {len(passed)} passed, "
        f"{len(crashed)} crashed, {len(skipped)} skipped "
        f"(of {len(_all_scores)} sites)\n"
        f"{'='*96}"
    )
    print(hdr, file=sys.stderr)

    if passed:
        avg_tag = sum(s.tag_score for s in passed) / len(passed)
        avg_text = sum(s.text_score for s in passed) / len(passed)
        avg_id = sum(s.id_score for s in passed) / len(passed)
        avg_composite = sum(s.composite_score for s in passed) / len(passed)
        avg_time = sum(s.render_time_ms for s in passed) / len(passed)

        print(f"\n  Average composite score: {avg_composite:.1f}%", file=sys.stderr)
        print(f"  Average tag similarity:  {avg_tag:.1f}%", file=sys.stderr)
        print(f"  Average text overlap:    {avg_text:.1f}%", file=sys.stderr)
        print(f"  Average ID overlap:      {avg_id:.1f}%", file=sys.stderr)
        print(f"  Average render time:     {avg_time:.0f} ms", file=sys.stderr)

        # Tier breakdown
        tiers = {"S (90-100%)": [], "A (70-89%)": [], "B (50-69%)": [], "C (25-49%)": [], "F (<25%)": []}
        for s in passed:
            cs = s.composite_score
            if cs >= 90:
                tiers["S (90-100%)"].append(s)
            elif cs >= 70:
                tiers["A (70-89%)"].append(s)
            elif cs >= 50:
                tiers["B (50-69%)"].append(s)
            elif cs >= 25:
                tiers["C (25-49%)"].append(s)
            else:
                tiers["F (<25%)"].append(s)

        print(f"\n  Tier breakdown:", file=sys.stderr)
        for tier, sites in tiers.items():
            print(f"    {tier}: {len(sites)} sites", file=sys.stderr)

        # Full table
        print(
            f"\n  {'Site':<40} {'Score':>6} {'Tags':>7} {'Text':>7} "
            f"{'IDs':>7} {'Elems':>10} {'Time':>8}",
            file=sys.stderr,
        )
        print(f"  {'-'*40} {'-'*6} {'-'*7} {'-'*7} {'-'*7} {'-'*10} {'-'*8}", file=sys.stderr)
        for s in sorted(passed, key=lambda s: s.composite_score, reverse=True):
            name = _site_id(s.url)
            if len(name) > 39:
                name = name[:36] + "..."
            elems = f"{s.bc_element_count}/{s.chrome_element_count}"
            print(
                f"  {name:<40} {s.composite_score:>5.1f}% {s.tag_score:>6.1f}% "
                f"{s.text_score:>6.1f}% {s.id_score:>6.1f}% {elems:>10} "
                f"{s.render_time_ms:>6.0f}ms",
                file=sys.stderr,
            )

    if crashed:
        print(f"\n  CRASHED ({len(crashed)}):", file=sys.stderr)
        for s in crashed:
            print(f"    {_site_id(s.url)}: {s.crash_message[:100]}", file=sys.stderr)

    if skipped:
        print(f"\n  SKIPPED ({len(skipped)}):", file=sys.stderr)
        for s in skipped:
            print(f"    {_site_id(s.url)}: {s.skip_reason[:100]}", file=sys.stderr)

    print(f"\n{'='*96}", file=sys.stderr)
