"""Download a pinned ``chrome-headless-shell`` binary into
``python/blazeweb/_binaries/<platform>/`` so ``uv build`` / ``maturin build``
can include it in the wheel.

Exposed as the ``blazeweb-download-chrome`` console script:

    uv run blazeweb-download-chrome              # current platform
    uv run blazeweb-download-chrome --all        # every supported platform
    uv run blazeweb-download-chrome --force      # re-download even if present

Idempotent: skips if the binary is already present and non-empty.
Versions are pinned here — bump ``CHROME_VERSION`` to upgrade the bundled
Chrome across all platforms.
"""

from __future__ import annotations

import argparse
import platform
import shutil
import stat
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path

# Pinned Chrome for Testing version. Bumping this pulls fresh binaries for every
# platform listed in PLATFORMS on the next download. Find current URLs at:
#   https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json
CHROME_VERSION = "148.0.7778.56"

# CDN base: https://storage.googleapis.com/chrome-for-testing-public/<version>/<platform>/chrome-headless-shell-<platform>.zip
CDN_BASE = "https://storage.googleapis.com/chrome-for-testing-public"

# (internal_key, cft_platform, zip_subdir, binary_filename)
#   internal_key → matches Rust `chrome::platform_subdir()` output
#   cft_platform → Chrome for Testing URL slug
#   zip_subdir   → directory inside the unzipped archive
#   binary_filename → the executable we care about
PLATFORMS: tuple[tuple[str, str, str, str], ...] = (
    ("linux_x86_64",   "linux64",   "chrome-headless-shell-linux64",   "chrome-headless-shell"),
    ("darwin_x86_64",  "mac-x64",   "chrome-headless-shell-mac-x64",   "chrome-headless-shell"),
    ("darwin_aarch64", "mac-arm64", "chrome-headless-shell-mac-arm64", "chrome-headless-shell"),
    ("windows_x86_64", "win64",     "chrome-headless-shell-win64",     "chrome-headless-shell.exe"),  # noqa: E501
)


def current_platform_key() -> str:
    """Return the internal_key matching the host OS+arch."""
    system = platform.system()
    machine = platform.machine().lower()
    if system == "Linux" and machine in {"x86_64", "amd64"}:
        return "linux_x86_64"
    if system == "Linux" and machine in {"aarch64", "arm64"}:
        return "linux_aarch64"
    if system == "Darwin" and machine == "x86_64":
        return "darwin_x86_64"
    if system == "Darwin" and machine in {"arm64", "aarch64"}:
        return "darwin_aarch64"
    if system == "Windows" and machine in {"amd64", "x86_64"}:
        return "windows_x86_64"
    raise RuntimeError(f"unsupported host platform: {system}/{machine}")


def download_for(
    internal_key: str,
    *,
    dest_root: Path,
    force: bool = False,
    verbose: bool = True,
) -> Path:
    """Download + extract chrome-headless-shell for ``internal_key``.
    Returns the path to the installed binary.
    """
    try:
        entry = next(p for p in PLATFORMS if p[0] == internal_key)
    except StopIteration as e:
        known = [p[0] for p in PLATFORMS]
        raise RuntimeError(
            f"no download config for platform {internal_key!r}. Known: {known}"
        ) from e
    _, cft_plat, zip_subdir, binary_name = entry

    dest_dir = dest_root / internal_key
    dest_bin = dest_dir / binary_name

    if dest_bin.is_file() and dest_bin.stat().st_size > 0 and not force:
        if verbose:
            print(
                f"  [{internal_key}] already present at {dest_bin} — skip "
                "(pass --force to re-download)."
            )
        return dest_bin

    url = f"{CDN_BASE}/{CHROME_VERSION}/{cft_plat}/chrome-headless-shell-{cft_plat}.zip"
    if verbose:
        print(f"  [{internal_key}] downloading {url}")

    # Stream to a tempfile so we don't buffer 100+MB in memory.
    with tempfile.NamedTemporaryFile(suffix=".zip", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    try:
        with urllib.request.urlopen(url) as resp, open(tmp_path, "wb") as out:
            shutil.copyfileobj(resp, out, length=1024 * 1024)

        if dest_dir.exists():
            shutil.rmtree(dest_dir)
        dest_dir.mkdir(parents=True, exist_ok=True)

        if verbose:
            print(f"  [{internal_key}] extracting...")

        with zipfile.ZipFile(tmp_path) as zf:
            # Archive layout: chrome-headless-shell-<cft_plat>/<files>
            # We flatten into dest_dir/, dropping the top-level dir.
            for member in zf.namelist():
                # Strip leading "<zip_subdir>/"
                rel = member
                if rel.startswith(zip_subdir + "/"):
                    rel = rel[len(zip_subdir) + 1:]
                if not rel or rel.endswith("/"):
                    continue
                target = dest_dir / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                with zf.open(member) as src, open(target, "wb") as dst:
                    shutil.copyfileobj(src, dst)
                # Preserve executable bit if it was set in the zip.
                info = zf.getinfo(member)
                mode = info.external_attr >> 16
                if mode & 0o111:
                    target.chmod(target.stat().st_mode | 0o755)

        # Always ensure the main binary is executable (some zips lose the bit).
        if dest_bin.is_file():
            dest_bin.chmod(dest_bin.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

        size_mb = dest_bin.stat().st_size / (1024 * 1024)
        if verbose:
            print(f"  [{internal_key}] ok — {dest_bin} ({size_mb:.0f} MB)")
        return dest_bin
    finally:
        tmp_path.unlink(missing_ok=True)


def default_dest_dir() -> Path:
    """Where install_chrome writes by default — next to this module inside
    the installed package, i.e. ``<site-packages>/blazeweb/_binaries/``."""
    return Path(__file__).resolve().parent / "_binaries"


def install_chrome(
    *,
    dest: Path | None = None,
    force: bool = False,
    platform_key: str | None = None,
    all_platforms: bool = False,
) -> int:
    """Fetch chrome-headless-shell. Returns a CLI-style exit code (0 = success).

    Shared by the ``blazeweb-download-chrome`` console-script entry point and
    the ``blazeweb --install`` CLI flag — kept callable (no argparse) so both
    can invoke it without fighting over ``sys.argv``.
    """
    dest = (dest or default_dest_dir()).resolve()
    dest.mkdir(parents=True, exist_ok=True)

    if platform_key:
        targets = [platform_key]
    elif all_platforms:
        targets = [p[0] for p in PLATFORMS]
    else:
        targets = [current_platform_key()]

    print(f"Chrome version: {CHROME_VERSION}")
    print(f"Destination:    {dest}")
    print(f"Platforms:      {targets}")
    for t in targets:
        download_for(t, dest_root=dest, force=force)

    print("done.")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--dest", default=None,
        help="Destination dir (default: <installed package>/_binaries)",
    )
    p.add_argument(
        "--all", action="store_true",
        help="Download for every supported platform, not just the current one",
    )
    p.add_argument(
        "--platform",
        help="Internal platform key to download (overrides --all)",
    )
    p.add_argument(
        "--force", action="store_true",
        help="Re-download even if binary is already present",
    )
    args = p.parse_args()

    return install_chrome(
        dest=Path(args.dest) if args.dest else None,
        force=args.force,
        platform_key=args.platform,
        all_platforms=args.all,
    )


if __name__ == "__main__":
    sys.exit(main())
