use crate::dom;
use crate::error::EngineError;
use crate::js;

/// Top-level render function: parse HTML → execute JS → serialize.
pub fn render(html: &[u8], base_url: Option<&str>) -> Result<String, EngineError> {
    // Step 1: Parse HTML into Arena
    let html_str = std::str::from_utf8(html)
        .map_err(|e| EngineError::Parse(format!("invalid UTF-8: {e}")))?;

    let mut arena = dom::parse_document(html_str);

    // Step 2: Execute scripts (skips V8 init if no scripts found)
    let _js_errors = js::runtime::execute_scripts(&mut arena, base_url)?;

    // Step 3: Serialize back to HTML
    let output = dom::serialize(&arena);

    Ok(output)
}
