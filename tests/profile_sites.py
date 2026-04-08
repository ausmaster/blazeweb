"""Profile each gauntlet site individually to find bottlenecks."""
import time, sys, os, re, multiprocessing

# Must be at top level for fork to work
import blazeweb

def render_worker(html, base_url, out_path):
    """Render in a forked subprocess, write timing to file."""
    try:
        t = time.perf_counter()
        r = blazeweb.render(html, base_url=base_url)
        ms = (time.perf_counter() - t) * 1000
        tags = str(r).count("<")
        with open(out_path, "w") as f:
            f.write(f"{ms:.0f}|{tags}|ok")
    except Exception as e:
        ms = (time.perf_counter() - t) * 1000
        with open(out_path, "w") as f:
            f.write(f"{ms:.0f}|0|err:{str(e)[:50]}")

def main():
    from playwright.sync_api import sync_playwright

    # Extract site URLs from test file
    with open("tests/test_real_sites.py") as f:
        content = f.read()
    in_list = False
    site_urls = []
    for line in content.split("\n"):
        if "SITES" in line and "=" in line and "[" in line:
            in_list = True
        elif in_list and "]" in line:
            in_list = False
        elif in_list:
            m = re.search(r'"(https?://[^"]+)"', line)
            if m:
                site_urls.append(m.group(1))

    UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"

    print(f"Profiling {len(site_urls)} sites...")
    print(f"{'#':<3} {'Site':<35} {'Chrome':>8} {'Render':>8} {'Status'}")
    print("-" * 80)
    sys.stdout.flush()

    pw = sync_playwright().start()
    browser = pw.chromium.launch(headless=True)
    slow = []

    for i, url in enumerate(site_urls):
        site = url.split("//")[1][:33]

        # ── Chrome ──
        t0 = time.perf_counter()
        ctx = browser.new_context(user_agent=UA, viewport={"width": 1920, "height": 1080})
        page = ctx.new_page()
        raw_html = [None]
        def on_resp(r, raw=raw_html):
            if raw[0] is None and r.request.resource_type == "document":
                try: raw[0] = r.text()
                except: pass
        page.on("response", on_resp)
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=15000)
            chrome_html = page.content()
            chrome_ms = (time.perf_counter() - t0) * 1000
        except Exception as e:
            chrome_ms = (time.perf_counter() - t0) * 1000
            page.close(); ctx.close()
            print(f"{i+1:<3} {site:<35} {chrome_ms:>7.0f}ms {'---':>8} SKIP: {str(e)[:30]}")
            sys.stdout.flush()
            continue
        page.close(); ctx.close()
        source = raw_html[0] or chrome_html

        # ── Blazeweb (forked subprocess with timeout) ──
        import tempfile
        tmp = tempfile.NamedTemporaryFile(delete=False, suffix=".prof")
        tmp.close()

        ctx2 = multiprocessing.get_context("fork")
        proc = ctx2.Process(target=render_worker, args=(source, url, tmp.name))
        proc.start()
        proc.join(timeout=12)

        if proc.is_alive():
            proc.kill()
            proc.join()
            render_ms = 12000
            status = "**TIMEOUT**"
            slow.append((site, "RENDER_TIMEOUT"))
        else:
            try:
                with open(tmp.name) as f:
                    parts = f.read().split("|", 2)
                render_ms = float(parts[0])
                tags = int(parts[1])
                status = f"{tags} tags" if parts[2] == "ok" else parts[2]
            except:
                render_ms = 0
                status = "read_err"

        os.unlink(tmp.name)

        flag = ""
        if chrome_ms > 5000: flag = " << CHROME SLOW"
        elif render_ms > 5000: flag = " << RENDER SLOW"

        print(f"{i+1:<3} {site:<35} {chrome_ms:>7.0f}ms {render_ms:>7.0f}ms {status}{flag}")
        sys.stdout.flush()

        if chrome_ms > 5000 or render_ms > 5000:
            slow.append((site, f"chrome={chrome_ms:.0f} render={render_ms:.0f}"))

    browser.close()
    pw.stop()

    print(f"\n{'='*60}")
    if slow:
        print(f"BOTTLENECKS ({len(slow)}):")
        for s, r in slow:
            print(f"  {s}: {r}")
    else:
        print("No bottlenecks found (all sites < 5s)")

if __name__ == "__main__":
    main()
