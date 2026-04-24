//! chromiumoxide_spike — parity-test against servo_spike batch mode.
//!
//! Reads URLs from stdin (one per line), writes PNGs to --out-dir, prints a
//! JSON line per URL on stdout (same shape as servo_spike).
//!
//! Parallelism via --concurrency N: a single Browser with N concurrent pages,
//! each task does navigate+screenshot+close and pushes its JSON line to stdout.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams,
};
use chromiumoxide::{Browser, BrowserConfig};
use clap::Parser;
use futures::StreamExt;
use tokio::io::AsyncBufReadExt;
use tokio::sync::Semaphore;

#[derive(Parser, Debug, Clone)]
#[command(about = "chromium-via-chromiumoxide batch screenshot + html tool")]
struct Args {
    /// Output directory for PNGs and HTML files.
    #[arg(long, default_value = "shots_oxide")]
    out_dir: PathBuf,

    /// Viewport width.
    #[arg(long, default_value = "1200")]
    width: u32,

    /// Viewport height.
    #[arg(long, default_value = "800")]
    height: u32,

    /// Per-URL timeout.
    #[arg(long, default_value = "30.0")]
    timeout_secs: f64,

    /// Concurrency (simultaneous page tabs).
    #[arg(long, default_value = "1")]
    concurrency: usize,

    /// Path to chromium binary.
    #[arg(long, default_value = "/usr/bin/chromium-browser")]
    chrome: PathBuf,

    /// What to capture per URL.
    #[arg(long, value_enum, default_value_t = CaptureMode::Png)]
    mode: CaptureMode,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureMode {
    /// PNG only.
    Png,
    /// Fully-rendered HTML only (document.documentElement.outerHTML after load).
    Html,
    /// Both, from one page visit (cheap because navigation dominates).
    Both,
}

fn sanitize(url: &str) -> String {
    url.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(96)
        .collect()
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    tokio::fs::create_dir_all(&args.out_dir).await?;

    // Read URLs from stdin upfront.
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut urls: Vec<String> = Vec::new();
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            urls.push(trimmed.to_string());
        }
    }
    if urls.is_empty() {
        eprintln!("no URLs on stdin");
        return Ok(());
    }

    let init_start = Instant::now();
    let cfg = BrowserConfig::builder()
        .chrome_executable(&args.chrome)
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-sandbox")
        .arg("--hide-scrollbars")
        .arg("--disable-dev-shm-usage")
        .arg(format!("--window-size={},{}", args.width, args.height))
        .build()
        .map_err(|e| anyhow::anyhow!("browser config: {e}"))?;

    let (browser, mut handler) = Browser::launch(cfg).await?;
    let handler_task = tokio::spawn(async move {
        while let Some(res) = handler.next().await {
            if let Err(e) = res {
                eprintln!("handler err: {e}");
                break;
            }
        }
    });
    let init_elapsed = init_start.elapsed().as_secs_f64();
    eprintln!(
        "chromium up in {:.2}s (N={}, conc={})",
        init_elapsed,
        urls.len(),
        args.concurrency
    );

    let sem = Arc::new(Semaphore::new(args.concurrency));
    let browser = Arc::new(browser);
    let stdout = Arc::new(tokio::sync::Mutex::new(tokio::io::stdout()));

    let batch_start = Instant::now();
    let mut handles = Vec::with_capacity(urls.len());
    for url in urls.iter().cloned() {
        let permit = sem.clone().acquire_owned().await?;
        let browser = browser.clone();
        let out_dir = args.out_dir.clone();
        let width = args.width;
        let height = args.height;
        let timeout_secs = args.timeout_secs;
        let stdout = stdout.clone();
        let mode = args.mode;

        let handle = tokio::spawn(async move {
            let _permit = permit;
            let t0 = Instant::now();
            let result = tokio::time::timeout(
                Duration::from_secs_f64(timeout_secs),
                capture(&browser, &url, &out_dir, width, height, mode),
            )
            .await;
            let elapsed_s = (t0.elapsed().as_secs_f64() * 10000.0).round() / 10000.0;
            let line = match result {
                Ok(Ok(artifacts)) => serde_json::json!({
                    "url": url,
                    "png": artifacts.png,
                    "html": artifacts.html,
                    "html_bytes": artifacts.html_bytes,
                    "ok": true,
                    "elapsed_s": elapsed_s,
                }),
                Ok(Err(e)) => serde_json::json!({
                    "url": url,
                    "ok": false,
                    "error": format!("{e}"),
                }),
                Err(_) => serde_json::json!({
                    "url": url,
                    "ok": false,
                    "error": "timeout",
                }),
            };
            use tokio::io::AsyncWriteExt;
            let text = format!("{}\n", line);
            let mut out = stdout.lock().await;
            let _ = out.write_all(text.as_bytes()).await;
            let _ = out.flush().await;
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    let batch_elapsed = batch_start.elapsed().as_secs_f64();
    eprintln!(
        "batch done in {:.2}s (init {:.2}s, {} urls)",
        batch_elapsed,
        init_elapsed,
        urls.len()
    );

    // Clean shutdown.
    drop(browser);
    let _ = tokio::time::timeout(Duration::from_secs(3), handler_task).await;
    Ok(())
}

#[derive(Debug, Default)]
struct Artifacts {
    png: Option<String>,
    html: Option<String>,
    html_bytes: Option<usize>,
}

async fn capture(
    browser: &Browser,
    url: &str,
    out_dir: &Path,
    width: u32,
    height: u32,
    mode: CaptureMode,
) -> anyhow::Result<Artifacts> {
    let page = browser.new_page(url).await?;
    page.execute(
        SetDeviceMetricsOverrideParams::builder()
            .width(width as i64)
            .height(height as i64)
            .device_scale_factor(1.0)
            .mobile(false)
            .build()
            .map_err(|e| anyhow::anyhow!("metrics: {e}"))?,
    )
    .await?;
    page.wait_for_navigation().await?;

    let name = sanitize(url);
    let mut artifacts = Artifacts::default();

    if matches!(mode, CaptureMode::Png | CaptureMode::Both) {
        let bytes = page
            .screenshot(
                CaptureScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await?;
        let out_path = out_dir.join(format!("{name}.png"));
        tokio::fs::write(&out_path, &bytes).await?;
        artifacts.png = Some(out_path.display().to_string());
    }

    if matches!(mode, CaptureMode::Html | CaptureMode::Both) {
        // page.content() returns document.documentElement.outerHTML — the
        // fully-rendered post-JS DOM serialized. This is what "render URL -> HTML"
        // means in CDP terms.
        let html = page.content().await?;
        let out_path = out_dir.join(format!("{name}.html"));
        tokio::fs::write(&out_path, &html).await?;
        artifacts.html = Some(out_path.display().to_string());
        artifacts.html_bytes = Some(html.len());
    }

    let _ = page.close().await;
    Ok(artifacts)
}
