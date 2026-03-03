"""Analyze Intl API usage on crashing sites using Playwright/Chromium.

For each site that crashes in blazeweb, this script:
1. Loads the page in headless Chromium
2. Monitors JS console for Intl-related activity
3. Checks what Intl APIs are used and how many times
4. Reports memory usage and timing
"""

import json
import time
from playwright.sync_api import sync_playwright

CRASHING_SITES = [
    "https://www.tiktok.com",
    "https://vimeo.com",
    "https://dev.to",
    "https://techcrunch.com",
    "https://www.theverge.com",
    "https://www.stripe.com",
    "https://www.airbnb.com",
    "https://www.canva.com",
    "https://www.zoom.us",
]


def analyze_site(page, url: str) -> dict:
    """Load a site and analyze its Intl API usage."""
    result = {
        "url": url,
        "intl_calls": {},
        "errors": [],
        "timing_ms": 0,
        "js_heap_mb": 0,
    }

    # Inject Intl monitoring BEFORE any page scripts run
    page.add_init_script("""
    (function() {
        window.__intlLog = {};

        // Wrap Intl.DateTimeFormat
        const origDTF = Intl.DateTimeFormat;
        Intl.DateTimeFormat = function(...args) {
            window.__intlLog['DateTimeFormat'] = (window.__intlLog['DateTimeFormat'] || 0) + 1;
            try {
                return new origDTF(...args);
            } catch(e) {
                window.__intlLog['DateTimeFormat_errors'] = (window.__intlLog['DateTimeFormat_errors'] || 0) + 1;
                throw e;
            }
        };
        Intl.DateTimeFormat.prototype = origDTF.prototype;
        Intl.DateTimeFormat.supportedLocalesOf = origDTF.supportedLocalesOf;

        // Wrap Intl.NumberFormat
        const origNF = Intl.NumberFormat;
        Intl.NumberFormat = function(...args) {
            window.__intlLog['NumberFormat'] = (window.__intlLog['NumberFormat'] || 0) + 1;
            return new origNF(...args);
        };
        Intl.NumberFormat.prototype = origNF.prototype;
        Intl.NumberFormat.supportedLocalesOf = origNF.supportedLocalesOf;

        // Wrap Intl.RelativeTimeFormat
        if (Intl.RelativeTimeFormat) {
            const origRTF = Intl.RelativeTimeFormat;
            Intl.RelativeTimeFormat = function(...args) {
                window.__intlLog['RelativeTimeFormat'] = (window.__intlLog['RelativeTimeFormat'] || 0) + 1;
                return new origRTF(...args);
            };
            Intl.RelativeTimeFormat.prototype = origRTF.prototype;
        }

        // Wrap Intl.ListFormat
        if (Intl.ListFormat) {
            const origLF = Intl.ListFormat;
            Intl.ListFormat = function(...args) {
                window.__intlLog['ListFormat'] = (window.__intlLog['ListFormat'] || 0) + 1;
                return new origLF(...args);
            };
            Intl.ListFormat.prototype = origLF.prototype;
        }

        // Wrap Intl.PluralRules
        if (Intl.PluralRules) {
            const origPR = Intl.PluralRules;
            Intl.PluralRules = function(...args) {
                window.__intlLog['PluralRules'] = (window.__intlLog['PluralRules'] || 0) + 1;
                return new origPR(...args);
            };
            Intl.PluralRules.prototype = origPR.prototype;
        }
    })();
    """)

    # Collect console errors
    console_errors = []
    page.on("console", lambda msg: console_errors.append(msg.text) if msg.type == "error" else None)

    start = time.time()
    try:
        page.goto(url, timeout=30000, wait_until="domcontentloaded")
        # Wait a bit for async scripts
        page.wait_for_timeout(3000)
    except Exception as e:
        result["errors"].append(f"Navigation: {e}")

    result["timing_ms"] = int((time.time() - start) * 1000)

    # Get Intl call counts
    try:
        intl_log = page.evaluate("window.__intlLog || {}")
        result["intl_calls"] = intl_log
    except Exception as e:
        result["errors"].append(f"Eval intl: {e}")

    # Get JS heap usage
    try:
        metrics = page.evaluate("""
        (() => {
            if (performance.memory) {
                return {
                    usedJSHeapSize: performance.memory.usedJSHeapSize,
                    totalJSHeapSize: performance.memory.totalJSHeapSize,
                };
            }
            return null;
        })()
        """)
        if metrics:
            result["js_heap_mb"] = round(metrics["usedJSHeapSize"] / 1024 / 1024, 1)
            result["total_heap_mb"] = round(metrics["totalJSHeapSize"] / 1024 / 1024, 1)
    except Exception as e:
        result["errors"].append(f"Metrics: {e}")

    # Get total script count and sizes
    try:
        script_info = page.evaluate("""
        (() => {
            const scripts = document.querySelectorAll('script');
            let totalSize = 0;
            let externalCount = 0;
            let inlineCount = 0;
            scripts.forEach(s => {
                if (s.src) externalCount++;
                else {
                    inlineCount++;
                    totalSize += (s.textContent || '').length;
                }
            });
            return { external: externalCount, inline: inlineCount, inlineKB: Math.round(totalSize/1024) };
        })()
        """)
        result["scripts"] = script_info
    except Exception as e:
        result["errors"].append(f"Scripts: {e}")

    if console_errors:
        result["console_errors"] = len(console_errors)

    return result


def main():
    with sync_playwright() as p:
        browser = p.chromium.launch(
            headless=True,
            args=["--enable-precise-memory-info"],
        )

        print(f"{'Site':<30} {'DTF':>5} {'NF':>5} {'RTF':>5} {'Heap MB':>8} {'Time ms':>8} {'Scripts':>10}")
        print("-" * 85)

        for url in CRASHING_SITES:
            context = browser.new_context(
                viewport={"width": 1920, "height": 1080},
                user_agent="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120.0.0.0",
            )
            page = context.new_page()

            try:
                result = analyze_site(page, url)
                dtf = result["intl_calls"].get("DateTimeFormat", 0)
                nf = result["intl_calls"].get("NumberFormat", 0)
                rtf = result["intl_calls"].get("RelativeTimeFormat", 0)
                heap = result.get("js_heap_mb", "?")
                timing = result["timing_ms"]
                scripts = result.get("scripts", {})
                script_str = f"{scripts.get('external', '?')}ext/{scripts.get('inline', '?')}inl"

                print(f"{url:<30} {dtf:>5} {nf:>5} {rtf:>5} {heap:>8} {timing:>8} {script_str:>10}")

                if result["errors"]:
                    for err in result["errors"]:
                        print(f"  ERROR: {err[:80]}")
            except Exception as e:
                print(f"{url:<30} FAILED: {e}")
            finally:
                context.close()

        browser.close()


if __name__ == "__main__":
    main()
