# blazeweb

**URL → fully-rendered HTML (and optionally a screenshot), for Python.**

blazeweb is a Rust + PyO3 Python package that wraps Chromium via the Chrome
DevTools Protocol. It gives you Playwright-class output (post-JavaScript DOM,
PNG screenshots, HTTP header injection, locale/timezone/geo emulation) at
roughly half the per-URL overhead — because there's no Python-to-Node
driver hop in the call chain.

```python
import blazeweb

# fully rendered HTML, post-JS
html = blazeweb.fetch("https://example.com")

# screenshot — PNG by default; JPEG/WebP with quality
png = blazeweb.screenshot("https://example.com")
jpg = blazeweb.screenshot("https://example.com", format="jpeg", quality=80)

# both, from a single page visit (HTML is free once you've loaded)
both = blazeweb.fetch_all("https://example.com")

# Rust-side CSS query — no Python HTML parsing tax
print(html.dom.title())              # "Example Domain"
print(html.dom.find("h1").text)      # "Example Domain"
print(html.dom.links())              # ["https://iana.org/domains/example"]
```

Also available as a CLI:

```bash
python -m blazeweb https://example.com                   # HTML → stdout
python -m blazeweb https://example.com -o page.html      # → file
python -m blazeweb https://example.com -s shot.jpg       # screenshot + HTML
python -m blazeweb https://example.com --screenshot-only shot.webp  # image only
python -m blazeweb https://example.com --json            # JSON w/ metadata
```

## Install

```bash
pip install blazeweb
```

That's it — the wheel ships Chrome (headless-shell 148) inside. No extra
`playwright install` step, no system chromium required.

## Why blazeweb

If you need fully-rendered HTML from a URL (i.e. after JavaScript has run),
your existing options are:

- **`requests` / `httpx`** — fast, but they don't run JS. Modern SPAs return
  an empty `<div id="root">` and nothing useful.
- **BeautifulSoup / lxml** — parse HTML you already have. Doesn't fetch.
- **Playwright-python** — capable but Python → Node driver chain adds latency
  per CDP call; ~2.7s/URL on our bench vs blazeweb's ~1.9s/URL at equal
  concurrency.
- **Selenium** — older, slower, browser-driver abstraction.

blazeweb's niche: **URL → fully-rendered HTML + optional PNG, fast, Python-native,
one pip install.** Specifically tuned for high-throughput scraping pipelines
(BBOT-style subdomain fan-outs, security recon, change detection) where you
want hundreds of URLs per minute from a single process.

### Benchmarks (48-URL stable gauntlet, 16-core Linux, ``chrome-headless-shell 148``)

| Engine                                            | Config        | URL/s   |
|---------------------------------------------------|---------------|---------|
| blazeweb (this package)                           | P=16 mode=both| **8.54**|
| Playwright-python                                 | P=16          | 5.82    |
| Chromium headless (CLI fork-per-URL)              | P=16          | 4.51    |
| Servo 0.1.0 in-process                            | P=8           | 1.13    |

Full methodology + breakdown in ``experiments/BENCHMARKS.md``.

## The core API

### Module-level convenience

One-shot calls use a shared, lazy-initialized `Client`. Good for scripts and
notebooks. For high-throughput work, instantiate your own `Client` so you
can tune `concurrency` and re-use the warm chromium.

```python
blazeweb.fetch(url)                        # → RenderResult (str subclass + metadata)
blazeweb.screenshot(url)                   # → image bytes (PNG by default)
blazeweb.screenshot(url, format="jpeg", quality=80)  # JPEG
blazeweb.screenshot(url, format="webp", quality=80)  # WebP
blazeweb.fetch_all(url)                    # → FetchResult (html + image)
```

### Persistent `Client` with config

Three equivalent ways to configure:

```python
# 1. Flat kwargs — most common
with blazeweb.Client(
    viewport=(1920, 1080),
    user_agent="MyScraper/1.0",
    concurrency=16,
    locale="en-GB",
    timezone="Europe/London",
    block_urls=["*doubleclick*", "*.googletagmanager.com/*"],
) as client:
    ...

# 2. Structured pydantic config
from blazeweb import ClientConfig, NetworkConfig, EmulationConfig
cfg = ClientConfig(
    concurrency=32,
    network=NetworkConfig(user_agent="X", extra_headers={"X-Run": "abc"}),
    emulation=EmulationConfig(locale="ja-JP"),
)
client = blazeweb.Client(config=cfg)

# 3. Env vars (auto-loaded by pydantic-settings)
#   BLAZEWEB_CONCURRENCY=32 BLAZEWEB_VIEWPORT__WIDTH=1920 python script.py
client = blazeweb.Client()
```

### Batching at high concurrency

`client.batch(urls, capture=...)` dispatches N URLs in parallel inside
Rust's tokio runtime, capped by the Client's `concurrency`:

```python
urls = [...]  # thousands
with blazeweb.Client(concurrency=16) as client:
    for result in client.batch(urls, capture="both"):  # "html"|"png"|"both"
        if result.html.dom.exists("meta[name='generator']"):
            ...
```

### Live config updates at runtime

The `client.config` attribute is a live proxy — attribute writes at any depth
take effect on the next fetch:

```python
client.config.network.user_agent = "Bot/2.0"       # next fetch picks up
client.config.emulation.locale = "ja-JP"
client.config.viewport.width = 2560
```

Launch-only fields (things Chrome needs at startup — `concurrency`, chrome
args, proxy, `ignore_https_errors`) raise `ValueError` at the assignment line
so you see the error immediately:

```python
client.config.concurrency = 32         # ValueError — recreate Client instead
client.config.chrome.args = ["--x"]    # ValueError — set at construction
```

### Thread-safe by design

A single `Client` is safe to share across Python threads; every public method
releases the GIL before entering Rust. N Python threads all do real parallel
work inside one tokio runtime, gated by the Client's `concurrency` semaphore:

```python
with blazeweb.Client(concurrency=16) as client:
    with ThreadPoolExecutor(max_workers=16) as pool:
        results = list(pool.map(client.fetch, urls))
```

### Rust-side HTML query — fast by default

`result.dom` is a lazy Rust-parsed DOM with both CSS-selector and BS4-style
lookups. Parsing + querying in Rust avoids the Python HTML-parsing tax on
high-volume pipelines:

```python
r = blazeweb.fetch(url)

# CSS selectors
r.dom.query("a[href^='https://']")     # → list[Element]
r.dom.query_one("meta[name='generator']")
r.dom.exists("script[type='module']")  # → bool

# BS4-familiar
r.dom.find("nav", class_="main-nav")
r.dom.find_all("div", class_="card", limit=10)

# Shortcuts
r.dom.title()                          # <title> text
r.dom.links()                          # all <a href=...>
r.dom.images()                         # all <img src=...>

# Fast substring — skips the html5ever parse entirely
if r.dom.contains("Cloudflare"): ...
```

### Navigation lifecycle — `wait_until` and `wait_after_ms`

Two knobs control when a fetch returns:

```python
client = blazeweb.Client(
    wait_until="load",        # default — window.onload (complete, Playwright-default)
    # wait_until="domcontentloaded",  # opt-in — parser done, no subresource wait
    wait_after_ms=0,          # additional post-event sleep in ms
)

# Per-call override
client.fetch(url, wait_until="domcontentloaded")
client.fetch(url, wait_after_ms=500)   # settle 500ms after load for SPA hydration
```

- `load` (default) waits for all subresources + deferred scripts; most complete.
- `domcontentloaded` returns as soon as the DOM parser finishes — faster on
  tracker-heavy sites but may miss post-DCL SPA mutations. In our A/B this
  saved single-digit ms per URL on lean sites, so it's a real knob but not a
  massive win at scale.
- `wait_after_ms` adds a fixed sleep after the chosen event — useful for
  React/Vue/etc. pages that hydrate post-load.

### CLI — `python -m blazeweb`

```
python -m blazeweb <URL>                     # HTML → stdout
python -m blazeweb <URL> -o out.html         # HTML → file, stdout silent
python -m blazeweb <URL> -s shot.png         # HTML → stdout, screenshot → file
python -m blazeweb <URL> --screenshot-only shot.jpg   # no HTML, image-only
python -m blazeweb <URL> --json              # metadata + html as JSON
python -m blazeweb <URL> --meta              # final_url/status → stderr
```

Image format is inferred from the output extension (`.jpg` / `.jpeg` → jpeg,
`.webp` → webp, else png). Override with `--format png|jpeg|webp --quality N`.

All the Client config knobs are available as flags: `--user-agent`,
`--width`, `--height`, `--timeout-ms`, `--locale`, `--timezone`, `--proxy`,
`--header K=V`, `--headers-file FILE`, `--no-js`, `--ignore-certs`, `--chrome PATH`.

### Logging

Both Rust and Python sides emit structured logs. Control globally via the
`BLAZEWEB_LOG` env var (or `RUST_LOG` as fallback); runtime via
`blazeweb.set_log_level()`:

```bash
BLAZEWEB_LOG=info  python script.py       # launch, close, batch dispatch
BLAZEWEB_LOG=debug python script.py       # per-fetch entry/exit + timings
BLAZEWEB_LOG=trace python script.py       # every CDP step with millisecond timing
BLAZEWEB_LOG='blazeweb::engine=trace,warn' python script.py   # per-module
```

```python
import blazeweb
blazeweb.set_log_level("debug")           # Python + Rust, both sides
blazeweb.logger.info("my app event")       # hierarchical under "blazeweb.*"
```

Bare levels auto-narrow to `blazeweb` only — you won't drown in
tungstenite/hyper chatter when you set `BLAZEWEB_LOG=debug`.

## Configuration reference

Every knob lives under a nested sub-config. Flat kwargs on `Client(...)` and
`ClientConfig.from_flat(...)` map to the corresponding nested field.

| Section        | Fields                                                                              |
|----------------|-------------------------------------------------------------------------------------|
| (top level)    | `concurrency`, `wait_until`, `wait_after_ms`                                         |
| `viewport`     | `width`, `height`, `device_scale_factor`, `mobile`                                  |
| `network`      | `user_agent`, `proxy`, `extra_headers`, `ignore_https_errors`, `block_urls`, `disable_cache`, `offline`, `latency_ms`, `download_bps`, `upload_bps` |
| `emulation`    | `locale`, `timezone`, `geolocation`, `prefers_color_scheme`, `javascript_enabled`   |
| `timeout`      | `navigation_ms`, `launch_ms`, `screenshot_ms`                                        |
| `chrome`       | `path`, `args`, `user_data_dir`, `headless`                                          |

Per-call overrides (on `FetchConfig` / `ScreenshotConfig`): `extra_headers`,
`timeout_ms`, `wait_until`, `wait_after_ms`. `ScreenshotConfig` also takes
`viewport`, `full_page`, `format`, `quality`.

**Env vars**: set via `BLAZEWEB_` prefix + `__` delimiter for nesting.
`BLAZEWEB_CONCURRENCY=32`, `BLAZEWEB_VIEWPORT__WIDTH=1920`,
`BLAZEWEB_NETWORK__USER_AGENT='Mozilla/5.0 …'`.

## What blazeweb is NOT

- **Not a full browser automation framework.** No element clicking, form
  filling, screenshot-of-selector, auto-waiting for arbitrary conditions,
  multiple-browser-types (we target Chromium only). Use Playwright for that.
- **Not a TLS fingerprint tool.** Chrome's native JA3 is what goes on the
  wire. If you need curl-impersonate-style TLS spoofing, route through an
  upstream proxy that does it.
- **Not invisible-scraper territory.** Headless Chrome is detectable. If
  you need full anti-detection (canvas fingerprinting, CDP detection,
  stealth JS patches), that's a different stack.

## Development

```bash
# Install dev deps
uv pip install -e '.[dev,bench]'

# Build (incremental; full build ~2 min cold)
maturin develop --uv

# Run tests
uv run pytest tests/ -v

# Refresh the bundled Chrome binary for this platform
python scripts/download-chrome.py              # current platform only
python scripts/download-chrome.py --all        # every supported platform (for wheel builds)
python scripts/download-chrome.py --force      # re-download even if present

# Build a wheel
maturin build --release
```

Supported platforms for the bundled binary: linux-x86_64, linux-aarch64,
darwin-x86_64, darwin-aarch64, windows-x86_64. (v2.0 ships linux-x86_64;
others in follow-ups.)

## License

blazeweb is Apache-2.0 or MIT (your choice). The bundled
`chrome-headless-shell` is BSD-3-Clause (Google Chrome for Testing).
