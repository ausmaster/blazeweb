#!/usr/bin/env python3
"""Playwright-python parity benchmark.

Mirrors chromiumoxide_spike's batch mode as closely as possible:
- Same chrome-headless-shell binary via executable_path
- Same viewport (1200x800), same per-URL timeout
- Same concurrency model: one browser, N pages via asyncio.gather with a Semaphore
- Same output shape: JSON line per URL on stdout
- Same modes: --mode png | html | both

Usage:
  python3 playwright_bench.py --chrome /path/to/chrome-headless-shell \
          --out-dir out --concurrency 16 --mode both --timeout-secs 20 < urls.txt
"""

from __future__ import annotations

import argparse
import asyncio
import json
import re
import sys
import time
from pathlib import Path

from playwright.async_api import async_playwright


def sanitize(url: str) -> str:
    return re.sub(r"[^A-Za-z0-9]", "_", url)[:96]


async def shoot(context, url: str, out_dir: Path, mode: str, timeout_secs: float) -> dict:
    t0 = time.perf_counter()
    page = await context.new_page()
    try:
        await page.goto(url, wait_until="load", timeout=int(timeout_secs * 1000))
        name = sanitize(url)
        png_path = None
        html_path = None
        html_bytes = None
        if mode in ("png", "both"):
            p = out_dir / f"{name}.png"
            await page.screenshot(path=str(p))
            png_path = str(p)
        if mode in ("html", "both"):
            html = await page.content()
            p = out_dir / f"{name}.html"
            p.write_text(html)
            html_path = str(p)
            html_bytes = len(html)
        elapsed = time.perf_counter() - t0
        return {
            "url": url,
            "ok": True,
            "png": png_path,
            "html": html_path,
            "html_bytes": html_bytes,
            "elapsed_s": round(elapsed, 4),
        }
    except Exception as e:
        return {"url": url, "ok": False, "error": str(e), "elapsed_s": round(time.perf_counter() - t0, 4)}
    finally:
        await page.close()


async def amain(args: argparse.Namespace) -> int:
    args.out_dir.mkdir(parents=True, exist_ok=True)
    urls = [ln.strip() for ln in sys.stdin if ln.strip() and not ln.strip().startswith("#")]
    if not urls:
        print("no URLs on stdin", file=sys.stderr)
        return 1

    init_t0 = time.perf_counter()
    async with async_playwright() as p:
        browser = await p.chromium.launch(
            executable_path=str(args.chrome),
            headless=True,
            args=[
                "--disable-gpu",
                "--no-sandbox",
                "--hide-scrollbars",
                "--disable-dev-shm-usage",
                f"--window-size={args.width},{args.height}",
            ],
        )
        context = await browser.new_context(
            viewport={"width": args.width, "height": args.height},
        )
        init_elapsed = time.perf_counter() - init_t0
        print(f"playwright up in {init_elapsed:.2f}s", file=sys.stderr)

        sem = asyncio.Semaphore(args.concurrency)
        batch_t0 = time.perf_counter()

        async def worker(url: str):
            async with sem:
                return await shoot(context, url, args.out_dir, args.mode, args.timeout_secs)

        tasks = [asyncio.create_task(worker(u)) for u in urls]
        for fut in asyncio.as_completed(tasks):
            result = await fut
            sys.stdout.write(json.dumps(result, separators=(",", ":")) + "\n")
            sys.stdout.flush()

        await browser.close()
        batch_elapsed = time.perf_counter() - batch_t0
        print(
            f"batch done in {batch_elapsed:.2f}s (init {init_elapsed:.2f}s, {len(urls)} urls)",
            file=sys.stderr,
        )
    return 0


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--out-dir", type=Path, default=Path("shots_playwright"))
    p.add_argument("--width", type=int, default=1200)
    p.add_argument("--height", type=int, default=800)
    p.add_argument("--timeout-secs", type=float, default=20.0)
    p.add_argument("--concurrency", type=int, default=1)
    p.add_argument("--chrome", type=Path, required=True)
    p.add_argument("--mode", choices=["png", "html", "both"], default="png")
    args = p.parse_args()
    return asyncio.run(amain(args))


if __name__ == "__main__":
    sys.exit(main())
