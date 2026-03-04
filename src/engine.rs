use crate::dom;
use crate::error::EngineError;
use crate::js;
use crate::net::fetch::FetchContext;
use crate::net::request::Request;

/// Render result containing HTML output and any JS errors encountered.
pub struct RenderOutput {
    pub html: String,
    pub errors: Vec<String>,
}

/// Top-level render function: parse HTML → execute JS → serialize.
///
/// Creates a stateless FetchContext (no persistent cache or cookies).
pub fn render(html: &[u8], base_url: Option<&str>) -> Result<RenderOutput, EngineError> {
    let context = FetchContext::new(base_url);
    render_with_context(html, base_url, &context)
}

/// Render with a caller-supplied FetchContext (e.g. from a Client with persistent cache).
pub fn render_with_context(
    html: &[u8],
    base_url: Option<&str>,
    context: &FetchContext,
) -> Result<RenderOutput, EngineError> {
    render_inner(html, base_url, context)
}

/// Fetch a URL and render it: fetch document → parse → execute JS → serialize.
///
/// Uses the final URL after redirects as the `base_url` for resource resolution.
/// Creates a stateless FetchContext.
pub fn fetch(url: &str) -> Result<RenderOutput, EngineError> {
    let context = FetchContext::with_cookies_and_cache(Some(url));
    fetch_with_context(url, &context)
}

/// Fetch a URL and render with a caller-supplied FetchContext.
pub fn fetch_with_context(url: &str, context: &FetchContext) -> Result<RenderOutput, EngineError> {
    let parsed = reqwest::Url::parse(url).map_err(|e| EngineError::Network {
        url: url.into(),
        reason: format!("invalid URL: {e}"),
    })?;
    let mut request = Request::document(parsed);
    let response = crate::net::fetch::fetch(&mut request, context);

    if response.is_network_error() {
        return Err(EngineError::Network {
            url: url.into(),
            reason: response.status_text,
        });
    }
    if !response.ok() {
        return Err(EngineError::Network {
            url: url.into(),
            reason: format!("HTTP {}", response.status),
        });
    }

    let final_url = response
        .final_url()
        .map(|u| u.as_str().to_owned())
        .unwrap_or_else(|| url.to_string());
    let html = response.text();

    // Use the same context for the render (shares cache/cookies with the document fetch)
    let mut ctx = context.clone();
    ctx.base_url = Some(final_url.clone());
    render_inner(html.as_bytes(), Some(&final_url), &ctx)
}

fn render_inner(
    html: &[u8],
    base_url: Option<&str>,
    context: &FetchContext,
) -> Result<RenderOutput, EngineError> {
    let t0 = std::time::Instant::now();
    let url_label = base_url.unwrap_or("<inline>");

    // Step 1: Parse HTML into Arena
    let html_str = std::str::from_utf8(html)
        .map_err(|e| EngineError::Parse(format!("invalid UTF-8: {e}")))?;
    log::info!("[{}] parsing {} bytes of HTML", url_label, html.len());
    let mut arena = dom::parse_document(html_str);
    log::debug!("[{}] parsed in {:?}", url_label, t0.elapsed());

    // Step 2: Execute scripts (skips V8 init if no scripts found)
    let js_errors = js::runtime::execute_scripts(&mut arena, base_url, context)?;

    // Step 3: Resolve CSS styles via Stylo (after scripts may have mutated DOM)
    crate::css::resolve::resolve_styles(&arena);

    // Step 4: Serialize back to HTML
    let ser_start = std::time::Instant::now();
    let output = dom::serialize(&arena);
    log::debug!("[{}] serialized {} bytes in {:?}", url_label, output.len(), ser_start.elapsed());
    log::info!(
        "[{}] render complete in {:?} ({} JS errors, {} bytes output)",
        url_label, t0.elapsed(), js_errors.len(), output.len(),
    );

    Ok(RenderOutput {
        html: output,
        errors: js_errors,
    })
}
