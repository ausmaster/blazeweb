"""Command-line interface.

  python -m blazeweb <URL>                              # HTML → stdout
  python -m blazeweb <URL> -o page.html                 # HTML → file, stdout suppressed
  python -m blazeweb <URL> -s shot.png                  # HTML → stdout, screenshot → file
  python -m blazeweb <URL> -o page.html -s shot.jpg     # HTML → file, screenshot → file
  python -m blazeweb <URL> --screenshot-only shot.webp  # image-only, HTML suppressed
  python -m blazeweb <URL> --json                       # single-line JSON → stdout (all metadata)

Image format is inferred from the output file extension (``.jpg`` / ``.jpeg``
 → jpeg, ``.webp`` → webp, anything else → png). Override with ``--format``.
Quality (0-100) applies to jpeg / webp only.

Common knobs (all optional):
  --user-agent STR        # override default UA
  --width N --height N    # viewport (default 1200×800)
  --timeout-ms MS         # per-URL navigation cap (default 30000)
  --locale LOCALE         # e.g. en-US, ja-JP
  --timezone TZ           # e.g. America/New_York
  --proxy URL             # http:// or socks5://
  --header KEY=VALUE      # extra HTTP headers (repeatable)
  --headers-file FILE     # newline-separated KEY=VALUE headers
  --no-js                 # disable JS execution (compare with requests/httpx)
  --ignore-certs          # --ignore-certificate-errors
  --chrome PATH           # override bundled Chrome binary
  --format FMT            # png|jpeg|webp (default: inferred from extension)
  --quality N             # 0-100 for jpeg/webp
  --full-page             # full scrollable height for screenshot
  --version               # print blazeweb version and exit

Exit codes:
  0 success, 1 bad arg, 2 fetch error (with message on stderr)
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import blazeweb


def _parse_header(s: str) -> tuple[str, str]:
    if ":" in s:
        k, _, v = s.partition(":")
    elif "=" in s:
        k, _, v = s.partition("=")
    else:
        raise argparse.ArgumentTypeError(
            f"--header must be KEY=VALUE or KEY:VALUE, got {s!r}"
        )
    return k.strip(), v.strip()


def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="python -m blazeweb",
        description="Fetch a URL, return fully-rendered HTML (post-JS) and/or a screenshot.",
    )
    p.add_argument("url", nargs="?", help="URL to fetch")
    p.add_argument("--version", action="store_true", help="print version and exit")

    out = p.add_argument_group("output")
    out.add_argument(
        "--output", "-o", metavar="PATH",
        help="write HTML to PATH (stdout suppressed). Use '-' to force stdout.",
    )
    out.add_argument(
        "--screenshot", "-s", metavar="PATH",
        help="also capture a PNG screenshot to PATH",
    )
    out.add_argument(
        "--screenshot-only", metavar="PATH",
        help="capture screenshot to PATH; suppress HTML (image-only mode)",
    )
    out.add_argument(
        "--json", action="store_true",
        help="emit a single JSON object {html, errors, final_url, status_code, elapsed_s}",
    )
    out.add_argument(
        "--meta", action="store_true",
        help="print metadata (final_url, status_code, elapsed_s) to stderr",
    )

    cfg = p.add_argument_group("config")
    cfg.add_argument("--user-agent", "-A", help="override User-Agent")
    cfg.add_argument("--width", type=int, default=1200)
    cfg.add_argument("--height", type=int, default=800)
    cfg.add_argument("--timeout-ms", type=int, default=30000, help="per-URL navigation cap")
    cfg.add_argument("--locale", help="e.g. en-US, ja-JP")
    cfg.add_argument("--timezone", help="e.g. America/New_York")
    cfg.add_argument("--proxy", help="e.g. http://host:8080 or socks5://host:1080")
    cfg.add_argument(
        "--header", "-H", action="append", default=[], type=_parse_header,
        metavar="K=V",
        help="extra HTTP header (repeatable). Also accepts 'K: V'.",
    )
    cfg.add_argument(
        "--headers-file", metavar="PATH",
        help="file of KEY=VALUE headers, one per line (blank / '#'-prefixed lines ignored)",
    )
    cfg.add_argument("--no-js", action="store_true", help="disable JavaScript execution")
    cfg.add_argument(
        "--ignore-certs", action="store_true",
        help="ignore HTTPS cert errors (--ignore-certificate-errors)",
    )
    cfg.add_argument("--chrome", metavar="PATH", help="override Chrome binary path")
    cfg.add_argument(
        "--full-page", action="store_true",
        help="capture the full scrollable height for --screenshot / --screenshot-only",
    )
    cfg.add_argument(
        "--format", choices=["png", "jpeg", "webp"], default=None,
        help="screenshot image format (default: inferred from output file extension, else png)",
    )
    cfg.add_argument(
        "--quality", type=int, default=None, metavar="N",
        help="jpeg/webp quality 0-100 (ignored for png)",
    )
    return p


def _infer_format(path: str | None, explicit: str | None) -> str:
    if explicit:
        return explicit
    if path:
        ext = Path(path).suffix.lower()
        if ext in (".jpg", ".jpeg"):
            return "jpeg"
        if ext == ".webp":
            return "webp"
    return "png"


def _read_headers_file(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for ln in path.read_text().splitlines():
        ln = ln.strip()
        if not ln or ln.startswith("#"):
            continue
        k, v = _parse_header(ln)
        out[k] = v
    return out


def _build_client_kwargs(args: argparse.Namespace) -> dict:
    kwargs: dict = {
        "viewport": (args.width, args.height),
        "navigation_timeout_ms": args.timeout_ms,
    }
    if args.user_agent:
        kwargs["user_agent"] = args.user_agent
    if args.locale:
        kwargs["locale"] = args.locale
    if args.timezone:
        kwargs["timezone"] = args.timezone
    if args.proxy:
        kwargs["proxy"] = args.proxy
    if args.no_js:
        kwargs["javascript_enabled"] = False
    if args.ignore_certs:
        kwargs["ignore_https_errors"] = True
    if args.chrome:
        kwargs["chrome_path"] = args.chrome

    headers: dict[str, str] = {}
    if args.headers_file:
        headers.update(_read_headers_file(Path(args.headers_file)))
    for k, v in args.header:
        headers[k] = v
    if headers:
        kwargs["extra_headers"] = headers
    return kwargs


def _emit_meta(html_result, stream=sys.stderr) -> None:
    stream.write(
        f"final_url={html_result.final_url}  "
        f"status={html_result.status_code}  "
        f"elapsed={html_result.elapsed_s:.3f}s  "
        f"errors={len(html_result.errors)}\n"
    )


def main(argv: list[str] | None = None) -> int:
    p = _build_parser()
    args = p.parse_args(argv)

    if args.version:
        try:
            from importlib.metadata import version
            print(version("blazeweb"))
        except Exception:
            print("unknown")
        return 0

    if not args.url:
        p.error("URL is required (see --help)")

    if args.screenshot_only and (args.output or args.screenshot):
        p.error("--screenshot-only is mutually exclusive with --output / --screenshot")
    if args.screenshot_only and args.json:
        p.error("--screenshot-only and --json are mutually exclusive")
    if args.quality is not None and not 0 <= args.quality <= 100:
        p.error("--quality must be between 0 and 100")

    client_kwargs = _build_client_kwargs(args)
    want_shot = bool(args.screenshot or args.screenshot_only)
    shot_path = args.screenshot or args.screenshot_only
    img_format = _infer_format(shot_path, args.format)

    # HTML destination: file (--output PATH), stdout ('-' or unset), or suppressed (--screenshot-only)
    html_to_file: Path | None = None
    html_to_stdout = True
    if args.screenshot_only:
        html_to_stdout = False
    elif args.output and args.output != "-":
        html_to_file = Path(args.output)
        html_to_stdout = False

    def _emit_html(text: str) -> None:
        if html_to_file is not None:
            html_to_file.write_text(text)
        elif html_to_stdout:
            sys.stdout.write(text)
            if not text.endswith("\n"):
                sys.stdout.write("\n")

    try:
        with blazeweb.Client(**client_kwargs) as client:
            if want_shot:
                result = client.fetch_all(
                    args.url,
                    full_page=args.full_page,
                    format=img_format,
                    quality=args.quality,
                )
                Path(shot_path).write_bytes(result.png)
                html_result = result.html
                if args.json:
                    out = {
                        "url": args.url,
                        "final_url": result.final_url,
                        "status_code": result.status_code,
                        "elapsed_s": result.elapsed_s,
                        "errors": result.errors,
                        "html": str(result.html),
                        "image_path": str(Path(shot_path).resolve()),
                        "image_format": img_format,
                        "image_bytes": len(result.png),
                    }
                    sys.stdout.write(json.dumps(out) + "\n")
                else:
                    _emit_html(str(result.html))
            else:
                result = client.fetch(args.url)
                html_result = result
                if args.json:
                    out = {
                        "url": args.url,
                        "final_url": result.final_url,
                        "status_code": result.status_code,
                        "elapsed_s": result.elapsed_s,
                        "errors": result.errors,
                        "html": str(result),
                    }
                    sys.stdout.write(json.dumps(out) + "\n")
                else:
                    _emit_html(str(result))

            if args.meta and not args.json:
                _emit_meta(html_result)

    except RuntimeError as e:
        sys.stderr.write(f"blazeweb: {e}\n")
        return 2
    except KeyboardInterrupt:
        sys.stderr.write("blazeweb: interrupted\n")
        return 130

    return 0


if __name__ == "__main__":
    sys.exit(main())
