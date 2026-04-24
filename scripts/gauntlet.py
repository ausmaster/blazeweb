"""Performance gauntlet for blazeweb — maximize URLs/sec.

Sweeps concurrency, compares capture modes, compares drive paths, then
hits a big max-throughput run at the winning config. Run directly:

    uv run python scripts/gauntlet.py
    uv run python scripts/gauntlet.py --urls 150 --big 500 --max-c 64

Not a pytest. Prints a final summary line with the peak URL/s number.
"""

from __future__ import annotations

import argparse
import random
import statistics
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import blazeweb

URL_FILE = (
    Path(__file__).resolve().parent.parent / "experiments" / "servo_spike" / "urls_bench_big.txt"
)


def load_urls(n: int) -> list[str]:
    base = [
        ln.strip()
        for ln in URL_FILE.read_text().splitlines()
        if ln.strip() and not ln.startswith("#")
    ]
    urls: list[str] = []
    while len(urls) < n:
        urls.extend(base)
    random.Random(42).shuffle(urls)
    return urls[:n]


def classify(r) -> str:
    """ok = real 2xx/3xx response; http4xx = got bytes but error status; fail = nav dead."""
    if not r.status_code:
        return "fail"
    if r.status_code >= 400:
        return "http4xx"
    return "ok"


def count_ok(results, capture: str) -> int:
    if capture == "png":
        return sum(1 for b in results if b)
    return sum(1 for r in results if classify(r) == "ok")


def banner(msg: str) -> None:
    print(f"\n─── {msg} ───", flush=True)


def sweep_concurrency(
    urls: list[str], concurrencies: list[int], nav_timeout_ms: int
) -> tuple[int, float]:
    """Core perf question: how many concurrent pages give max URL/s?"""
    banner(
        f"concurrency sweep (capture=html, {len(urls)} URLs/run, nav_timeout={nav_timeout_ms}ms)"
    )
    print(f"  {'concurrency':>11}  {'URL/s':>7}  {'ok':>7}  {'elapsed':>7}")
    best_c, best_rate = 0, 0.0
    for c in concurrencies:
        with blazeweb.Client(concurrency=c, navigation_timeout_ms=nav_timeout_ms) as client:
            t0 = time.perf_counter()
            results = client.batch(urls, capture="html")
            elapsed = time.perf_counter() - t0
        rate = len(urls) / elapsed
        ok = count_ok(results, "html")
        marker = " ★" if rate > best_rate else ""
        print(f"  {c:>11d}  {rate:>6.2f}   {ok:>3d}/{len(urls):<3d}  {elapsed:>6.2f}s{marker}")
        if rate > best_rate:
            best_c, best_rate = c, rate
    return best_c, best_rate


def compare_capture_modes(urls: list[str], concurrency: int, nav_timeout_ms: int) -> None:
    """PNG adds cost. How much?"""
    banner(f"capture-mode comparison at concurrency={concurrency}")
    print(f"  {'mode':>6}  {'URL/s':>7}  {'ok':>7}  {'elapsed':>7}")
    for mode in ("html", "png", "both"):
        with blazeweb.Client(
            concurrency=concurrency, navigation_timeout_ms=nav_timeout_ms
        ) as client:
            t0 = time.perf_counter()
            results = client.batch(urls, capture=mode)
            elapsed = time.perf_counter() - t0
        rate = len(urls) / elapsed
        ok = count_ok(results, mode)
        print(f"  {mode:>6}  {rate:>6.2f}   {ok:>3d}/{len(urls):<3d}  {elapsed:>6.2f}s")


def python_threads_drive(
    urls: list[str], concurrency: int, threads: int, nav_timeout_ms: int
) -> None:
    """Prove the GIL-release model: N Python threads drive ONE Client, in parallel."""
    banner(f"Python-thread drive — {threads} threads × Client(concurrency={concurrency})")
    with blazeweb.Client(
        concurrency=concurrency, navigation_timeout_ms=nav_timeout_ms
    ) as client:
        latencies: list[float] = []
        errors = 0

        def work(url: str) -> float:
            t = time.perf_counter()
            try:
                r = client.fetch(url)
                if classify(r) != "ok":
                    return -1.0
            except Exception:
                return -1.0
            return time.perf_counter() - t

        t0 = time.perf_counter()
        with ThreadPoolExecutor(max_workers=threads) as pool:
            for lat in pool.map(work, urls):
                if lat >= 0:
                    latencies.append(lat)
                else:
                    errors += 1
        elapsed = time.perf_counter() - t0
    rate = len(urls) / elapsed
    print(f"  {rate:.2f} URL/s   {len(urls) - errors}/{len(urls)} ok   {elapsed:.2f}s")
    if latencies:
        s = sorted(latencies)

        def pctile(q: float) -> float:
            return s[min(int(len(s) * q), len(s) - 1)]

        print(
            f"  per-URL latency: p50={statistics.median(s):.2f}s  "
            f"p95={pctile(0.95):.2f}s  p99={pctile(0.99):.2f}s"
        )


def max_throughput(
    urls: list[str], concurrency: int, capture: str, nav_timeout_ms: int
) -> float:
    """Biggest run at winning concurrency — the headline number."""
    banner(
        f"MAX THROUGHPUT — {len(urls)} URLs, concurrency={concurrency}, capture={capture}"
    )
    with blazeweb.Client(concurrency=concurrency, navigation_timeout_ms=nav_timeout_ms) as client:
        t0 = time.perf_counter()
        results = client.batch(urls, capture=capture)
        elapsed = time.perf_counter() - t0
    rate = len(urls) / elapsed
    if capture == "png":
        ok = sum(1 for b in results if b)
        print(
            f"  ★ {rate:.2f} URL/s   {ok}/{len(urls)} ok   wall {elapsed:.1f}s   "
            f"png {sum(len(b) for b in results) / 1e6:.1f} MB"
        )
    else:
        buckets = {"ok": 0, "fail": 0, "http4xx": 0}
        for r in results:
            buckets[classify(r)] += 1
        print(
            f"  ★ {rate:.2f} URL/s   "
            f"{buckets['ok']} ok / {buckets['http4xx']} 4xx / {buckets['fail']} fail   "
            f"wall {elapsed:.1f}s"
        )
        if capture == "both":
            html_mb = sum(len(r.html) for r in results) / 1e6
            png_mb = sum(len(r.png) for r in results) / 1e6
            print(f"  payload: {html_mb:.1f} MB html + {png_mb:.1f} MB png")
    return rate


def prewarm_and_filter(urls: list[str], nav_timeout_ms: int) -> list[str]:
    """Hit every unique URL twice, keep only ones that succeeded both times.

    Also warms DNS + Chrome cache. Returns a clean URL list so subsequent sweeps
    measure ACTUAL throughput rather than nav-timeout wall time from flaky URLs.
    """
    uniq = sorted(set(urls))
    banner(f"prewarm + filter — probing {len(uniq)} unique URLs (2 passes)")
    survivors: set[str] = set(uniq)
    with blazeweb.Client(concurrency=16, navigation_timeout_ms=nav_timeout_ms) as client:
        for pass_n in (1, 2):
            t = time.perf_counter()
            results = client.batch(uniq, capture="html")
            elapsed = time.perf_counter() - t
            bad = set()
            for url, r in zip(uniq, results, strict=True):
                if classify(r) != "ok":
                    bad.add(url)
            survivors -= bad
            print(
                f"  pass {pass_n}: {len(uniq) - len(bad)}/{len(uniq)} ok in {elapsed:.1f}s "
                f"(dropped {len(bad)} this pass)"
            )
    final = [u for u in uniq if u in survivors]
    print(f"  survivors: {len(final)} URLs (will repeat these to fill sweep/big runs)")
    return final


def expand(clean: list[str], n: int) -> list[str]:
    """Repeat a clean URL list up to n entries, shuffled."""
    urls: list[str] = []
    while len(urls) < n:
        urls.extend(clean)
    random.Random(42).shuffle(urls)
    return urls[:n]


def main() -> int:
    ap = argparse.ArgumentParser(description="blazeweb performance gauntlet")
    ap.add_argument("--urls", type=int, default=100, help="URLs per sweep iteration")
    ap.add_argument("--big", type=int, default=500, help="URLs for the max-throughput run")
    ap.add_argument("--max-c", type=int, default=128, help="max concurrency in sweep")
    ap.add_argument("--threads", type=int, default=32, help="Python threads for drive test")
    ap.add_argument(
        "--nav-timeout", type=int, default=10000,
        help="per-URL nav timeout (ms). Short timeout keeps tail URLs from dominating.",
    )
    args = ap.parse_args()

    raw_pool = load_urls(args.urls)
    print(
        f"blazeweb performance gauntlet\n"
        f"  sweep={args.urls}  big={args.big}  max-c={args.max_c}  threads={args.threads}"
        f"  nav-timeout={args.nav_timeout}ms\n"
        f"  URL pool: {len(set(raw_pool))} unique",
        flush=True,
    )

    clean = prewarm_and_filter(raw_pool, args.nav_timeout)
    if not clean:
        print("No URLs survived filter — network issue?")
        return 2
    sweep_urls = expand(clean, args.urls)
    big_urls = expand(clean, args.big)

    concurrencies = [c for c in (4, 8, 16, 32, 48, 64, 96, 128) if c <= args.max_c]
    best_c, best_sweep_rate = sweep_concurrency(sweep_urls, concurrencies, args.nav_timeout)
    compare_capture_modes(sweep_urls, best_c, args.nav_timeout)
    python_threads_drive(sweep_urls, best_c, args.threads, args.nav_timeout)
    big_html = max_throughput(big_urls, best_c, "html", args.nav_timeout)
    big_both = max_throughput(big_urls, best_c, "both", args.nav_timeout)

    print(
        f"\n═══ FINAL ═══\n"
        f"  best concurrency    : {best_c}\n"
        f"  sweep peak (html)   : {best_sweep_rate:.2f} URL/s\n"
        f"  big run (html only) : {big_html:.2f} URL/s ({args.big} URLs)\n"
        f"  big run (html+png)  : {big_both:.2f} URL/s ({args.big} URLs)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
