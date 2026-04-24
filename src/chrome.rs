//! Chrome binary resolver.
//!
//! Priority:
//!   1. explicit `chrome_path=` arg on Client (or `BLAZEWEB_CHROME__PATH` env →
//!      pydantic loads → reaches us here as `explicit`)
//!   2. bundled `python/blazeweb/_binaries/<platform>/chrome-headless-shell`
//!      (relative to the loaded `_blazeweb` module location)
//!   3. system common paths: /usr/bin/chromium-browser, /usr/bin/chromium,
//!      /usr/bin/google-chrome-stable, /usr/bin/google-chrome
//!   4. PATH lookup: chromium-browser, chromium, chrome
//!   5. error

use std::path::{Path, PathBuf};

use crate::error::{BlazeError, Result};

/// Platform identifier used to pick the right bundled subdir.
pub fn platform_subdir() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "linux_x86_64";
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return "linux_aarch64";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "darwin_x86_64";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "darwin_aarch64";
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return "windows_x86_64";
    // Fallback
    #[allow(unreachable_code)]
    "unknown"
}

/// Canonical binary filename per platform.
pub fn chrome_binary_name() -> &'static str {
    #[cfg(target_os = "windows")]
    return "chrome-headless-shell.exe";
    #[allow(unreachable_code)]
    "chrome-headless-shell"
}

/// Resolve chrome binary path. `explicit` wins, then bundled, then system.
/// Env-based config (`BLAZEWEB_CHROME__PATH=...`) reaches us as `explicit`
/// via pydantic-settings → ClientConfig.chrome.path; no separate env lookup here.
pub fn resolve(explicit: Option<&str>) -> Result<PathBuf> {
    // 1. explicit arg (or env-injected via pydantic)
    if let Some(p) = explicit {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
        return Err(BlazeError::ChromeNotFound(format!(
            "explicit chrome_path {p:?} not a file"
        )));
    }

    // 2. bundled — relative to this crate's shared library. In maturin wheels
    //    the installed layout has `python/blazeweb/_binaries/<platform>/...`
    //    next to the `_blazeweb.so` (or after install: under site-packages).
    if let Some(bundled) = find_bundled() {
        return Ok(bundled);
    }

    // 3. system common paths
    for candidate in &[
        "/usr/bin/chromium-browser",
        "/usr/bin/chromium",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/google-chrome",
        "/usr/local/bin/chromium",
        "/usr/local/bin/chrome",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ] {
        let p = Path::new(candidate);
        if p.is_file() {
            return Ok(p.to_path_buf());
        }
    }

    // 4. PATH lookup
    for name in &["chromium-browser", "chromium", "google-chrome", "chrome"] {
        if let Ok(p) = which_on_path(name) {
            return Ok(p);
        }
    }

    Err(BlazeError::ChromeNotFound(
        "no chrome binary found in arg/env/bundled/system/PATH. \
         Pass chrome_path=, set BLAZEWEB_CHROME, or install chromium."
            .to_string(),
    ))
}

/// Look for `python/blazeweb/_binaries/<platform>/<binary>` relative to well-known
/// locations: the installed package dir (via env `BLAZEWEB_PKG_DIR` set at Python
/// import if needed), or walking up from CARGO_MANIFEST_DIR for dev builds.
fn find_bundled() -> Option<PathBuf> {
    let plat = platform_subdir();
    let bin = chrome_binary_name();
    let rel = format!("_binaries/{plat}/{bin}");

    // If the Python loader set BLAZEWEB_PKG_DIR to the installed package path,
    // check there first. (See python/blazeweb/__init__.py for where this is set.)
    if let Ok(pkg) = std::env::var("BLAZEWEB_PKG_DIR") {
        let p = Path::new(&pkg).join(&rel);
        if p.is_file() {
            return Some(p);
        }
    }

    // Dev build fallback — walk up from CARGO_MANIFEST_DIR looking for
    // `python/blazeweb/_binaries/<plat>/<bin>`.
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = Path::new(&manifest_dir)
            .join("python/blazeweb")
            .join(&rel);
        if p.is_file() {
            return Some(p);
        }
    }

    None
}

fn which_on_path(name: &str) -> Result<PathBuf> {
    let path = std::env::var("PATH").map_err(|_| {
        BlazeError::ChromeNotFound("PATH env not set".to_string())
    })?;
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(BlazeError::ChromeNotFound(format!("{name} not on PATH")))
}
