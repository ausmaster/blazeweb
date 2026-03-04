"""Real-website gauntlet: blazeweb vs headless Chromium on production sites.

For each site we:
1. Navigate Chromium to the URL (full render with all resources)
2. Capture the raw page source (before JS) via response interception
3. Feed the raw source to blazeweb.render(html, base_url=url)
4. Assert blazeweb doesn't crash/panic
5. Compare DOM similarity between blazeweb output and Chromium output
   using multiple metrics: tag frequency, text fragments, IDs, classes,
   SequenceMatcher HTML similarity, visible text similarity, word-level diff
6. Report a per-site scorecard and final summary table
"""

from __future__ import annotations

import difflib
import re
import sys
import time
from collections import Counter
from dataclasses import dataclass, field

import pytest

lxml_html = pytest.importorskip("lxml.html")
rfuzz = pytest.importorskip("rapidfuzz.fuzz")

import blazeweb  # noqa: E402

pytestmark = [pytest.mark.conformance, pytest.mark.real_sites]

# ── Site list ────────────────────────────────────────────────────────────────
# 100+ production sites across categories. Ordered by category.

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
    # DOM similarity (Counter-based)
    matching_tags: int = 0
    total_tags: int = 0
    # Text fragment overlap (set-based)
    matching_text_nodes: int = 0
    total_text_nodes: int = 0
    # ID overlap (set-based)
    matching_ids: int = 0
    total_ids: int = 0
    # Class overlap (set-based)
    matching_classes: int = 0
    total_classes: int = 0
    # SequenceMatcher: normalized full HTML
    html_similarity: float = 0.0
    # SequenceMatcher: visible text only
    visible_text_similarity: float = 0.0
    visible_text_length_blaze: int = 0
    visible_text_length_chrome: int = 0
    # Word-level diff
    shared_words: int = 0
    blaze_only_words: int = 0
    chrome_only_words: int = 0
    # Tag sequence positional comparison
    tag_sequence_mismatches: int = 0
    total_tag_positions: int = 0
    # JS errors
    bc_js_errors: list[str] = field(default_factory=list)
    chrome_js_errors: list[str] = field(default_factory=list)
    # Verbose diff details (populated for -v output)
    visible_text_diff: list[str] = field(default_factory=list)
    tag_diff_sample: list[str] = field(default_factory=list)
    blaze_only_word_sample: list[str] = field(default_factory=list)
    chrome_only_word_sample: list[str] = field(default_factory=list)
    # Performance
    render_time_ms: float = 0
    chrome_render_ms: float = 0

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
    def class_score(self) -> float:
        return (self.matching_classes / self.total_classes * 100) if self.total_classes else 0

    @property
    def composite_score(self) -> float:
        """Weighted composite:
        25% tag similarity (Counter-based)
        25% html_similarity (SequenceMatcher)
        20% visible_text_similarity (SequenceMatcher)
        15% text fragment overlap (set-based)
        15% ID overlap (set-based)
        """
        return (
            self.tag_score * 0.25
            + self.html_similarity * 100 * 0.25
            + self.visible_text_similarity * 100 * 0.20
            + self.text_score * 0.15
            + self.id_score * 0.15
        )


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
            for text in (el.text, el.tail):
                if text and text.strip():
                    normalized = " ".join(text.split())
                    if len(normalized) >= 3:
                        fragments.add(normalized)
        return fragments
    except Exception:
        return set()


def extract_visible_text(html_string: str) -> str:
    """Extract all visible text content, excluding script/style tags."""
    try:
        doc = lxml_html.document_fromstring(html_string)
        for el in doc.xpath("//script | //style | //noscript"):
            el.getparent().remove(el)
        texts = []
        for el in doc.iter():
            for text in (el.text, el.tail):
                if text:
                    t = text.strip()
                    if t:
                        texts.append(t)
        return " ".join(texts)
    except Exception:
        return ""


def extract_ids(html_string: str) -> set[str]:
    try:
        doc = lxml_html.document_fromstring(html_string)
        return {el.get("id") for el in doc.xpath("//*[@id]")}
    except Exception:
        return set()


def extract_classes(html_string: str) -> set[str]:
    try:
        doc = lxml_html.document_fromstring(html_string)
        classes = set()
        for el in doc.xpath("//*[@class]"):
            for cls in el.get("class", "").split():
                classes.add(cls)
        return classes
    except Exception:
        return set()


def compute_tag_similarity(seq1: list[str], seq2: list[str]) -> tuple[int, int]:
    """Counter-based tag frequency similarity."""
    if not seq1 or not seq2:
        return 0, max(len(seq1), len(seq2))
    c1 = Counter(seq1)
    c2 = Counter(seq2)
    all_tags = set(c1) | set(c2)
    matching = sum(min(c1.get(t, 0), c2.get(t, 0)) for t in all_tags)
    total = max(len(seq1), len(seq2))
    return matching, total


def compute_tag_sequence_mismatches(seq1: list[str], seq2: list[str]) -> tuple[int, int]:
    """Positional tag-by-tag comparison."""
    min_len = min(len(seq1), len(seq2))
    if min_len == 0:
        return 0, 0
    mismatches = sum(1 for i in range(min_len) if seq1[i] != seq2[i])
    return mismatches, min_len


def normalize_html(html: str) -> str:
    """Normalize HTML for SequenceMatcher comparison."""
    # Strip nonce/hash values that differ per request
    html = re.sub(r'nonce="[^"]*"', 'nonce=""', html)
    html = re.sub(r'integrity="[^"]*"', 'integrity=""', html)
    # Normalize whitespace
    html = re.sub(r"\s+", " ", html)
    return html.strip()


def compute_html_similarity(html1: str, html2: str) -> float:
    """rapidfuzz ratio on normalized HTML — O(n) C++ Levenshtein."""
    n1 = normalize_html(html1)
    n2 = normalize_html(html2)
    if not n1 and not n2:
        return 1.0
    if not n1 or not n2:
        return 0.0
    return rfuzz.ratio(n1, n2) / 100.0


def compute_visible_text_similarity(text1: str, text2: str) -> float:
    """rapidfuzz ratio on visible text — O(n) C++ Levenshtein."""
    if not text1 and not text2:
        return 1.0
    if not text1 or not text2:
        return 0.0
    return rfuzz.ratio(text1, text2) / 100.0


def set_overlap(s1: set, s2: set) -> tuple[int, int]:
    """Set overlap count. Returns (matching, total)."""
    if not s2:
        return 0, len(s1)
    return len(s1 & s2), len(s2)


def score_site(
    url: str,
    bc_html: str,
    chrome_html: str,
    render_time_ms: float,
    chrome_render_ms: float,
) -> SiteScore:
    """Compute all comparison metrics between blazeweb and Chrome output."""
    score = SiteScore(
        url=url,
        render_time_ms=render_time_ms,
        chrome_render_ms=chrome_render_ms,
    )
    score.bc_html_length = len(bc_html)
    score.chrome_html_length = len(chrome_html)
    score.bc_element_count = count_elements(bc_html)
    score.chrome_element_count = count_elements(chrome_html)

    # Tag frequency similarity (Counter-based)
    bc_tags = extract_tag_sequence(bc_html)
    ch_tags = extract_tag_sequence(chrome_html)
    score.matching_tags, score.total_tags = compute_tag_similarity(bc_tags, ch_tags)

    # Tag sequence positional comparison
    score.tag_sequence_mismatches, score.total_tag_positions = (
        compute_tag_sequence_mismatches(bc_tags, ch_tags)
    )

    # Text fragment overlap (set-based)
    bc_texts = extract_text_fragments(bc_html)
    ch_texts = extract_text_fragments(chrome_html)
    if ch_texts:
        score.matching_text_nodes = len(bc_texts & ch_texts)
        score.total_text_nodes = len(ch_texts)
    else:
        score.total_text_nodes = len(bc_texts)

    # ID overlap (set-based)
    bc_ids = extract_ids(bc_html)
    ch_ids = extract_ids(chrome_html)
    if ch_ids:
        score.matching_ids = len(bc_ids & ch_ids)
        score.total_ids = len(ch_ids)
    else:
        score.total_ids = len(bc_ids)

    # Class overlap (set-based)
    bc_classes = extract_classes(bc_html)
    ch_classes = extract_classes(chrome_html)
    if ch_classes:
        score.matching_classes = len(bc_classes & ch_classes)
        score.total_classes = len(ch_classes)
    else:
        score.total_classes = len(bc_classes)

    # SequenceMatcher: full HTML similarity
    score.html_similarity = compute_html_similarity(bc_html, chrome_html)

    # Visible text comparison
    bc_visible = extract_visible_text(bc_html)
    ch_visible = extract_visible_text(chrome_html)
    score.visible_text_length_blaze = len(bc_visible)
    score.visible_text_length_chrome = len(ch_visible)
    score.visible_text_similarity = compute_visible_text_similarity(bc_visible, ch_visible)

    # Word-level diff
    bc_words = set(bc_visible.split())
    ch_words = set(ch_visible.split())
    score.shared_words = len(bc_words & ch_words)
    score.blaze_only_words = len(bc_words - ch_words)
    score.chrome_only_words = len(ch_words - bc_words)

    # Verbose diff details (for -v output)
    # Visible text unified diff (first 30 lines)
    if bc_visible != ch_visible:
        bc_lines = bc_visible.split()
        ch_lines = ch_visible.split()
        diff = list(
            difflib.unified_diff(
                ch_lines, bc_lines,
                fromfile="chrome", tofile="blazeweb",
                lineterm="", n=1,
            )
        )
        score.visible_text_diff = diff[:30]

    # Tag sequence mismatch samples (first 10)
    min_len = min(len(bc_tags), len(ch_tags))
    samples = []
    for i in range(min_len):
        if bc_tags[i] != ch_tags[i] and len(samples) < 10:
            samples.append(f"  pos {i}: chrome=<{ch_tags[i]}> blazeweb=<{bc_tags[i]}>")
    if len(bc_tags) != len(ch_tags):
        samples.append(f"  tag count: chrome={len(ch_tags)} blazeweb={len(bc_tags)}")
    score.tag_diff_sample = samples

    # Word samples (first 15 each)
    score.blaze_only_word_sample = sorted(bc_words - ch_words)[:15]
    score.chrome_only_word_sample = sorted(ch_words - bc_words)[:15]

    return score


# ── Render helper ────────────────────────────────────────────────────────────


def _render_worker(html, base_url, out_path, err_path, js_err_path):
    """Worker function for subprocess-isolated render."""
    import time as t
    import blazeweb as bw
    t0 = t.perf_counter()
    try:
        result = bw.render(html, base_url=base_url)
        elapsed = (t.perf_counter() - t0) * 1000
        with open(out_path, "w") as f:
            f.write(str(result))
        with open(err_path, "w") as f:
            f.write(f"OK|{elapsed}")
        with open(js_err_path, "w") as f:
            f.write("\x00".join(result.errors) if result.errors else "")
    except Exception as e:
        with open(err_path, "w") as f:
            f.write(f"ERR|{type(e).__name__}: {e}")


def render_site(
    html: str, base_url: str, timeout: float = 15.0,
) -> tuple[str | None, float, str | None, list[str]]:
    """Run blazeweb.render() in a forked subprocess with timeout.

    Uses fork (not forkserver) for near-zero overhead — the child inherits
    the parent's memory so blazeweb is already imported. The subprocess is
    needed because V8 runs inside py.allow_threads() and Python signals
    cannot interrupt it.

    Returns (html_output, render_time_ms, error_message, js_errors).
    """
    import multiprocessing
    import os
    import tempfile

    out_f = tempfile.NamedTemporaryFile(delete=False, suffix=".html")
    err_f = tempfile.NamedTemporaryFile(delete=False, suffix=".err")
    js_f = tempfile.NamedTemporaryFile(delete=False, suffix=".jserr")
    out_f.close(); err_f.close(); js_f.close()

    ctx = multiprocessing.get_context("fork")
    proc = ctx.Process(
        target=_render_worker,
        args=(html, base_url, out_f.name, err_f.name, js_f.name),
    )
    proc.start()
    proc.join(timeout=timeout)

    if proc.is_alive():
        proc.kill()
        proc.join()
        _cleanup(out_f.name, err_f.name, js_f.name)
        return None, 0, f"Timed out after {timeout:.0f}s", []

    if proc.exitcode != 0:
        _cleanup(out_f.name, err_f.name, js_f.name)
        return None, 0, f"Process crashed (exit code {proc.exitcode})", []

    try:
        with open(err_f.name) as f:
            status = f.read()
        if status.startswith("OK|"):
            elapsed = float(status.split("|", 1)[1])
            with open(out_f.name) as f:
                result_html = f.read()
            js_errors = []
            try:
                with open(js_f.name) as f:
                    content = f.read()
                if content:
                    js_errors = content.split("\x00")
            except Exception:
                pass
            _cleanup(out_f.name, err_f.name, js_f.name)
            return result_html, elapsed, None, js_errors
        elif status.startswith("ERR|"):
            msg = status.split("|", 1)[1]
            _cleanup(out_f.name, err_f.name, js_f.name)
            return None, 0, msg, []
        else:
            _cleanup(out_f.name, err_f.name, js_f.name)
            return None, 0, f"Unknown status: {status[:200]}", []
    except Exception as e:
        _cleanup(out_f.name, err_f.name, js_f.name)
        return None, 0, f"Failed to read output: {e}", []


def _cleanup(*paths):
    import os
    for p in paths:
        try:
            os.unlink(p)
        except OSError:
            pass


# ── Gauntlet tests ───────────────────────────────────────────────────────────

_all_scores: list[SiteScore] = []

_USER_AGENT = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
)


def _site_id(url: str) -> str:
    return re.sub(r"https?://(www\.)?", "", url).rstrip("/")


@pytest.mark.parametrize("url", SITES, ids=[_site_id(u) for u in SITES])
def test_real_site(url, browser):
    """Render a real production site and compare against Chromium.

    Hard assertions:
    - blazeweb must not crash/panic
    - blazeweb must return non-empty HTML with >0 elements

    Soft reporting:
    - DOM similarity scores (tags, text, IDs, classes)
    - SequenceMatcher HTML/text similarity
    - Word-level diff
    - Render time, element counts, HTML sizes
    """
    # Fresh context per test (isolated cookies/storage)
    context = browser.new_context(
        user_agent=_USER_AGENT,
        viewport={"width": 1920, "height": 1080},
        locale="en-US",
    )
    page = context.new_page()
    score = SiteScore(url=url)

    # ── Step 1: Fetch with Chromium ──────────────────────────────────────
    raw_html = None
    chrome_js_errors: list[str] = []

    def handle_response(response):
        nonlocal raw_html
        if raw_html is None and response.request.resource_type == "document":
            try:
                raw_html = response.text()
            except Exception:
                pass

    def handle_page_error(error):
        chrome_js_errors.append(str(error))

    def handle_console(msg):
        if msg.type == "error":
            chrome_js_errors.append(msg.text)

    page.on("response", handle_response)
    page.on("pageerror", handle_page_error)
    page.on("console", handle_console)

    chrome_t0 = time.perf_counter()
    try:
        page.goto(url, wait_until="domcontentloaded", timeout=20000)
    except Exception as e:
        score.skipped = True
        score.skip_reason = str(e)[:200]
        _all_scores.append(score)
        page.close()
        context.close()
        pytest.skip(f"Could not load {url}: {str(e)[:100]}")
        return

    chrome_html = page.content()
    chrome_ms = (time.perf_counter() - chrome_t0) * 1000
    page.close()
    context.close()

    source_html = raw_html or chrome_html

    # ── Step 2: Render with blazeweb (subprocess-isolated) ────────────
    bc_html, render_ms, error, bc_js_errors = render_site(source_html, url)

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
    score = score_site(url, bc_html, chrome_html, render_ms, chrome_ms)
    score.bc_js_errors = bc_js_errors
    score.chrome_js_errors = chrome_js_errors
    _all_scores.append(score)

    assert score.bc_element_count > 0, f"blazeweb produced 0 elements for {url}"


def _is_verbose() -> bool:
    """Check if pytest was invoked with -v or higher verbosity."""
    return "-v" in sys.argv or "-vv" in sys.argv or "-vvv" in sys.argv or "--verbose" in sys.argv


def test_gauntlet_summary():
    """Print the final summary table of all site scores."""
    if not _all_scores:
        pytest.skip("No site scores collected")

    skipped = [s for s in _all_scores if s.skipped]
    crashed = [s for s in _all_scores if s.crashed]
    passed = [s for s in _all_scores if not s.crashed and not s.skipped]

    hdr = (
        f"\n{'=' * 120}\n"
        f"  BLAZEWEB vs CHROMIUM — Full Comparison ({len(passed)} passed, "
        f"{len(crashed)} crashed, {len(skipped)} skipped "
        f"of {len(_all_scores)} sites)\n"
        f"{'=' * 120}"
    )
    print(hdr, file=sys.stderr)

    if passed:
        avg_composite = sum(s.composite_score for s in passed) / len(passed)
        avg_tag = sum(s.tag_score for s in passed) / len(passed)
        avg_html = sum(s.html_similarity * 100 for s in passed) / len(passed)
        avg_vtext = sum(s.visible_text_similarity * 100 for s in passed) / len(passed)
        avg_text = sum(s.text_score for s in passed) / len(passed)
        avg_id = sum(s.id_score for s in passed) / len(passed)
        avg_class = sum(s.class_score for s in passed) / len(passed)
        avg_blaze_ms = sum(s.render_time_ms for s in passed) / len(passed)
        avg_chrome_ms = sum(s.chrome_render_ms for s in passed) / len(passed)
        total_shared = sum(s.shared_words for s in passed)
        total_blaze_only = sum(s.blaze_only_words for s in passed)
        total_chrome_only = sum(s.chrome_only_words for s in passed)

        print(f"\n  Overall Scores:", file=sys.stderr)
        print(f"    Composite:             {avg_composite:>6.1f}%", file=sys.stderr)
        print(f"    Tag similarity:        {avg_tag:>6.1f}%", file=sys.stderr)
        print(f"    HTML similarity:       {avg_html:>6.1f}%  (SequenceMatcher)", file=sys.stderr)
        print(f"    Visible text sim:      {avg_vtext:>6.1f}%  (SequenceMatcher)", file=sys.stderr)
        print(f"    Text fragment overlap: {avg_text:>6.1f}%", file=sys.stderr)
        print(f"    ID overlap:            {avg_id:>6.1f}%", file=sys.stderr)
        print(f"    Class overlap:         {avg_class:>6.1f}%", file=sys.stderr)
        print(f"    Avg blazeweb time:     {avg_blaze_ms:>6.0f} ms", file=sys.stderr)
        print(f"    Avg Chrome time:       {avg_chrome_ms:>6.0f} ms", file=sys.stderr)
        if avg_blaze_ms > 0:
            print(f"    Avg speedup:           {avg_chrome_ms / avg_blaze_ms:>5.1f}x", file=sys.stderr)
        print(
            f"    Words: {total_shared} shared, "
            f"{total_blaze_only} blazeweb-only, "
            f"{total_chrome_only} chrome-only",
            file=sys.stderr,
        )

        # Tier breakdown
        tiers = {
            "S (90-100%)": [],
            "A (70-89%)": [],
            "B (50-69%)": [],
            "C (25-49%)": [],
            "F (<25%)": [],
        }
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
            f"\n  {'Site':<35} {'Score':>6} {'Tags':>6} {'HTML':>6} "
            f"{'VText':>6} {'Text':>6} {'IDs':>6} {'Class':>6} "
            f"{'Elems':>11} {'Blaze':>7} {'Chrome':>7} {'Speed':>6}",
            file=sys.stderr,
        )
        print(
            f"  {'-' * 35} {'-' * 6} {'-' * 6} {'-' * 6} "
            f"{'-' * 6} {'-' * 6} {'-' * 6} {'-' * 6} "
            f"{'-' * 11} {'-' * 7} {'-' * 7} {'-' * 6}",
            file=sys.stderr,
        )

        for s in sorted(passed, key=lambda x: x.composite_score, reverse=True):
            name = _site_id(s.url)
            if len(name) > 34:
                name = name[:31] + "..."
            elems = f"{s.bc_element_count}/{s.chrome_element_count}"
            speedup = s.chrome_render_ms / s.render_time_ms if s.render_time_ms > 0 else 0
            print(
                f"  {name:<35} {s.composite_score:>5.1f}% "
                f"{s.tag_score:>5.1f}% {s.html_similarity * 100:>5.1f}% "
                f"{s.visible_text_similarity * 100:>5.1f}% {s.text_score:>5.1f}% "
                f"{s.id_score:>5.1f}% {s.class_score:>5.1f}% "
                f"{elems:>11} {s.render_time_ms:>5.0f}ms "
                f"{s.chrome_render_ms:>5.0f}ms {speedup:>5.1f}x",
                file=sys.stderr,
            )

        # Word diff details for sites with notable differences
        notable = [s for s in passed if s.blaze_only_words > 10 or s.chrome_only_words > 10]
        if notable:
            print(f"\n  Word-level differences (sites with >10 unique words):", file=sys.stderr)
            for s in sorted(notable, key=lambda x: x.chrome_only_words, reverse=True):
                name = _site_id(s.url)
                if len(name) > 34:
                    name = name[:31] + "..."
                print(
                    f"    {name:<35} shared={s.shared_words:>5} "
                    f"blaze-only={s.blaze_only_words:>5} "
                    f"chrome-only={s.chrome_only_words:>5}",
                    file=sys.stderr,
                )

        # JS error comparison
        total_bc_errors = sum(len(s.bc_js_errors) for s in passed)
        total_ch_errors = sum(len(s.chrome_js_errors) for s in passed)
        fewer = sum(1 for s in passed if len(s.bc_js_errors) < len(s.chrome_js_errors))
        equal = sum(1 for s in passed if len(s.bc_js_errors) == len(s.chrome_js_errors))
        more = sum(1 for s in passed if len(s.bc_js_errors) > len(s.chrome_js_errors))

        print(f"\n  JS Errors:", file=sys.stderr)
        print(f"    Total blazeweb errors: {total_bc_errors}", file=sys.stderr)
        print(f"    Total Chrome errors:   {total_ch_errors}", file=sys.stderr)
        print(
            f"    Sites: {fewer} fewer errors, {equal} equal, {more} more errors",
            file=sys.stderr,
        )

        # Show per-site JS error details (sites with errors from either side)
        sites_with_errors = [
            s for s in passed if s.bc_js_errors or s.chrome_js_errors
        ]
        if sites_with_errors:
            print(
                f"\n  {'Site':<35} {'Blaze':>6} {'Chrome':>7} {'Delta':>6}",
                file=sys.stderr,
            )
            print(
                f"  {'-' * 35} {'-' * 6} {'-' * 7} {'-' * 6}",
                file=sys.stderr,
            )
            for s in sorted(
                sites_with_errors,
                key=lambda x: len(x.bc_js_errors) - len(x.chrome_js_errors),
                reverse=True,
            ):
                name = _site_id(s.url)
                if len(name) > 34:
                    name = name[:31] + "..."
                bc_n = len(s.bc_js_errors)
                ch_n = len(s.chrome_js_errors)
                delta = bc_n - ch_n
                marker = "  <+" if delta < 0 else ("  !!" if delta > 0 else "")
                print(
                    f"  {name:<35} {bc_n:>6} {ch_n:>7} {delta:>+6}{marker}",
                    file=sys.stderr,
                )

    # ── Verbose per-site details (only with -v) ────────────────────────
    if passed and _is_verbose():
        # Collect sites that have something interesting to show
        detailed = [
            s for s in passed
            if s.bc_js_errors or s.chrome_js_errors
            or s.visible_text_diff or s.tag_diff_sample
        ]
        if detailed:
            print(
                f"\n{'─' * 120}\n"
                f"  VERBOSE: Per-site JS errors & HTML discrepancies "
                f"({len(detailed)} sites with differences)\n"
                f"{'─' * 120}",
                file=sys.stderr,
            )
            for s in sorted(detailed, key=lambda x: x.composite_score):
                name = _site_id(s.url)
                print(f"\n  ▸ {name}  (composite: {s.composite_score:.1f}%)", file=sys.stderr)

                # JS errors — blazeweb
                if s.bc_js_errors:
                    print(f"    Blazeweb JS errors ({len(s.bc_js_errors)}):", file=sys.stderr)
                    for err in s.bc_js_errors[:10]:
                        trunc = err[:200].replace("\n", " ")
                        print(f"      - {trunc}", file=sys.stderr)
                    if len(s.bc_js_errors) > 10:
                        print(f"      ... and {len(s.bc_js_errors) - 10} more", file=sys.stderr)

                # JS errors — chrome
                if s.chrome_js_errors:
                    print(f"    Chrome JS errors ({len(s.chrome_js_errors)}):", file=sys.stderr)
                    for err in s.chrome_js_errors[:10]:
                        trunc = err[:200].replace("\n", " ")
                        print(f"      - {trunc}", file=sys.stderr)
                    if len(s.chrome_js_errors) > 10:
                        print(f"      ... and {len(s.chrome_js_errors) - 10} more", file=sys.stderr)

                # Tag sequence mismatches
                if s.tag_diff_sample:
                    print(f"    Tag mismatches ({s.tag_sequence_mismatches}/{s.total_tag_positions}):", file=sys.stderr)
                    for line in s.tag_diff_sample:
                        print(f"      {line}", file=sys.stderr)

                # Visible text diff
                if s.visible_text_diff:
                    print(f"    Visible text diff (first 30 lines):", file=sys.stderr)
                    for line in s.visible_text_diff:
                        print(f"      {line}", file=sys.stderr)

                # Word samples
                if s.blaze_only_word_sample:
                    words = ", ".join(s.blaze_only_word_sample)
                    print(f"    Blazeweb-only words ({s.blaze_only_words} total): {words}", file=sys.stderr)
                if s.chrome_only_word_sample:
                    words = ", ".join(s.chrome_only_word_sample)
                    print(f"    Chrome-only words ({s.chrome_only_words} total): {words}", file=sys.stderr)

    if crashed:
        print(f"\n  CRASHED ({len(crashed)}):", file=sys.stderr)
        for s in crashed:
            print(f"    {_site_id(s.url)}: {s.crash_message[:100]}", file=sys.stderr)

    if skipped:
        print(f"\n  SKIPPED ({len(skipped)}):", file=sys.stderr)
        for s in skipped:
            print(f"    {_site_id(s.url)}: {s.skip_reason[:100]}", file=sys.stderr)

    print(f"\n{'=' * 120}", file=sys.stderr)
