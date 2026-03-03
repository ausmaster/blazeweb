#!/usr/bin/env python3
"""Analyze missing JS APIs across real websites."""
import json, multiprocessing, os, re, sys, tempfile, time, urllib.request
from collections import Counter, defaultdict

SITES = [
    "https://apnews.com", "https://www.bbc.com", "https://www.wired.com",
    "https://www.tumblr.com", "https://www.substack.com", "https://www.ebay.com",
    "https://www.ibm.com", "https://aws.amazon.com", "https://www.spotify.com",
    "https://arstechnica.com", "https://www.who.int", "https://www.pinterest.com",
    "https://x.com", "https://www.booking.com", "https://www.amazon.com",
    "https://www.cloudflare.com", "https://www.reddit.com", "https://www.apple.com",
    "https://stackoverflow.com", "https://www.twitch.tv",
    "https://www.youtube.com", "https://www.khanacademy.org", "https://crates.io",
    "https://www.netflix.com", "https://www.microsoft.com", "https://www.google.com",
    "https://www.nvidia.com", "https://meta.com", "https://dev.to",
    "https://www.airbnb.com", "https://www.zoom.us",
    "https://news.ycombinator.com", "https://www.python.org", "https://pypi.org",
    "https://en.wikipedia.org/wiki/Rust_(programming_language)",
]

UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"

def fetch_html(url):
    try:
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        with urllib.request.urlopen(req, timeout=20) as resp:
            raw = resp.read()
            charset = resp.headers.get_content_charset("utf-8") or "utf-8"
            return raw.decode(charset, errors="replace"), None
    except Exception as e:
        return None, str(e)

def _worker(html, base_url, err_path):
    import blazeweb
    try:
        result = blazeweb.render(html, base_url=base_url)
        data = {"status": "ok", "errors": result.errors}
    except Exception as e:
        data = {"status": "error", "msg": str(e), "errors": []}
    with open(err_path, "w") as f:
        json.dump(data, f)

def render_isolated(html, base_url):
    with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
        err_path = f.name
    proc = multiprocessing.Process(target=_worker, args=(html, base_url, err_path))
    proc.start()
    proc.join(timeout=60)
    if proc.is_alive():
        proc.kill(); proc.join()
        try: os.unlink(err_path)
        except: pass
        return {"status": "timeout", "errors": []}
    if proc.exitcode != 0:
        try: os.unlink(err_path)
        except: pass
        return {"status": "crash", "exit_code": proc.exitcode, "errors": []}
    try:
        with open(err_path) as f:
            data = json.load(f)
        os.unlink(err_path)
        return data
    except:
        return {"status": "read_error", "errors": []}

def parse_error(msg):
    if msg.startswith("network error"):
        return "network_error", msg[:80]
    m = re.match(r"ReferenceError: (\S+) is not defined", msg)
    if m: return "reference_error", m.group(1)
    m = re.match(r"TypeError: (.+?) is not a function", msg)
    if m: return "type_not_function", m.group(1)
    m = re.match(r"TypeError: (.+?) is not a constructor", msg)
    if m: return "type_not_constructor", m.group(1)
    m = re.match(r"TypeError: Cannot read propert(?:y|ies) of (?:null|undefined) \(reading '(.+?)'\)", msg)
    if m: return "type_null_read", m.group(1)
    m = re.match(r"TypeError: Cannot set propert(?:y|ies) of (?:null|undefined)", msg)
    if m: return "type_null_set", "set property of null/undefined"
    if msg.startswith("SyntaxError:"): return "syntax_error", msg[:80]
    if msg.startswith("TypeError:"): return "type_other", msg[:100]
    return "other", msg[:100]

def label(url):
    return re.sub(r"https?://(www\.)?", "", url).rstrip("/")

def main():
    counters = defaultdict(Counter)
    sites_for = defaultdict(lambda: defaultdict(list))
    crashes = []

    for url in SITES:
        l = label(url)
        print(f"[{l}] Fetching...", end=" ", flush=True)
        html, err = fetch_html(url)
        if not html:
            print(f"FETCH FAIL: {err}")
            continue
        print(f"{len(html)//1024}KB. Rendering...", end=" ", flush=True)
        t0 = time.perf_counter()
        result = render_isolated(html, url)
        dt = time.perf_counter() - t0
        status = result.get("status", "?")
        errors = result.get("errors", [])
        if status in ("timeout", "crash"):
            print(f"CRASHED ({status})")
            crashes.append(l)
            continue
        print(f"{dt:.1f}s, {len(errors)} errors")
        for e in errors:
            cat, key = parse_error(e)
            counters[cat][key] += 1
            if l not in sites_for[cat][key]:
                sites_for[cat][key].append(l)

    def section(title, cat, n=50):
        if cat not in counters: return
        c = counters[cat]
        print(f"\n{'='*90}")
        print(f"  {title} ({sum(c.values())} total, {len(c)} unique)")
        print(f"{'='*90}")
        for key, count in c.most_common(n):
            sl = ", ".join(sites_for[cat][key][:6])
            if len(sites_for[cat][key]) > 6: sl += f" +{len(sites_for[cat][key])-6}"
            print(f"  {count:4d}x  {key:<55s}  [{sl}]")

    total = sum(sum(c.values()) for c in counters.values())
    print(f"\n\n{'#'*90}")
    print(f"  MISSING API REPORT — {len(SITES)} sites, {total} JS errors")
    print(f"{'#'*90}")
    section("MISSING GLOBALS (ReferenceError)", "reference_error")
    section("NOT A FUNCTION (TypeError)", "type_not_function")
    section("NOT A CONSTRUCTOR (TypeError)", "type_not_constructor")
    section("NULL/UNDEFINED READ", "type_null_read")
    section("NULL/UNDEFINED SET", "type_null_set")
    section("OTHER TYPE ERRORS", "type_other")
    section("SYNTAX ERRORS", "syntax_error")
    section("NETWORK ERRORS", "network_error")
    section("OTHER", "other")
    if crashes:
        print(f"\n  CRASHES: {', '.join(crashes)}")

if __name__ == "__main__":
    multiprocessing.set_start_method("fork", force=True)
    main()
