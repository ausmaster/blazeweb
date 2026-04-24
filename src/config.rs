//! Rust-side mirror of the Python pydantic config hierarchy.
//!
//! Python side owns validation via pydantic. We accept a plain dict (the
//! `.model_dump()` output) across FFI and parse it here into typed Rust structs.
//! One place converts Python → Rust; nowhere else. This keeps Rust free of
//! pydantic and Python free of Rust's view.

use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyTuple};

use crate::error::{BlazeError, Result};

// ----------------------------------------------------------------------------
// Top-level Client config
// ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ClientConfigRs {
    pub concurrency: usize,
    pub viewport: ViewportRs,
    pub network: NetworkRs,
    pub emulation: EmulationRs,
    pub timeout: TimeoutRs,
    pub chrome: ChromeRs,
}

#[derive(Debug, Clone)]
pub struct ViewportRs {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: f64,
    pub mobile: bool,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkRs {
    pub user_agent: Option<String>,
    pub proxy: Option<String>,
    pub extra_headers: HashMap<String, String>,
    pub ignore_https_errors: bool,
    pub block_urls: Vec<String>,
    pub disable_cache: bool,
    pub offline: bool,
    pub latency_ms: Option<f64>,
    pub download_bps: Option<u64>,
    pub upload_bps: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct EmulationRs {
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub geolocation: Option<(f64, f64)>,
    pub prefers_color_scheme: Option<String>, // "light" | "dark"
    pub javascript_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct TimeoutRs {
    pub navigation_ms: u64,
    pub launch_ms: u64,
    pub screenshot_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ChromeRs {
    pub path: Option<String>,
    pub args: Vec<String>,
    pub user_data_dir: Option<String>,
    pub headless: bool,
}

impl Default for ViewportRs {
    fn default() -> Self {
        Self { width: 1200, height: 800, device_scale_factor: 1.0, mobile: false }
    }
}

impl Default for TimeoutRs {
    fn default() -> Self {
        Self { navigation_ms: 30_000, launch_ms: 15_000, screenshot_ms: 5_000 }
    }
}

impl Default for ClientConfigRs {
    fn default() -> Self {
        Self {
            concurrency: 16,
            viewport: ViewportRs::default(),
            network: NetworkRs::default(),
            emulation: EmulationRs { javascript_enabled: true, ..Default::default() },
            timeout: TimeoutRs::default(),
            chrome: ChromeRs { headless: true, ..Default::default() },
        }
    }
}

// ----------------------------------------------------------------------------
// Per-call overrides
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct FetchConfigRs {
    pub extra_headers: HashMap<String, String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct ScreenshotConfigRs {
    pub viewport: Option<(u32, u32)>,
    pub full_page: bool,
    pub timeout_ms: Option<u64>,
    pub extra_headers: HashMap<String, String>,
}

// ----------------------------------------------------------------------------
// Python → Rust conversion
// ----------------------------------------------------------------------------

/// Parse a Python dict (from pydantic's `.model_dump()`) into ClientConfigRs.
pub fn parse_client_config(py_dict: &Bound<'_, PyAny>) -> Result<ClientConfigRs> {
    let mut cfg = ClientConfigRs::default();

    if let Some(d) = as_dict(py_dict)? {
        if let Some(v) = d.get_item("concurrency")? {
            cfg.concurrency = v.extract().map_err(to_internal)?;
        }
        if let Some(v) = d.get_item("viewport")? {
            cfg.viewport = parse_viewport(&v)?;
        }
        if let Some(v) = d.get_item("network")? {
            cfg.network = parse_network(&v)?;
        }
        if let Some(v) = d.get_item("emulation")? {
            cfg.emulation = parse_emulation(&v)?;
        }
        if let Some(v) = d.get_item("timeout")? {
            cfg.timeout = parse_timeout(&v)?;
        }
        if let Some(v) = d.get_item("chrome")? {
            cfg.chrome = parse_chrome(&v)?;
        }
    }
    Ok(cfg)
}

pub fn parse_fetch_config(py_dict: &Bound<'_, PyAny>) -> Result<FetchConfigRs> {
    let mut cfg = FetchConfigRs::default();
    if let Some(d) = as_dict(py_dict)? {
        if let Some(v) = d.get_item("extra_headers")? {
            cfg.extra_headers = parse_headers(&v)?;
        }
        if let Some(v) = d.get_item("timeout_ms")? {
            if !v.is_none() {
                cfg.timeout_ms = Some(v.extract().map_err(to_internal)?);
            }
        }
    }
    Ok(cfg)
}

pub fn parse_screenshot_config(py_dict: &Bound<'_, PyAny>) -> Result<ScreenshotConfigRs> {
    let mut cfg = ScreenshotConfigRs::default();
    if let Some(d) = as_dict(py_dict)? {
        if let Some(v) = d.get_item("viewport")? {
            if !v.is_none() {
                cfg.viewport = Some(parse_pair(&v)?);
            }
        }
        if let Some(v) = d.get_item("full_page")? {
            cfg.full_page = v.extract().map_err(to_internal)?;
        }
        if let Some(v) = d.get_item("timeout_ms")? {
            if !v.is_none() {
                cfg.timeout_ms = Some(v.extract().map_err(to_internal)?);
            }
        }
        if let Some(v) = d.get_item("extra_headers")? {
            cfg.extra_headers = parse_headers(&v)?;
        }
    }
    Ok(cfg)
}

// --- helpers ---------------------------------------------------------------

fn as_dict<'py>(v: &Bound<'py, PyAny>) -> Result<Option<Bound<'py, PyDict>>> {
    if v.is_none() {
        return Ok(None);
    }
    v.downcast::<PyDict>()
        .map(|d| Some(d.clone()))
        .map_err(|_| BlazeError::InvalidConfig("expected dict".to_string()))
}

fn to_internal(e: PyErr) -> BlazeError {
    BlazeError::InvalidConfig(e.to_string())
}

fn parse_viewport(v: &Bound<'_, PyAny>) -> Result<ViewportRs> {
    let mut out = ViewportRs::default();
    if let Some(d) = as_dict(v)? {
        if let Some(x) = d.get_item("width")? { out.width = x.extract().map_err(to_internal)?; }
        if let Some(x) = d.get_item("height")? { out.height = x.extract().map_err(to_internal)?; }
        if let Some(x) = d.get_item("device_scale_factor")? {
            out.device_scale_factor = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("mobile")? { out.mobile = x.extract().map_err(to_internal)?; }
    }
    Ok(out)
}

fn parse_network(v: &Bound<'_, PyAny>) -> Result<NetworkRs> {
    let mut out = NetworkRs::default();
    if let Some(d) = as_dict(v)? {
        if let Some(x) = d.get_item("user_agent")? {
            if !x.is_none() { out.user_agent = Some(x.extract().map_err(to_internal)?); }
        }
        if let Some(x) = d.get_item("proxy")? {
            if !x.is_none() { out.proxy = Some(x.extract().map_err(to_internal)?); }
        }
        if let Some(x) = d.get_item("extra_headers")? {
            out.extra_headers = parse_headers(&x)?;
        }
        if let Some(x) = d.get_item("ignore_https_errors")? {
            out.ignore_https_errors = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("block_urls")? {
            out.block_urls = parse_str_list(&x)?;
        }
        if let Some(x) = d.get_item("disable_cache")? {
            out.disable_cache = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("offline")? {
            out.offline = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("latency_ms")? {
            if !x.is_none() { out.latency_ms = Some(x.extract().map_err(to_internal)?); }
        }
        if let Some(x) = d.get_item("download_bps")? {
            if !x.is_none() { out.download_bps = Some(x.extract().map_err(to_internal)?); }
        }
        if let Some(x) = d.get_item("upload_bps")? {
            if !x.is_none() { out.upload_bps = Some(x.extract().map_err(to_internal)?); }
        }
    }
    Ok(out)
}

fn parse_emulation(v: &Bound<'_, PyAny>) -> Result<EmulationRs> {
    let mut out = EmulationRs { javascript_enabled: true, ..Default::default() };
    if let Some(d) = as_dict(v)? {
        if let Some(x) = d.get_item("locale")? {
            if !x.is_none() { out.locale = Some(x.extract().map_err(to_internal)?); }
        }
        if let Some(x) = d.get_item("timezone")? {
            if !x.is_none() { out.timezone = Some(x.extract().map_err(to_internal)?); }
        }
        if let Some(x) = d.get_item("geolocation")? {
            if !x.is_none() { out.geolocation = Some(parse_pair_f64(&x)?); }
        }
        if let Some(x) = d.get_item("prefers_color_scheme")? {
            if !x.is_none() {
                out.prefers_color_scheme = Some(x.extract().map_err(to_internal)?);
            }
        }
        if let Some(x) = d.get_item("javascript_enabled")? {
            out.javascript_enabled = x.extract().map_err(to_internal)?;
        }
    }
    Ok(out)
}

fn parse_timeout(v: &Bound<'_, PyAny>) -> Result<TimeoutRs> {
    let mut out = TimeoutRs::default();
    if let Some(d) = as_dict(v)? {
        if let Some(x) = d.get_item("navigation_ms")? {
            out.navigation_ms = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("launch_ms")? {
            out.launch_ms = x.extract().map_err(to_internal)?;
        }
        if let Some(x) = d.get_item("screenshot_ms")? {
            out.screenshot_ms = x.extract().map_err(to_internal)?;
        }
    }
    Ok(out)
}

fn parse_chrome(v: &Bound<'_, PyAny>) -> Result<ChromeRs> {
    let mut out = ChromeRs { headless: true, ..Default::default() };
    if let Some(d) = as_dict(v)? {
        if let Some(x) = d.get_item("path")? {
            if !x.is_none() { out.path = Some(x.extract().map_err(to_internal)?); }
        }
        if let Some(x) = d.get_item("args")? {
            out.args = parse_str_list(&x)?;
        }
        if let Some(x) = d.get_item("user_data_dir")? {
            if !x.is_none() {
                out.user_data_dir = Some(x.extract().map_err(to_internal)?);
            }
        }
        if let Some(x) = d.get_item("headless")? {
            out.headless = x.extract().map_err(to_internal)?;
        }
    }
    Ok(out)
}

fn parse_headers(v: &Bound<'_, PyAny>) -> Result<HashMap<String, String>> {
    if v.is_none() {
        return Ok(HashMap::new());
    }
    let d = v.downcast::<PyDict>().map_err(|_| {
        BlazeError::InvalidConfig("extra_headers must be dict".to_string())
    })?;
    let mut out = HashMap::with_capacity(d.len());
    for (k, val) in d.iter() {
        let key: String = k.extract().map_err(to_internal)?;
        let v: String = val.extract().map_err(to_internal)?;
        out.insert(key, v);
    }
    Ok(out)
}

fn parse_str_list(v: &Bound<'_, PyAny>) -> Result<Vec<String>> {
    if v.is_none() { return Ok(Vec::new()); }
    let lst = v.downcast::<PyList>().map_err(|_| {
        BlazeError::InvalidConfig("expected list of strings".to_string())
    })?;
    lst.iter()
        .map(|item| item.extract::<String>().map_err(to_internal))
        .collect()
}

fn parse_pair(v: &Bound<'_, PyAny>) -> Result<(u32, u32)> {
    let tuple = v.downcast::<PyTuple>().map_err(|_| {
        BlazeError::InvalidConfig("expected (w, h) tuple".to_string())
    })?;
    if tuple.len() != 2 {
        return Err(BlazeError::InvalidConfig("tuple must be length 2".to_string()));
    }
    let w: u32 = tuple.get_item(0).map_err(to_internal)?.extract().map_err(to_internal)?;
    let h: u32 = tuple.get_item(1).map_err(to_internal)?.extract().map_err(to_internal)?;
    Ok((w, h))
}

fn parse_pair_f64(v: &Bound<'_, PyAny>) -> Result<(f64, f64)> {
    let tuple = v.downcast::<PyTuple>().map_err(|_| {
        BlazeError::InvalidConfig("expected (a, b) tuple".to_string())
    })?;
    if tuple.len() != 2 {
        return Err(BlazeError::InvalidConfig("tuple must be length 2".to_string()));
    }
    let a: f64 = tuple.get_item(0).map_err(to_internal)?.extract().map_err(to_internal)?;
    let b: f64 = tuple.get_item(1).map_err(to_internal)?.extract().map_err(to_internal)?;
    Ok((a, b))
}
