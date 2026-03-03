"""Side-by-side comparison: blazeweb vs headless Chromium on real websites.

Fetches the raw HTML from a site, renders it through both blazeweb and
Chromium, then produces a detailed structural comparison report.
"""

from __future__ import annotations

import re
import sys
import time
from collections import Counter

import pytest

lxml_html = pytest.importorskip("lxml.html")

import blazeweb  # noqa: E402

pytestmark = pytest.mark.conformance

# Sites chosen to exercise different patterns:
# - static HTML (wikipedia, craigslist)
# - server-rendered with inline JS (HN, python.org)
# - JS-heavy SPAs (crates.io, npmjs)
# - news sites with complex DOM (reuters, apnews)
COMPARISON_SITES = [
    "https://news.ycombinator.com",
    "https://www.python.org",
    "https://www.craigslist.org",
    "https://www.w3.org",
    "https://whatwg.org",
    "https://arxiv.org",
    "https://www.eff.org",
    "https://www.wikipedia.org",
    "https://en.wikipedia.org/wiki/Rust_(programming_language)",
    "https://apnews.com",
    "https://www.reuters.com",
    "https://pypi.org",
    "https://www.usa.gov",
]


def _site_id(url: str) -> str:
    return re.sub(r"https?://(www\.)?", "", url).rstrip("/")


# ── DOM analysis helpers ─────────────────────────────────────────────────────


def count_elements(html_string: str) -> int:
    try:
        doc = lxml_html.document_fromstring(html_string)
        return len(doc.xpath("//*"))
    except Exception:
        return 0


def extract_tag_bag(html_string: str) -> Counter:
    try:
        doc = lxml_html.document_fromstring(html_string)
        return Counter(el.tag for el in doc.iter() if isinstance(el.tag, str))
    except Exception:
        return Counter()


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


def tag_similarity(c1: Counter, c2: Counter) -> float:
    if not c1 and not c2:
        return 100.0
    all_tags = set(c1) | set(c2)
    matching = sum(min(c1.get(t, 0), c2.get(t, 0)) for t in all_tags)
    total = max(sum(c1.values()), sum(c2.values()))
    return (matching / total * 100) if total else 0


def set_overlap(s1: set, s2: set) -> float:
    if not s2:
        return 100.0 if not s1 else 0.0
    return len(s1 & s2) / len(s2) * 100


# ── Fixtures ─────────────────────────────────────────────────────────────────


# ── Test ─────────────────────────────────────────────────────────────────────

_results: list[dict] = []


@pytest.mark.parametrize("url", COMPARISON_SITES, ids=[_site_id(u) for u in COMPARISON_SITES])
def test_chrome_comparison(url, browser):
    """Fetch raw HTML, render through blazeweb and Chrome, compare DOMs."""
    context = browser.new_context(
        user_agent=(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
            "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
        ),
    )
    page = context.new_page()

    # Intercept the document response to capture raw HTML
    raw_html = None

    def handle_response(response):
        nonlocal raw_html
        if raw_html is None and response.request.resource_type == "document":
            try:
                raw_html = response.text()
            except Exception:
                pass

    page.on("response", handle_response)

    chrome_t0 = time.perf_counter()
    try:
        page.goto(url, wait_until="domcontentloaded", timeout=15000)
    except Exception as e:
        page.close()
        context.close()
        pytest.skip(f"Could not load {url}: {str(e)[:100]}")
        return

    # Chrome's final rendered HTML
    chrome_html = page.content()
    chrome_ms = (time.perf_counter() - chrome_t0) * 1000
    page.close()
    context.close()

    source_html = raw_html or chrome_html

    # blazeweb render
    t0 = time.perf_counter()
    try:
        bc_html = blazeweb.render(source_html, base_url=url)
    except Exception as e:
        _results.append({
            "url": url,
            "error": str(e)[:200],
        })
        pytest.fail(f"blazeweb.render() failed on {url}: {e}")
        return
    render_ms = (time.perf_counter() - t0) * 1000

    # Compute similarity metrics
    bc_tags = extract_tag_bag(bc_html)
    ch_tags = extract_tag_bag(chrome_html)
    tag_sim = tag_similarity(bc_tags, ch_tags)

    bc_text = extract_text_fragments(bc_html)
    ch_text = extract_text_fragments(chrome_html)
    text_sim = set_overlap(bc_text, ch_text)

    bc_ids = extract_ids(bc_html)
    ch_ids = extract_ids(chrome_html)
    id_sim = set_overlap(bc_ids, ch_ids)

    bc_classes = extract_classes(bc_html)
    ch_classes = extract_classes(chrome_html)
    class_sim = set_overlap(bc_classes, ch_classes)

    composite = tag_sim * 0.35 + text_sim * 0.30 + id_sim * 0.20 + class_sim * 0.15

    result = {
        "url": url,
        "tag_sim": tag_sim,
        "text_sim": text_sim,
        "id_sim": id_sim,
        "class_sim": class_sim,
        "composite": composite,
        "bc_elements": sum(bc_tags.values()),
        "ch_elements": sum(ch_tags.values()),
        "bc_text_frags": len(bc_text),
        "ch_text_frags": len(ch_text),
        "render_ms": render_ms,
        "chrome_ms": chrome_ms,
    }
    _results.append(result)

    # The test passes as long as blazeweb doesn't crash and produces output
    assert sum(bc_tags.values()) > 0, f"blazeweb produced empty DOM for {url}"


def test_comparison_summary():
    """Print the final comparison report."""
    if not _results:
        pytest.skip("No results collected")

    valid = [r for r in _results if "error" not in r]
    errors = [r for r in _results if "error" in r]

    print(
        f"\n{'='*100}\n"
        f"  BLAZEWEB vs CHROMIUM — DOM Comparison ({len(valid)} sites)\n"
        f"{'='*100}",
        file=sys.stderr,
    )

    if valid:
        avg_tag = sum(r["tag_sim"] for r in valid) / len(valid)
        avg_text = sum(r["text_sim"] for r in valid) / len(valid)
        avg_id = sum(r["id_sim"] for r in valid) / len(valid)
        avg_class = sum(r["class_sim"] for r in valid) / len(valid)
        avg_composite = sum(r["composite"] for r in valid) / len(valid)
        avg_time = sum(r["render_ms"] for r in valid) / len(valid)

        print(f"\n  Overall Scores:", file=sys.stderr)
        print(f"    Composite:       {avg_composite:>6.1f}%", file=sys.stderr)
        print(f"    Tag similarity:  {avg_tag:>6.1f}%", file=sys.stderr)
        print(f"    Text overlap:    {avg_text:>6.1f}%", file=sys.stderr)
        print(f"    ID overlap:      {avg_id:>6.1f}%", file=sys.stderr)
        print(f"    Class overlap:   {avg_class:>6.1f}%", file=sys.stderr)
        avg_chrome = sum(r["chrome_ms"] for r in valid) / len(valid)
        print(f"    Avg render time:   {avg_time:>6.0f} ms (blazeweb)", file=sys.stderr)
        print(f"    Avg render time:   {avg_chrome:>6.0f} ms (chrome)", file=sys.stderr)
        if avg_chrome > 0:
            print(f"    Avg speedup:       {avg_chrome / avg_time:>5.1f}x", file=sys.stderr)

        print(
            f"\n  {'Site':<50} {'Score':>6} {'Tags':>6} {'Text':>6} "
            f"{'IDs':>6} {'Class':>6} {'Elems':>12} {'Blaze':>8} {'Chrome':>8} {'Speed':>6}",
            file=sys.stderr,
        )
        print(f"  {'-'*50} {'-'*6} {'-'*6} {'-'*6} {'-'*6} {'-'*6} {'-'*12} {'-'*8} {'-'*8} {'-'*6}", file=sys.stderr)

        for r in sorted(valid, key=lambda x: x["composite"], reverse=True):
            name = _site_id(r["url"])
            if len(name) > 49:
                name = name[:46] + "..."
            elems = f"{r['bc_elements']}/{r['ch_elements']}"
            speedup = r["chrome_ms"] / r["render_ms"] if r["render_ms"] > 0 else 0
            print(
                f"  {name:<50} {r['composite']:>5.1f}% {r['tag_sim']:>5.1f}% "
                f"{r['text_sim']:>5.1f}% {r['id_sim']:>5.1f}% {r['class_sim']:>5.1f}% "
                f"{elems:>12} {r['render_ms']:>6.0f}ms {r['chrome_ms']:>6.0f}ms {speedup:>5.1f}x",
                file=sys.stderr,
            )

    if errors:
        print(f"\n  ERRORS ({len(errors)}):", file=sys.stderr)
        for r in errors:
            print(f"    {_site_id(r['url'])}: {r['error'][:100]}", file=sys.stderr)

    print(f"\n{'='*100}", file=sys.stderr)
