# Blazeweb Field Test — WikiLeaks Vault 8 Scrape

Date: 2026-04-20
Workload: Mirror the CIA Hive source repo from `https://wikileaks.org/vault8/document/repo_hive/` — ~3,000 source files served as static-HTML wrappers (`<div class="leak-content">` containing the file's content in `<pre>` blocks). Pages include MooTools + an e-highlighter JS overlay but **no content-bearing JavaScript** — the source code is in the static markup.

Driver code: `/home/aus/PycharmProjects/vault7/tools/v8_scrape.py` — pluggable HTTP backend (`--backend requests|blazeweb`), BFS dir walk, threadpool file fetching.

## Summary

| Backend | Workers | Files | Bytes extracted | Elapsed | Throughput | Notes |
|---|---|---|---|---|---|---|
| requests | 1 | 24 | 527,523 | 13.89 s | 1.7 files/s | baseline |
| requests | 4 | 7 | 35,258 | 2.75 s | 2.5 files/s | scales with concurrency |
| blazeweb | 1 | 24 | 527,521 | 51.68 s | 0.5 files/s | **3.7× slower** than requests/1 |
| blazeweb | 4 | — | — | — | — | **core dump** during concurrent fetches |

## Findings

### 1. `Client.fetch()` is not thread-safe — concurrent calls SIGSEGV

Repro:

```bash
cd /home/aus/PycharmProjects/vault7
timeout 60 /home/aus/PycharmProjects/blazeweb/.venv/bin/python \
    tools/v8_scrape.py --backend blazeweb --workers 4 --limit 3
# → "the monitored command dumped core"
```

Single shared `blazeweb.Client()` instance, fetched from a `concurrent.futures.ThreadPoolExecutor` with 4 workers, on plain HTTPS GETs to wikileaks.org. Crashes deterministically.

V8 isolates are explicitly single-thread by design — likely the underlying `_Client` is sharing one isolate across the threadpool and racing inside it. Either:
- The Rust binding needs to mark `Client.fetch` with the GIL held (so callers serialize), and document that, **or**
- It needs an internal isolate-per-thread / mutex / queue, **or**
- It should expose an async API that's the only sanctioned multi-call path.

Either way, **the current "shared Client + threads" pattern is a footgun** — it looks supported (Client is a long-lived stateful object) but isn't.

Two follow-ups worth doing:
- Add a `pytest` test that does exactly this: a few concurrent `Client.fetch` calls and asserts no crash.
- Document concurrency model in the `Client` docstring and the README.

### 2. Performance vs `requests` on JS-light pages

Single-thread, 24 files identical workload:

```
requests : 13.89 s   (1.7 files/s, 37.1 KiB/s)
blazeweb : 51.68 s   (0.5 files/s, 10.0 KiB/s)
```

This is an unfair head-to-head — requests doesn't execute JS at all, so it's "competing" the way a bicycle competes with a car on a one-lane road. The fairer competitors are Playwright/Puppeteer/Selenium (also full browser stacks) — that comparison is the one worth running. **Recommend benchmarking blazeweb vs `playwright.sync_api` on these same pages** to position it correctly.

That said, on this workload the cost split is presumably:
- HTTP fetch: ~same as requests
- HTML parse: cheap
- JS parse + execute (mootools-core-and-more.js, efatmarker.min.js — both ~50–100 KB): the rest

If the dominant cost is parsing the same MooTools script 24 times, the Client's script cache should help — but the throughput suggests the cache is either not warming or not deduping these. Worth investigating: is the cache key URL-based and being hit, or is each page treated as a cold compile? `Client.cache_size` after a run would tell you.

### 3. Output drift — 2-byte difference in extracted source

| | requests | blazeweb |
|---|---|---|
| 24 files | 527,523 | 527,521 |

The same parser (`BeautifulSoup` extracting `<pre>`'s `get_text()`) on both backends' returned HTML, off by 2 bytes total across 24 files. Probably trivial — V8's serializer normalizing whitespace, or the e-highlighter JS toggling a class that changes a text node — but it means **`blazeweb.fetch(url)` is not byte-identical to `requests.get(url).text` even on pages with cosmetic JS only**. Worth being aware of for any downstream signature/hash workflow. Easy diagnostic: render the same page both ways, save HTML to disk, `diff` them.

### 4. What worked well

- The `Client` API is clean — `cache=True`, `timeout`, `connect_timeout`, `max_connections_per_host` are exactly the knobs you'd want.
- Single-threaded calls returned correct, parsable HTML on every page tested. No render errors surfaced via `RenderResult.errors`.
- `RenderResult` subclassing `str` is genuinely nice — drop-in compatible with bs4 and downstream tooling.
- The TLS feature set (ECH GREASE, ALPS, post-quantum) is differentiated and a real reason to pick this over `requests` once it's threadsafe.

## Suggested benchmark to actually publish

Run on a corpus that exercises blazeweb's strengths (JS-bearing SPA-ish pages, not static markup):

| Tool | What it does | Fair to compare? |
|---|---|---|
| `requests` | HTTP only, no JS | Only as a "lower bound", not a competitor |
| `httpx` | HTTP + HTTP/2 | Same as requests |
| `playwright` | Full Chromium, JS execution | **Yes — direct competitor** |
| `puppeteer` (via pyppeteer or subprocess) | Full Chromium | **Yes** |
| `selenium` | Full browser | Yes (slowest) |
| `splash` | Headless WebKit + Lua | Yes (older) |
| `blazeweb` | Embedded V8 + html5ever | This project |

Ideal benchmark corpus: a few hundred URLs with real JS payload (single-page apps, dynamically-loaded content). Measure: cold/warm throughput, peak RSS, p50/p99 latency per page, output HTML correctness vs Playwright as gold standard.

## Decision for Vault 8

Continuing the scrape with the `requests` backend — these particular pages don't need a JS engine and the threadsafety bug blocks the obvious blazeweb win (concurrency).
