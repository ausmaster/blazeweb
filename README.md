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

With [uv](https://docs.astral.sh/uv/) (recommended):

```bash
uv add blazeweb
```

Or pip:

```bash
pip install blazeweb
```

The wheel ships chrome-headless-shell 148 bundled inside — no extra
`playwright install` step, no system chromium required.

### Platforms without a pre-built wheel

When your platform doesn't have a matching wheel on PyPI, pip/uv falls
back to a source build. You'll need a stable Rust toolchain
([rustup](https://rustup.rs) is the easy path); maturin takes over from
there and compiles the native extension as part of the install.

```bash
# Triggers the source build on install — Rust compiles
# blazeweb._blazeweb as part of the step.
uv add blazeweb
```

Source installs don't include the bundled Chromium (that only ships
inside wheels). Fetch it once after install:

```bash
uv run blazeweb-download-chrome      # ~100 MB, one-time
```

Or, if you'd rather not bundle, install a system Chromium
(`chromium`, `chromium-browser`, `chrome`, or `google-chrome` on PATH).
blazeweb auto-resolves in order:

1. Explicit `chrome_path=` passed to `Client(...)`.
2. Bundled binary in `python/blazeweb/_binaries/<platform>/`
   (populated by `blazeweb-download-chrome`).
3. The first system binary found on PATH.

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

blazeweb's sweet spot: **URL → fully-rendered HTML + optional PNG, fast,
Python-native, one pip install.** Tuned for high-throughput read-mostly
pipelines (BBOT-style subdomain fan-outs, security recon, change detection)
where you want hundreds of URLs per minute from a single process.

### Benchmarks (48-URL stable gauntlet, 16-core Linux, ``chrome-headless-shell 148``)

| Engine                                            | Config        | URL/s   |
|---------------------------------------------------|---------------|---------|
| blazeweb (this package)                           | P=16 mode=both| **8.54**|
| Playwright-python                                 | P=16          | 5.82    |
| Chromium headless (CLI fork-per-URL)              | P=16          | 4.51    |
| Servo 0.1.0 in-process                            | P=8           | 1.13    |

Full methodology + breakdown in ``BENCHMARKS.md``.

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

Presets plug in via `--preset <module>.<NAME>` (with explicit flags
overriding preset fields when they overlap):

```bash
python -m blazeweb --preset stealth.BASIC https://cnn.com
python -m blazeweb --preset recon.FAST https://example.com -o page.html
python -m blazeweb --preset archival.FULL_PAGE https://spa.example/ --screenshot shot.webp

python -m blazeweb --preset list    # print every known preset and exit
```

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
| `network`      | `user_agent`, `user_agent_metadata`, `proxy`, `extra_headers`, `ignore_https_errors`, `block_urls`, `disable_cache`, `offline`, `latency_ms`, `download_bps`, `upload_bps` |
| `emulation`    | `locale`, `timezone`, `geolocation`, `prefers_color_scheme`, `javascript_enabled`   |
| `scripts`      | `on_new_document`, `on_dom_content_loaded`, `on_load`, `isolated_world`, `url_scoped` |
| `timeout`      | `navigation_ms`, `launch_ms`, `screenshot_ms`                                        |
| `chrome`       | `path`, `args`, `user_data_dir`, `headless`                                          |

Per-call overrides (on `FetchConfig` / `ScreenshotConfig`): `extra_headers`,
`timeout_ms`, `wait_until`, `wait_after_ms`. `ScreenshotConfig` also takes
`viewport`, `full_page`, `format`, `quality`.

**Env vars**: set via `BLAZEWEB_` prefix + `__` delimiter for nesting.
`BLAZEWEB_CONCURRENCY=32`, `BLAZEWEB_VIEWPORT__WIDTH=1920`,
`BLAZEWEB_NETWORK__USER_AGENT='Mozilla/5.0 …'`. List fields (e.g.
`BLAZEWEB_NETWORK__BLOCK_URLS`) JSON-parse: `'["*ad*","*track*"]'`.

**Runtime mutation**: `client.config.<section>.<field> = value` at any
depth auto-syncs to the Rust engine; next fetch picks it up. Launch-only
fields (`concurrency`, `chrome.*`, `network.proxy`, etc.) raise
`ValueError` at the offending assignment. Call `client.config.snapshot()`
for a detached deep-copy.

## Scripts, client hints, presets

blazeweb's config covers everything you'd otherwise have to bolt on after
the fact — JS injection with timing & scope, structured client-hint
metadata, ad/tracker blocking, proxy, network throttling, locale/timezone
overrides. Use the fields directly for one-off setups, or bundle related
settings into a **preset** (a plain `dict`) for reuse.

### Injecting JavaScript — `ScriptsConfig`

Declarative JS injection, applied per pool page via CDP's
`Page.addScriptToEvaluateOnNewDocument`. Five fields cover the common
timing / scope choices:

```python
Client(scripts={
    "on_new_document":       [js],  # fires before any page script, every nav
    "on_dom_content_loaded": [js],  # wrapped in a DOMContentLoaded listener
    "on_load":               [js],  # wrapped in a window.load listener
    "isolated_world":        [js],  # runs in world "blazeweb_isolated";
                                    # page JS can't read the globals it sets
    "url_scoped":            {"/path": [js]},  # substring-gated by URL
})
```

`on_new_document` is the CDP primitive; the rest are sugar implemented by
wrapping the source. `isolated_world` is genuinely separate — page scripts
live in one JS global, the isolated world in another (DOM is shared).
Useful for scraping logic that mustn't be observable by page JS.

Example — extract JSON-LD structured data during navigation:

```python
EXTRACT_LDJSON = """
document.addEventListener('DOMContentLoaded', () => {
  const nodes = document.querySelectorAll('script[type="application/ld+json"]');
  const data = Array.from(nodes).map(n => n.textContent);
  document.documentElement.dataset.ldjson = JSON.stringify(data);
});
"""
with Client(scripts={"on_new_document": [EXTRACT_LDJSON]}) as c:
    html = c.fetch(url)
    # data-ldjson attribute now present on <html>; pull it out with .dom.find(...)
```

**CDP-level limitations** (not blazeweb's — documented so you don't get
surprised):
- Scripts do NOT propagate into cross-origin iframes.
- Scripts do NOT run in service workers / shared workers.
- Runtime updates to `config.scripts.*` affect only *new* pool pages;
  existing pool pages keep their original registrations.

### Structured User-Agent — `network.user_agent_metadata`

The `User-Agent` header has a structured counterpart: the `Sec-CH-UA-*`
client hints. If you override the UA but leave the client hints alone,
servers that compare the two see a mismatch — itself a fingerprinting
tell. Pair them:

```python
Client(
    user_agent="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
               "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    user_agent_metadata={
        "brands": [
            {"brand": "Google Chrome", "version": "131"},
            {"brand": "Chromium",      "version": "131"},
            {"brand": "Not_A Brand",   "version": "24"},
        ],
        "platform": "Linux",
        "platform_version": "",
        "architecture": "x86",
        "model": "",
        "mobile": False,
        "bitness": "64",
    },
)
```

### Presets — bundles you can spread

A preset is a `dict` of kwargs. Spread it into `Client(...)`:

```python
from blazeweb import Client
from blazeweb.presets import stealth, recon, archival

Client(**recon.FAST).fetch(url)

# Pre-merge to tweak (Python forbids duplicate kwargs across multiple
# ** spreads or between ** and an explicit kwarg)
Client(**{**recon.FAST, "user_agent": "CorpScanner/1.0"}).fetch(url)

# Compose two presets
Client(**{**stealth.BASIC, **recon.FAST}).fetch(url)
```

Rolling your own is just a `dict` literal:

```python
CORP_CRAWLER = {
    "user_agent": "CorpCrawler/2.0 (+https://corp.example/crawler)",
    "extra_headers": {"X-Crawler-Token": os.environ["CRAWLER_TOKEN"]},
    "block_urls": ["*://*.tracker.example/*"],
    "scripts": {"on_load": [EXTRACT_LDJSON]},
}
with Client(**CORP_CRAWLER) as c: ...
```

**Built-in presets:**

| Preset                          | What it sets                                                                                         | When to reach for it                                                |
|---------------------------------|------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------|
| `presets.stealth.BASIC`         | UA brand swap + matching `Sec-CH-UA` + 5 JS patches (webdriver, chrome.runtime, plugins, permissions, hardware) | Akamai-fronted sites that first-byte-match `HeadlessChrome`         |
| `presets.stealth.FINGERPRINT`   | `BASIC` + WebGL vendor/renderer override + canvas `toDataURL` noise                                   | DataDome / PerimeterX / lower-sensitivity Cloudflare                |
| `presets.recon.FAST`            | `javascript_enabled=False`, 5s nav timeout, ad/tracker block list                                     | BBOT-style subdomain sweeps; you want bytes, fast, no JS            |
| `presets.archival.FULL_PAGE`    | 1920×1080 viewport, 30s nav timeout, 2s post-load settle                                              | Change detection / wayback-style snapshots of SPA-heavy sites       |

### Stealth preset — what each patch counters

`blazeweb.presets.stealth` is modeled on
[rebrowser-patches](https://github.com/rebrowser/rebrowser-patches) —
opt-in, off by default, every patch commented with the detection vector
it counters. No silent evasion.

The default chrome-headless-shell UA contains the literal substring
`HeadlessChrome`, which Akamai Bot Manager first-byte-matches to return a
250-byte `Unknown Error` stub in ~60 ms (observed on `cnn.com`, `reuters.com`,
`linkedin.com`). Beyond UA, anti-bot scripts probe JS runtime globals.

| `stealth.BASIC` patch                                             | Counters                                                      |
|-------------------------------------------------------------------|---------------------------------------------------------------|
| UA + matching `Sec-CH-UA` brand metadata                          | First-byte UA substring checks; client-hint consistency       |
| `navigator.webdriver` → `undefined`                               | The most-detected automation tell                             |
| `window.chrome.runtime` populated                                 | Checks for `chrome.runtime.OnInstalledReason`                 |
| `navigator.plugins` with 5 PDF-viewer entries                     | `plugins.length === 0` heuristic                              |
| `navigator.permissions.query` returns `default` for notifications | Headless returns `denied`; real Chrome returns `default`      |
| `navigator.hardwareConcurrency = 8`, `deviceMemory = 8`           | CI-env low-value heuristic                                    |

| `stealth.FINGERPRINT` also adds                                        | Counters                                                       |
|------------------------------------------------------------------------|----------------------------------------------------------------|
| WebGL `UNMASKED_VENDOR_WEBGL` / `UNMASKED_RENDERER_WEBGL` override     | `SwiftShader` identity as the software-renderer giveaway       |
| Canvas `toDataURL` per-session noise                                   | Bit-identical canvas fingerprint hash                          |

**What stealth doesn't fix** (documented rather than silently missing):

- **TLS ClientHello fingerprint** — chrome-headless-shell uses the same
  BoringSSL build as full Chrome, so JA3/JA4 already matches "real Chrome"
  by default. Vendors that fingerprint further at the network layer need
  something like `curl-impersonate` or the retired `wreq`/BoringSSL path
  that lived on blazeweb's pre-CDP branch.
- **Cross-origin iframe propagation** — CDP init scripts don't reach
  cross-origin iframes. Cloudflare Turnstile specifically runs in one.
- **Service-worker / shared-worker scope** — same limitation.
- **Behavioral simulation** — mouse curves, scroll physics, timing jitter.
- **`cdc_*` window property** injected when `Runtime.enable` is called —
  requires a Chromium binary patch (see rebrowser-patches).

## Development

Prerequisites: [`uv`](https://docs.astral.sh/uv/) and a stable Rust toolchain
(`rustup` recommended — `rust-toolchain.toml` pins the channel). Python is
auto-managed by `uv` via `.python-version` (3.11).

```bash
# One-shot env setup: creates .venv, installs deps, builds the Rust extension
# in editable mode. Safe to re-run — incremental.
uv sync

# Fetch chrome-headless-shell for this platform (~100 MB, one-time)
uv run blazeweb-download-chrome

# Run things — no venv activation needed
uv run blazeweb https://example.com              # the CLI
uv run pytest                                    # test suite
uv run ruff check .                              # lint
uv run mypy python/blazeweb                      # typecheck

# Benchmarks — gauntlet lives in tests/ as `@pytest.mark.benchmark`, skipped
# by default. Run it with -m benchmark -s (no output capture so the per-phase
# timing tables print live).
uv run pytest -m benchmark -s

# Cross-tool comparison benchmarks (pulls Playwright + its Chromium)
uv sync --group bench
uv run playwright install chromium

# Build a release wheel (bundles chrome-headless-shell for all target platforms)
uv run blazeweb-download-chrome --all
uv build
```

Rust edits to `src/*.rs` trigger a rebuild on the next `uv sync` / `uv run`
automatically — no manual `maturin develop` needed. This is driven by
`[tool.uv] cache-keys` in `pyproject.toml`, which tracks `Cargo.toml`,
`Cargo.lock`, `rust-toolchain.toml`, and `src/**/*.rs`.

Supported platforms for the bundled binary: linux-x86_64, linux-aarch64,
darwin-x86_64, darwin-aarch64, windows-x86_64. (v2.0 ships linux-x86_64;
others in follow-ups.)

## License

blazeweb is Apache-2.0 or MIT (your choice). The bundled
`chrome-headless-shell` is BSD-3-Clause (Google Chrome for Testing).
