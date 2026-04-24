//! Chrome binary resolver. Priority: explicit arg → bundled → system → PATH.

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

/// Resolve the chrome binary path. `explicit` is populated from both
/// `Client(chrome_path=...)` and `BLAZEWEB_CHROME__PATH` env (via pydantic).
pub fn resolve(explicit: Option<&str>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        let path = PathBuf::from(p);
        if path.is_file() {
            log::debug!(target: "blazeweb::chrome", "resolved from explicit arg: {p}");
            return Ok(path);
        }
        return Err(BlazeError::ChromeNotFound(format!(
            "explicit chrome_path {p:?} not a file"
        )));
    }

    if let Some(bundled) = find_bundled() {
        log::debug!(target: "blazeweb::chrome", "resolved from bundled: {}", bundled.display());
        return Ok(bundled);
    }

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
            log::debug!(target: "blazeweb::chrome", "resolved from system: {candidate}");
            return Ok(p.to_path_buf());
        }
    }

    for name in &["chromium-browser", "chromium", "google-chrome", "chrome"] {
        if let Ok(p) = which_on_path(name) {
            log::debug!(target: "blazeweb::chrome", "resolved from PATH: {}", p.display());
            return Ok(p);
        }
    }

    Err(BlazeError::ChromeNotFound(
        "no chrome binary found in arg/env/bundled/system/PATH. \
         Pass chrome_path=, set BLAZEWEB_CHROME__PATH, or install chromium."
            .to_string(),
    ))
}

/// Look for `_binaries/<platform>/<binary>` under the installed package dir
/// (`BLAZEWEB_PKG_DIR`, set by `python/blazeweb/__init__.py` at import) or —
/// for dev builds — under `CARGO_MANIFEST_DIR/python/blazeweb`.
fn find_bundled() -> Option<PathBuf> {
    let plat = platform_subdir();
    let bin = chrome_binary_name();
    let rel = format!("_binaries/{plat}/{bin}");

    if let Ok(pkg) = std::env::var("BLAZEWEB_PKG_DIR") {
        let p = Path::new(&pkg).join(&rel);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = Path::new(&manifest_dir).join("python/blazeweb").join(&rel);
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
