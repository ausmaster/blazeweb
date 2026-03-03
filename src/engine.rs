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

    // Step 3: Serialize back to HTML
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

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    /// Render HTML+JS and extract #result textContent from the output.
    fn run_js(js: &str) -> String {
        let html = format!(
            r#"<!DOCTYPE html><html><head></head><body>
<div id="result"></div>
<script>
try {{
    {js}
}} catch(e) {{
    document.getElementById('result').textContent = 'ERROR:' + e.name + ':' + e.message;
}}
</script></body></html>"#
        );
        let output = render(html.as_bytes(), None).expect("render failed");
        let re = Regex::new(r#"<div id="result">(.*?)</div>"#).unwrap();
        re.captures(&output.html)
            .map(|c| c.get(1).unwrap().as_str().to_string())
            .unwrap_or_default()
    }

    /// Render with a pre-built DOM tree (3 spans inside #tree).
    fn run_tree(js: &str) -> String {
        let html = format!(
            r#"<!DOCTYPE html><html><head></head><body>
<div id="result"></div>
<div id="tree"><span id="a">A</span><span id="b">B</span><span id="c">C</span></div>
<script>
try {{
    {js}
}} catch(e) {{
    document.getElementById('result').textContent = 'ERROR:' + e.name + ':' + e.message;
}}
</script></body></html>"#
        );
        let output = render(html.as_bytes(), None).expect("render failed");
        let re = Regex::new(r#"<div id="result">(.*?)</div>"#).unwrap();
        re.captures(&output.html)
            .map(|c| c.get(1).unwrap().as_str().to_string())
            .unwrap_or_default()
    }

    // ─── importNode tests ────────────────────────────────────

    #[test]
    fn import_node_deep_preserves_children() {
        let r = run_js(
            "var div = document.createElement('div');
             div.appendChild(document.createElement('span'));
             div.appendChild(document.createElement('p'));
             var clone = document.importNode(div, true);
             document.getElementById('result').textContent = clone.childNodes.length;",
        );
        assert_eq!(r, "2");
    }

    #[test]
    fn import_node_deep_preserves_attributes() {
        let r = run_js(
            "var div = document.createElement('div');
             div.id = 'original';
             div.className = 'foo bar';
             div.setAttribute('data-x', '42');
             var clone = document.importNode(div, true);
             document.getElementById('result').textContent = [
                 clone.id, clone.className, clone.getAttribute('data-x')
             ].join(',');",
        );
        assert_eq!(r, "original,foo bar,42");
    }

    #[test]
    fn import_node_deep_is_different_node() {
        let r = run_js(
            "var div = document.createElement('div');
             var clone = document.importNode(div, true);
             document.getElementById('result').textContent = String(div === clone);",
        );
        assert_eq!(r, "false");
    }

    #[test]
    fn import_node_deep_nested() {
        let r = run_js(
            "var div = document.createElement('div');
             var inner = document.createElement('span');
             inner.textContent = 'hello';
             div.appendChild(inner);
             var clone = document.importNode(div, true);
             document.getElementById('result').textContent = clone.firstChild.textContent;",
        );
        assert_eq!(r, "hello");
    }

    #[test]
    fn import_node_deep_preserves_text_nodes() {
        let r = run_js(
            "var p = document.createElement('p');
             p.textContent = 'test text';
             var clone = document.importNode(p, true);
             document.getElementById('result').textContent = clone.textContent;",
        );
        assert_eq!(r, "test text");
    }

    #[test]
    fn import_node_shallow_no_children() {
        let r = run_js(
            "var div = document.createElement('div');
             div.appendChild(document.createElement('span'));
             var clone = document.importNode(div, false);
             document.getElementById('result').textContent = clone.childNodes.length;",
        );
        assert_eq!(r, "0");
    }

    #[test]
    fn import_node_shallow_preserves_tag() {
        let r = run_js(
            "var span = document.createElement('span');
             var clone = document.importNode(span, false);
             document.getElementById('result').textContent = clone.tagName;",
        );
        assert_eq!(r, "SPAN");
    }

    #[test]
    fn import_node_default_is_shallow() {
        let r = run_js(
            "var div = document.createElement('div');
             div.appendChild(document.createElement('span'));
             var clone = document.importNode(div);
             document.getElementById('result').textContent = clone.childNodes.length;",
        );
        assert_eq!(r, "0");
    }

    #[test]
    fn import_node_shallow_preserves_attributes() {
        let r = run_js(
            "var div = document.createElement('div');
             div.id = 'test';
             div.setAttribute('data-val', 'abc');
             var clone = document.importNode(div, false);
             document.getElementById('result').textContent = [
                 clone.id, clone.getAttribute('data-val')
             ].join(',');",
        );
        assert_eq!(r, "test,abc");
    }

    #[test]
    fn import_node_document_throws() {
        let r = run_js(
            "try {
                 document.importNode(document, true);
                 document.getElementById('result').textContent = 'no error';
             } catch(e) {
                 document.getElementById('result').textContent = 'threw';
             }",
        );
        assert_eq!(r, "threw");
    }

    #[test]
    fn import_node_text_node() {
        let r = run_js(
            "var t = document.createTextNode('hello');
             var clone = document.importNode(t, true);
             document.getElementById('result').textContent = clone.textContent;",
        );
        assert_eq!(r, "hello");
    }

    #[test]
    fn import_node_comment_node() {
        let r = run_js(
            "var c = document.createComment('test comment');
             var clone = document.importNode(c, true);
             document.getElementById('result').textContent = clone.nodeType + ':' + clone.textContent;",
        );
        assert_eq!(r, "8:test comment");
    }

    #[test]
    fn import_node_document_fragment() {
        let r = run_js(
            "var frag = document.createDocumentFragment();
             frag.appendChild(document.createElement('div'));
             frag.appendChild(document.createElement('span'));
             var clone = document.importNode(frag, true);
             document.getElementById('result').textContent = [
                 clone.nodeType,
                 clone.childNodes.length
             ].join(',');",
        );
        assert_eq!(r, "11,2");
    }

    // ─── adoptNode tests ─────────────────────────────────────

    #[test]
    fn adopt_node_removes_from_parent() {
        let r = run_js(
            "var parent = document.createElement('div');
             var child = document.createElement('span');
             parent.appendChild(child);
             document.adoptNode(child);
             document.getElementById('result').textContent = [
                 parent.childNodes.length,
                 child.parentNode === null
             ].join(',');",
        );
        assert_eq!(r, "0,true");
    }

    #[test]
    fn adopt_node_returns_same_node() {
        let r = run_js(
            "var div = document.createElement('div');
             var adopted = document.adoptNode(div);
             document.getElementById('result').textContent = String(div === adopted);",
        );
        assert_eq!(r, "true");
    }

    #[test]
    fn adopt_node_document_throws() {
        let r = run_js(
            "try {
                 document.adoptNode(document);
                 document.getElementById('result').textContent = 'no error';
             } catch(e) {
                 document.getElementById('result').textContent = 'threw';
             }",
        );
        assert_eq!(r, "threw");
    }

    #[test]
    fn adopt_node_orphan_works() {
        let r = run_js(
            "var div = document.createElement('div');
             var adopted = document.adoptNode(div);
             document.getElementById('result').textContent = [
                 adopted.parentNode === null,
                 adopted === div
             ].join(',');",
        );
        assert_eq!(r, "true,true");
    }

    #[test]
    fn adopt_node_disconnects() {
        let html = r#"<!DOCTYPE html><html><head></head><body>
<div id="result"></div>
<div id="target"><span id="child">text</span></div>
<script>
var child = document.getElementById('child');
var wasBefore = child.isConnected;
document.adoptNode(child);
var isAfter = child.isConnected;
document.getElementById('result').textContent = wasBefore + ',' + isAfter;
</script></body></html>"#;
        let output = render(html.as_bytes(), None).unwrap();
        let re = Regex::new(r#"<div id="result">(.*?)</div>"#).unwrap();
        let r = re.captures(&output.html).unwrap().get(1).unwrap().as_str();
        assert_eq!(r, "true,false");
    }

    #[test]
    fn adopt_node_can_reinsert() {
        let r = run_js(
            "var src = document.createElement('div');
             var child = document.createElement('span');
             child.textContent = 'moved';
             src.appendChild(child);
             document.adoptNode(child);
             var dest = document.createElement('div');
             dest.appendChild(child);
             document.getElementById('result').textContent = [
                 src.childNodes.length,
                 dest.firstChild.textContent
             ].join(',');",
        );
        assert_eq!(r, "0,moved");
    }

    // ─── TreeWalker: nextNode ────────────────────────────────

    #[test]
    fn tw_next_node_elements() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var ids = [];
             var n;
             while (n = tw.nextNode()) ids.push(n.id);
             document.getElementById('result').textContent = ids.join(',');",
        );
        assert_eq!(r, "a,b,c");
    }

    #[test]
    fn tw_next_node_text() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_TEXT);
             var texts = [];
             var n;
             while (n = tw.nextNode()) texts.push(n.textContent);
             document.getElementById('result').textContent = texts.join(',');",
        );
        assert_eq!(r, "A,B,C");
    }

    #[test]
    fn tw_next_node_show_all() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ALL);
             var count = 0;
             while (tw.nextNode()) count++;
             document.getElementById('result').textContent = count;",
        );
        assert_eq!(r, "6"); // 3 spans + 3 text nodes
    }

    #[test]
    fn tw_next_node_bounded_by_root() {
        let r = run_tree(
            "var span = document.getElementById('a');
             var tw = document.createTreeWalker(span, NodeFilter.SHOW_ALL);
             var count = 0;
             while (tw.nextNode()) count++;
             document.getElementById('result').textContent = count;",
        );
        assert_eq!(r, "1"); // Only text "A"
    }

    #[test]
    fn tw_next_node_returns_null_at_end() {
        let r = run_tree(
            "var span = document.getElementById('a');
             var tw = document.createTreeWalker(span, NodeFilter.SHOW_ELEMENT);
             var n = tw.nextNode();
             document.getElementById('result').textContent = String(n);",
        );
        assert_eq!(r, "null");
    }

    #[test]
    fn tw_next_node_deep_tree() {
        let r = run_js(
            "var div = document.createElement('div'); div.id = 'root';
             var a = document.createElement('div'); a.id = 'a';
             var b = document.createElement('div'); b.id = 'b';
             var c = document.createElement('div'); c.id = 'c';
             a.appendChild(b);
             b.appendChild(c);
             div.appendChild(a);
             document.body.appendChild(div);
             var tw = document.createTreeWalker(div, NodeFilter.SHOW_ELEMENT);
             var ids = [];
             var n;
             while (n = tw.nextNode()) ids.push(n.id);
             document.getElementById('result').textContent = ids.join(',');",
        );
        assert_eq!(r, "a,b,c");
    }

    // ─── TreeWalker: firstChild / lastChild ──────────────────

    #[test]
    fn tw_first_child_basic() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var fc = tw.firstChild();
             document.getElementById('result').textContent = fc ? fc.id : 'null';",
        );
        assert_eq!(r, "a");
    }

    #[test]
    fn tw_first_child_updates_current() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             tw.firstChild();
             document.getElementById('result').textContent = tw.currentNode.id;",
        );
        assert_eq!(r, "a");
    }

    #[test]
    fn tw_first_child_with_text_filter() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_TEXT);
             var fc = tw.firstChild();
             document.getElementById('result').textContent = fc ? fc.textContent : 'null';",
        );
        assert_eq!(r, "A");
    }

    #[test]
    fn tw_first_child_no_children_returns_null() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('a'), NodeFilter.SHOW_ELEMENT);
             var fc = tw.firstChild();
             document.getElementById('result').textContent = String(fc);",
        );
        assert_eq!(r, "null");
    }

    #[test]
    fn tw_last_child_basic() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var lc = tw.lastChild();
             document.getElementById('result').textContent = lc ? lc.id : 'null';",
        );
        assert_eq!(r, "c");
    }

    #[test]
    fn tw_last_child_no_children_returns_null() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('c'), NodeFilter.SHOW_ELEMENT);
             var lc = tw.lastChild();
             document.getElementById('result').textContent = String(lc);",
        );
        assert_eq!(r, "null");
    }

    // ─── TreeWalker: parentNode ──────────────────────────────

    #[test]
    fn tw_parent_node_basic() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             tw.firstChild();
             var pn = tw.parentNode();
             document.getElementById('result').textContent = pn ? pn.id : 'null';",
        );
        assert_eq!(r, "tree");
    }

    #[test]
    fn tw_parent_node_at_root_returns_null() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var pn = tw.parentNode();
             document.getElementById('result').textContent = String(pn);",
        );
        assert_eq!(r, "null");
    }

    // ─── TreeWalker: nextSibling / previousSibling ───────────

    #[test]
    fn tw_next_sibling() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             tw.firstChild();
             var ns = tw.nextSibling();
             document.getElementById('result').textContent = ns ? ns.id : 'null';",
        );
        assert_eq!(r, "b");
    }

    #[test]
    fn tw_previous_sibling() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             tw.lastChild();
             var ps = tw.previousSibling();
             document.getElementById('result').textContent = ps ? ps.id : 'null';",
        );
        assert_eq!(r, "b");
    }

    #[test]
    fn tw_next_sibling_at_end_returns_null() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             tw.lastChild();
             var ns = tw.nextSibling();
             document.getElementById('result').textContent = String(ns);",
        );
        assert_eq!(r, "null");
    }

    #[test]
    fn tw_previous_sibling_at_start_returns_null() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             tw.firstChild();
             var ps = tw.previousSibling();
             document.getElementById('result').textContent = String(ps);",
        );
        assert_eq!(r, "null");
    }

    #[test]
    fn tw_sibling_chain() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             tw.firstChild();
             var ids = [tw.currentNode.id];
             while (tw.nextSibling()) ids.push(tw.currentNode.id);
             document.getElementById('result').textContent = ids.join(',');",
        );
        assert_eq!(r, "a,b,c");
    }

    // ─── TreeWalker: previousNode ────────────────────────────

    #[test]
    fn tw_previous_node() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             tw.lastChild();
             var pn = tw.previousNode();
             document.getElementById('result').textContent = pn ? pn.id : 'null';",
        );
        assert_eq!(r, "b");
    }

    #[test]
    fn tw_previous_node_at_root_returns_null() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var pn = tw.previousNode();
             document.getElementById('result').textContent = String(pn);",
        );
        assert_eq!(r, "null");
    }

    #[test]
    fn tw_previous_node_full_reverse() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             while (tw.nextNode()) {}
             var ids = [];
             var n;
             while (n = tw.previousNode()) { if (n.id) ids.push(n.id); }
             document.getElementById('result').textContent = ids.join(',');",
        );
        assert_eq!(r, "b,a,tree");
    }

    // ─── TreeWalker: NodeFilter ──────────────────────────────

    #[test]
    fn tw_filter_function_accept() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT, function(n) {
                 return n.id === 'b' ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
             });
             var n = tw.nextNode();
             document.getElementById('result').textContent = n ? n.id : 'null';",
        );
        assert_eq!(r, "b");
    }

    #[test]
    fn tw_filter_object_with_accept_node() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT, {
                 acceptNode: function(n) {
                     return n.id === 'c' ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
                 }
             });
             var n = tw.nextNode();
             document.getElementById('result').textContent = n ? n.id : 'null';",
        );
        assert_eq!(r, "c");
    }

    #[test]
    fn tw_filter_reject_skips_subtree() {
        let r = run_js(
            "var div = document.createElement('div');
             var a = document.createElement('span'); a.id = 'a';
             var a1 = document.createElement('em'); a1.id = 'a1';
             a.appendChild(a1);
             var b = document.createElement('span'); b.id = 'b';
             div.appendChild(a);
             div.appendChild(b);
             document.body.appendChild(div);
             var tw = document.createTreeWalker(div, NodeFilter.SHOW_ELEMENT, function(n) {
                 if (n.id === 'a') return NodeFilter.FILTER_REJECT;
                 return NodeFilter.FILTER_ACCEPT;
             });
             var ids = [];
             var n;
             while (n = tw.nextNode()) ids.push(n.id);
             document.getElementById('result').textContent = ids.join(',');",
        );
        assert_eq!(r, "b");
    }

    #[test]
    fn tw_filter_skip_enters_subtree() {
        let r = run_js(
            "var div = document.createElement('div');
             var a = document.createElement('span'); a.id = 'a';
             var a1 = document.createElement('em'); a1.id = 'a1';
             a.appendChild(a1);
             var b = document.createElement('span'); b.id = 'b';
             div.appendChild(a);
             div.appendChild(b);
             document.body.appendChild(div);
             var tw = document.createTreeWalker(div, NodeFilter.SHOW_ELEMENT, function(n) {
                 if (n.id === 'a') return NodeFilter.FILTER_SKIP;
                 return NodeFilter.FILTER_ACCEPT;
             });
             var ids = [];
             var n;
             while (n = tw.nextNode()) ids.push(n.id);
             document.getElementById('result').textContent = ids.join(',');",
        );
        assert_eq!(r, "a1,b");
    }

    #[test]
    fn tw_filter_reject_in_first_child() {
        let r = run_js(
            "var div = document.createElement('div'); div.id = 'root';
             var a = document.createElement('div'); a.id = 'a';
             var a1 = document.createElement('span'); a1.id = 'a1';
             a.appendChild(a1);
             var b = document.createElement('div'); b.id = 'b';
             div.appendChild(a);
             div.appendChild(b);
             document.body.appendChild(div);
             var tw = document.createTreeWalker(div, NodeFilter.SHOW_ELEMENT, function(n) {
                 if (n.id === 'a') return NodeFilter.FILTER_REJECT;
                 return NodeFilter.FILTER_ACCEPT;
             });
             var fc = tw.firstChild();
             document.getElementById('result').textContent = fc ? fc.id : 'null';",
        );
        assert_eq!(r, "b");
    }

    #[test]
    fn tw_filter_skip_in_first_child_descends() {
        let r = run_js(
            "var div = document.createElement('div'); div.id = 'root';
             var a = document.createElement('div'); a.id = 'a';
             var a1 = document.createElement('span'); a1.id = 'a1';
             a.appendChild(a1);
             var b = document.createElement('div'); b.id = 'b';
             div.appendChild(a);
             div.appendChild(b);
             document.body.appendChild(div);
             var tw = document.createTreeWalker(div, NodeFilter.SHOW_ELEMENT, function(n) {
                 if (n.id === 'a') return NodeFilter.FILTER_SKIP;
                 return NodeFilter.FILTER_ACCEPT;
             });
             var fc = tw.firstChild();
             document.getElementById('result').textContent = fc ? fc.id : 'null';",
        );
        assert_eq!(r, "a1");
    }

    // ─── TreeWalker: currentNode ─────────────────────────────

    #[test]
    fn tw_current_node_settable() {
        let r = run_tree(
            "var tree = document.getElementById('tree');
             var tw = document.createTreeWalker(tree, NodeFilter.SHOW_ELEMENT);
             tw.currentNode = document.getElementById('b');
             var ns = tw.nextSibling();
             document.getElementById('result').textContent = ns ? ns.id : 'null';",
        );
        assert_eq!(r, "c");
    }

    #[test]
    fn tw_current_node_initially_root() {
        let r = run_tree(
            "var tree = document.getElementById('tree');
             var tw = document.createTreeWalker(tree, NodeFilter.SHOW_ELEMENT);
             document.getElementById('result').textContent = (tw.currentNode === tree) + '';",
        );
        assert_eq!(r, "true");
    }

    #[test]
    fn tw_root_property() {
        let r = run_tree(
            "var tree = document.getElementById('tree');
             var tw = document.createTreeWalker(tree, NodeFilter.SHOW_ELEMENT);
             document.getElementById('result').textContent = (tw.root === tree) + '';",
        );
        assert_eq!(r, "true");
    }

    #[test]
    fn tw_default_show_all() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'));
             var count = 0;
             while (tw.nextNode()) count++;
             document.getElementById('result').textContent = count;",
        );
        assert_eq!(r, "6"); // 3 spans + 3 text nodes
    }

    // ─── TreeWalker: integration round-trip ──────────────────

    #[test]
    fn tw_forward_then_backward_roundtrip() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var forward = [];
             var n;
             while (n = tw.nextNode()) forward.push(n.id);
             var backward = [];
             while (n = tw.previousNode()) backward.push(n.id);
             document.getElementById('result').textContent = forward.join(',') + '|' + backward.join(',');",
        );
        assert_eq!(r, "a,b,c|b,a,tree");
    }

    #[test]
    fn tw_first_child_then_next_sibling_chain() {
        let r = run_tree(
            "var tw = document.createTreeWalker(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var ids = [];
             var n = tw.firstChild();
             while (n) { ids.push(n.id); n = tw.nextSibling(); }
             document.getElementById('result').textContent = ids.join(',');",
        );
        assert_eq!(r, "a,b,c");
    }

    // ─── NodeFilter constants ────────────────────────────────

    #[test]
    fn node_filter_constants_exist() {
        let r = run_js(
            "document.getElementById('result').textContent = [
                 NodeFilter.FILTER_ACCEPT === 1,
                 NodeFilter.FILTER_REJECT === 2,
                 NodeFilter.FILTER_SKIP === 3,
                 NodeFilter.SHOW_ALL === 0xFFFFFFFF,
                 NodeFilter.SHOW_ELEMENT === 0x1,
                 NodeFilter.SHOW_TEXT === 0x4,
                 NodeFilter.SHOW_COMMENT === 0x80,
                 NodeFilter.SHOW_DOCUMENT === 0x100,
             ].join(',');",
        );
        assert_eq!(r, "true,true,true,true,true,true,true,true");
    }

    // ─── NodeIterator tests ──────────────────────────────────

    // NOTE: Per WHATWG DOM §6.1, NodeIterator's iterator collection is
    // "the inclusive descendants of root, in tree order" — root IS included.
    // The first nextNode() returns root itself (if it passes the filter).

    #[test]
    fn ni_next_node_basic() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var ids = [];
             var n;
             while (n = ni.nextNode()) ids.push(n.id);
             document.getElementById('result').textContent = ids.join(',');",
        );
        // Root (div#tree) is an element, so it's included first
        assert_eq!(r, "tree,a,b,c");
    }

    #[test]
    fn ni_next_node_text_only() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_TEXT);
             var texts = [];
             var n;
             while (n = ni.nextNode()) texts.push(n.textContent);
             document.getElementById('result').textContent = texts.join(',');",
        );
        // Root is an element, filtered out by SHOW_TEXT → only text nodes
        assert_eq!(r, "A,B,C");
    }

    #[test]
    fn ni_next_node_show_all() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ALL);
             var count = 0;
             while (ni.nextNode()) count++;
             document.getElementById('result').textContent = count;",
        );
        // root + 3 spans + 3 text nodes = 7
        assert_eq!(r, "7");
    }

    #[test]
    fn ni_next_node_bounded_by_root() {
        let r = run_tree(
            "var span = document.getElementById('a');
             var ni = document.createNodeIterator(span, NodeFilter.SHOW_ALL);
             var count = 0;
             while (ni.nextNode()) count++;
             document.getElementById('result').textContent = count;",
        );
        // root span#a + text "A" = 2
        assert_eq!(r, "2");
    }

    #[test]
    fn ni_previous_node_after_next() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             ni.nextNode(); // tree (root)
             ni.nextNode(); // a
             ni.nextNode(); // b
             var pn = ni.previousNode();
             document.getElementById('result').textContent = pn ? pn.id : 'null';",
        );
        // After 3 nextNode calls (tree, a, b), previousNode returns b
        assert_eq!(r, "b");
    }

    #[test]
    fn ni_previous_node_returns_null_at_start() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var pn = ni.previousNode();
             document.getElementById('result').textContent = String(pn);",
        );
        assert_eq!(r, "null");
    }

    #[test]
    fn ni_next_then_prev_roundtrip() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var first = ni.nextNode(); // tree (root)
             var back = ni.previousNode(); // tree again
             document.getElementById('result').textContent = [first.id, back.id].join(',');",
        );
        // First nextNode returns root (tree), previousNode returns it again
        assert_eq!(r, "tree,tree");
    }

    #[test]
    fn ni_full_reverse_after_forward() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             var forward = [];
             var n;
             while (n = ni.nextNode()) forward.push(n.id);
             var backward = [];
             while (n = ni.previousNode()) backward.push(n.id);
             document.getElementById('result').textContent = forward.join(',') + '|' + backward.join(',');",
        );
        // Forward includes root; backward reverses through all including root
        assert_eq!(r, "tree,a,b,c|c,b,a,tree");
    }

    #[test]
    fn ni_reject_does_not_skip_subtree() {
        let r = run_js(
            "var div = document.createElement('div'); div.id = 'root';
             var a = document.createElement('span'); a.id = 'a';
             var a1 = document.createElement('em'); a1.id = 'a1';
             a.appendChild(a1);
             var b = document.createElement('span'); b.id = 'b';
             div.appendChild(a);
             div.appendChild(b);
             document.body.appendChild(div);
             var ni = document.createNodeIterator(div, NodeFilter.SHOW_ELEMENT, function(n) {
                 if (n.id === 'a') return NodeFilter.FILTER_REJECT;
                 return NodeFilter.FILTER_ACCEPT;
             });
             var ids = [];
             var n;
             while (n = ni.nextNode()) ids.push(n.id);
             document.getElementById('result').textContent = ids.join(',');",
        );
        // Root (div#root) accepted, span#a rejected (but a1 still visited), span#b accepted
        assert_eq!(r, "root,a1,b");
    }

    #[test]
    fn ni_reference_node() {
        let r = run_tree(
            "var root = document.getElementById('tree');
             var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
             ni.nextNode(); // returns root (tree)
             document.getElementById('result').textContent = ni.referenceNode ? ni.referenceNode.id : 'null';",
        );
        // First nextNode returns root itself
        assert_eq!(r, "tree");
    }

    #[test]
    fn ni_reference_node_initial() {
        let r = run_tree(
            "var root = document.getElementById('tree');
             var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
             document.getElementById('result').textContent = String(ni.referenceNode === root);",
        );
        assert_eq!(r, "true");
    }

    #[test]
    fn ni_pointer_before_reference_node_initial() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             document.getElementById('result').textContent = String(ni.pointerBeforeReferenceNode);",
        );
        assert_eq!(r, "true");
    }

    #[test]
    fn ni_pointer_before_after_next() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             ni.nextNode();
             document.getElementById('result').textContent = String(ni.pointerBeforeReferenceNode);",
        );
        assert_eq!(r, "false");
    }

    #[test]
    fn ni_root_property() {
        let r = run_tree(
            "var root = document.getElementById('tree');
             var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
             document.getElementById('result').textContent = String(ni.root === root);",
        );
        assert_eq!(r, "true");
    }

    #[test]
    fn ni_detach_is_noop() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT);
             ni.detach();
             var n = ni.nextNode();
             document.getElementById('result').textContent = n ? n.id : 'null';",
        );
        // detach is no-op, first nextNode returns root
        assert_eq!(r, "tree");
    }

    #[test]
    fn ni_default_show_all() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'));
             var count = 0;
             while (ni.nextNode()) count++;
             document.getElementById('result').textContent = count;",
        );
        // root + 3 spans + 3 text nodes = 7
        assert_eq!(r, "7");
    }

    #[test]
    fn ni_filter_function() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT, function(n) {
                 return n.id === 'b' ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
             });
             var ids = [];
             var n;
             while (n = ni.nextNode()) ids.push(n.id);
             document.getElementById('result').textContent = ids.join(',');",
        );
        assert_eq!(r, "b");
    }

    #[test]
    fn ni_filter_object() {
        let r = run_tree(
            "var ni = document.createNodeIterator(document.getElementById('tree'), NodeFilter.SHOW_ELEMENT, {
                 acceptNode: function(n) {
                     return n.id === 'c' ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
                 }
             });
             var n = ni.nextNode();
             document.getElementById('result').textContent = n ? n.id : 'null';",
        );
        assert_eq!(r, "c");
    }

    // ─── MutationObserver: constructor & lifecycle ─────────────

    #[test]
    fn mo_constructor_requires_function() {
        let r = run_js(
            "try {
                 new MutationObserver('not a function');
                 document.getElementById('result').textContent = 'no error';
             } catch(e) {
                 document.getElementById('result').textContent = e.name + ':' + (e.message.indexOf('not a function') >= 0);
             }",
        );
        assert_eq!(r, "TypeError:true");
    }

    #[test]
    fn mo_constructor_creates_object() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             document.getElementById('result').textContent = [
                 typeof mo,
                 typeof mo.observe,
                 typeof mo.disconnect,
                 typeof mo.takeRecords,
             ].join(',');",
        );
        assert_eq!(r, "object,function,function,function");
    }

    #[test]
    fn mo_options_validation_throws() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             try {
                 mo.observe(target, {});
                 document.getElementById('result').textContent = 'no error';
             } catch(e) {
                 document.getElementById('result').textContent = 'threw:' + e.name;
             }",
        );
        assert_eq!(r, "threw:TypeError");
    }

    #[test]
    fn mo_observe_requires_node() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             try {
                 mo.observe('not a node', { childList: true });
                 document.getElementById('result').textContent = 'no error';
             } catch(e) {
                 document.getElementById('result').textContent = 'threw:' + e.name;
             }",
        );
        assert_eq!(r, "threw:TypeError");
    }

    #[test]
    fn mo_take_records_empty_initially() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records.length;",
        );
        assert_eq!(r, "0");
    }

    // ─── MutationObserver: childList ──────────────────────────

    #[test]
    fn mo_child_list_append_child() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             var child = document.createElement('span');
             target.appendChild(child);
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].type,
                 records[0].addedNodes.length,
                 records[0].addedNodes[0].tagName,
                 records[0].removedNodes.length,
             ].join(',');",
        );
        assert_eq!(r, "1,childList,1,SPAN,0");
    }

    #[test]
    fn mo_child_list_remove_child() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             var child = document.createElement('span');
             target.appendChild(child);
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.removeChild(child);
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].type,
                 records[0].addedNodes.length,
                 records[0].removedNodes.length,
                 records[0].removedNodes[0].tagName,
             ].join(',');",
        );
        assert_eq!(r, "1,childList,0,1,SPAN");
    }

    #[test]
    fn mo_child_list_insert_before() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             var existing = document.createElement('p');
             target.appendChild(existing);
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             var inserted = document.createElement('span');
             target.insertBefore(inserted, existing);
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].addedNodes[0].tagName,
                 records[0].nextSibling.tagName,
             ].join(',');",
        );
        assert_eq!(r, "1,SPAN,P");
    }

    #[test]
    fn mo_child_list_replace_child() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             var old = document.createElement('span');
             target.appendChild(old);
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             var replacement = document.createElement('p');
             target.replaceChild(replacement, old);
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].addedNodes[0].tagName,
                 records[0].removedNodes[0].tagName,
             ].join(',');",
        );
        assert_eq!(r, "1,P,SPAN");
    }

    #[test]
    fn mo_child_list_target_is_correct() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.id = 'target';
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.appendChild(document.createElement('span'));
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records[0].target.id;",
        );
        assert_eq!(r, "target");
    }

    #[test]
    fn mo_child_list_siblings_captured() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             var a = document.createElement('span'); a.id = 'a';
             var b = document.createElement('span'); b.id = 'b';
             target.appendChild(a);
             target.appendChild(b);
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             var inserted = document.createElement('em');
             target.insertBefore(inserted, b);
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records[0].previousSibling ? records[0].previousSibling.id : 'null',
                 records[0].nextSibling ? records[0].nextSibling.id : 'null',
             ].join(',');",
        );
        assert_eq!(r, "a,b");
    }

    #[test]
    fn mo_child_list_subtree_observes_descendant() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             var child = document.createElement('div');
             target.appendChild(child);
             document.body.appendChild(target);
             mo.observe(target, { childList: true, subtree: true });
             child.appendChild(document.createElement('span'));
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].target === child,
             ].join(',');",
        );
        assert_eq!(r, "1,true");
    }

    #[test]
    fn mo_child_list_no_subtree_misses_descendant() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             var child = document.createElement('div');
             target.appendChild(child);
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             child.appendChild(document.createElement('span'));
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records.length;",
        );
        assert_eq!(r, "0");
    }

    #[test]
    fn mo_child_list_text_content_setter() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             var old = document.createElement('span');
             target.appendChild(old);
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.textContent = 'hello';
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].removedNodes.length,
                 records[0].removedNodes[0].tagName,
             ].join(',');",
        );
        assert_eq!(r, "1,1,SPAN");
    }

    // ─── MutationObserver: attributes ─────────────────────────

    #[test]
    fn mo_attributes_set_attribute() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { attributes: true });
             target.setAttribute('data-x', '42');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].type,
                 records[0].attributeName,
             ].join(',');",
        );
        assert_eq!(r, "1,attributes,data-x");
    }

    #[test]
    fn mo_attributes_remove_attribute() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.setAttribute('data-x', 'val');
             document.body.appendChild(target);
             mo.observe(target, { attributes: true });
             target.removeAttribute('data-x');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].type,
                 records[0].attributeName,
             ].join(',');",
        );
        assert_eq!(r, "1,attributes,data-x");
    }

    #[test]
    fn mo_attributes_old_value() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.setAttribute('data-x', 'old');
             document.body.appendChild(target);
             mo.observe(target, { attributes: true, attributeOldValue: true });
             target.setAttribute('data-x', 'new');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records[0].oldValue;",
        );
        assert_eq!(r, "old");
    }

    #[test]
    fn mo_attributes_old_value_null_when_not_requested() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.setAttribute('data-x', 'old');
             document.body.appendChild(target);
             mo.observe(target, { attributes: true });
             target.setAttribute('data-x', 'new');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = String(records[0].oldValue);",
        );
        assert_eq!(r, "null");
    }

    #[test]
    fn mo_attributes_filter_include() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { attributeFilter: ['data-x', 'data-y'] });
             target.setAttribute('data-x', '1');
             target.setAttribute('data-z', '2');
             target.setAttribute('data-y', '3');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records.map(function(r) { return r.attributeName; }).join('+'),
             ].join(',');",
        );
        assert_eq!(r, "2,data-x+data-y");
    }

    #[test]
    fn mo_attributes_class_name_setter() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { attributes: true });
             target.className = 'foo bar';
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].attributeName,
             ].join(',');",
        );
        assert_eq!(r, "1,class");
    }

    #[test]
    fn mo_attributes_id_setter() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { attributes: true });
             target.id = 'newid';
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].attributeName,
             ].join(',');",
        );
        assert_eq!(r, "1,id");
    }

    #[test]
    fn mo_attributes_implicit_from_old_value() {
        // Per spec: attributeOldValue implies attributes=true
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.setAttribute('x', 'old');
             document.body.appendChild(target);
             mo.observe(target, { attributeOldValue: true });
             target.setAttribute('x', 'new');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].oldValue,
             ].join(',');",
        );
        assert_eq!(r, "1,old");
    }

    #[test]
    fn mo_attributes_implicit_from_filter() {
        // Per spec: attributeFilter implies attributes=true
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { attributeFilter: ['data-x'] });
             target.setAttribute('data-x', '1');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records.length;",
        );
        assert_eq!(r, "1");
    }

    // ─── MutationObserver: characterData ──────────────────────

    #[test]
    fn mo_character_data_text_change() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.textContent = 'original';
             document.body.appendChild(target);
             var textNode = target.firstChild;
             mo.observe(textNode, { characterData: true });
             textNode.data = 'changed';
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].type,
             ].join(',');",
        );
        assert_eq!(r, "1,characterData");
    }

    #[test]
    fn mo_character_data_old_value() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.textContent = 'original';
             document.body.appendChild(target);
             var textNode = target.firstChild;
             mo.observe(textNode, { characterData: true, characterDataOldValue: true });
             textNode.data = 'changed';
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records[0].oldValue;",
        );
        assert_eq!(r, "original");
    }

    #[test]
    fn mo_character_data_append_data() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.textContent = 'hello';
             document.body.appendChild(target);
             var textNode = target.firstChild;
             mo.observe(textNode, { characterData: true, characterDataOldValue: true });
             textNode.appendData(' world');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].oldValue,
             ].join(',');",
        );
        assert_eq!(r, "1,hello");
    }

    #[test]
    fn mo_character_data_implicit_from_old_value() {
        // Per spec: characterDataOldValue implies characterData=true
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.textContent = 'old';
             document.body.appendChild(target);
             var textNode = target.firstChild;
             mo.observe(textNode, { characterDataOldValue: true });
             textNode.data = 'new';
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].oldValue,
             ].join(',');",
        );
        assert_eq!(r, "1,old");
    }

    #[test]
    fn mo_character_data_subtree() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.textContent = 'original';
             document.body.appendChild(target);
             mo.observe(target, { characterData: true, subtree: true });
             target.firstChild.data = 'changed';
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records.length;",
        );
        assert_eq!(r, "1");
    }

    // ─── MutationObserver: disconnect & takeRecords ───────────

    #[test]
    fn mo_disconnect_stops_observation() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             mo.disconnect();
             target.appendChild(document.createElement('span'));
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records.length;",
        );
        assert_eq!(r, "0");
    }

    #[test]
    fn mo_take_records_clears_queue() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.appendChild(document.createElement('span'));
             var first = mo.takeRecords();
             var second = mo.takeRecords();
             document.getElementById('result').textContent = first.length + ',' + second.length;",
        );
        assert_eq!(r, "1,0");
    }

    #[test]
    fn mo_disconnect_clears_pending() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.appendChild(document.createElement('span'));
             mo.disconnect();
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records.length;",
        );
        assert_eq!(r, "0");
    }

    // ─── MutationObserver: callback delivery via microtask ────

    #[test]
    fn mo_callback_fires_after_script() {
        // The MO callback fires during microtask checkpoint after script ends.
        // The callback sets #result — this happens before HTML serialization.
        let r = run_js(
            "var mo = new MutationObserver(function(records) {
                 document.getElementById('result').textContent = 'callback:' + records.length;
             });
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.appendChild(document.createElement('span'));",
        );
        assert_eq!(r, "callback:1");
    }

    #[test]
    fn mo_callback_receives_observer() {
        let r = run_js(
            "var myMo;
             myMo = new MutationObserver(function(records, observer) {
                 document.getElementById('result').textContent = (observer === myMo) + '';
             });
             var target = document.createElement('div');
             document.body.appendChild(target);
             myMo.observe(target, { childList: true });
             target.appendChild(document.createElement('span'));",
        );
        assert_eq!(r, "true");
    }

    #[test]
    fn mo_callback_records_have_correct_fields() {
        let r = run_js(
            "var mo = new MutationObserver(function(records) {
                 var r = records[0];
                 document.getElementById('result').textContent = [
                     r.type,
                     r.addedNodes.length,
                     r.removedNodes.length,
                     r.previousSibling === null,
                     r.nextSibling === null,
                     r.attributeName === null,
                     r.attributeNamespace === null,
                     r.oldValue === null,
                 ].join(',');
             });
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.appendChild(document.createElement('span'));",
        );
        assert_eq!(r, "childList,1,0,true,true,true,true,true");
    }

    #[test]
    fn mo_callback_batches_multiple_mutations() {
        let r = run_js(
            "var mo = new MutationObserver(function(records) {
                 document.getElementById('result').textContent = records.length;
             });
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.appendChild(document.createElement('span'));
             target.appendChild(document.createElement('p'));
             target.appendChild(document.createElement('em'));",
        );
        assert_eq!(r, "3");
    }

    // ─── MutationObserver: multiple observers ─────────────────

    #[test]
    fn mo_multiple_observers_same_target() {
        let r = run_js(
            "var results = [];
             var mo1 = new MutationObserver(function(records) {
                 results.push('mo1:' + records.length);
             });
             var mo2 = new MutationObserver(function(records) {
                 results.push('mo2:' + records.length);
                 document.getElementById('result').textContent = results.join(',');
             });
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo1.observe(target, { childList: true });
             mo2.observe(target, { childList: true });
             target.appendChild(document.createElement('span'));",
        );
        assert_eq!(r, "mo1:1,mo2:1");
    }

    #[test]
    fn mo_observe_replaces_existing_options() {
        // Per spec step 7: if observer already observes target, replace options
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             mo.observe(target, { attributes: true });
             target.appendChild(document.createElement('span'));
             target.setAttribute('x', '1');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].type,
             ].join(',');",
        );
        // childList was replaced by attributes-only
        assert_eq!(r, "1,attributes");
    }

    // ─── MutationObserver: innerHTML ──────────────────────────

    #[test]
    fn mo_inner_html_setter() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             target.innerHTML = '<span>old</span>';
             document.body.appendChild(target);
             mo.observe(target, { childList: true });
             target.innerHTML = '<p>new</p>';
             var records = mo.takeRecords();
             document.getElementById('result').textContent = [
                 records.length,
                 records[0].removedNodes.length,
                 records[0].removedNodes[0].tagName,
             ].join(',');",
        );
        assert_eq!(r, "1,1,SPAN");
    }

    // ─── MutationObserver: remove_attribute when not present ──

    #[test]
    fn mo_remove_nonexistent_attribute_no_record() {
        let r = run_js(
            "var mo = new MutationObserver(function() {});
             var target = document.createElement('div');
             document.body.appendChild(target);
             mo.observe(target, { attributes: true });
             target.removeAttribute('nonexistent');
             var records = mo.takeRecords();
             document.getElementById('result').textContent = records.length;",
        );
        assert_eq!(r, "0");
    }
}