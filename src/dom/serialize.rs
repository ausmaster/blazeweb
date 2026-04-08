use super::arena::{Arena, NodeId};
use super::node::NodeData;

/// Void elements that must not have a closing tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
];

/// Raw text elements whose children should not be entity-escaped.
const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style"];

/// Serialize the entire document arena to an HTML string.
pub fn serialize_document(arena: &Arena) -> String {
    let mut output = String::new();
    serialize_node(arena, arena.document, &mut output);
    output
}

/// Serialize a single node (and its children) to an HTML string.
/// Used by innerHTML/outerHTML getters in JS bindings.
pub fn serialize_node_to_string(arena: &Arena, id: NodeId, output: &mut String) {
    serialize_node(arena, id, output);
}

fn serialize_node(arena: &Arena, id: NodeId, output: &mut String) {
    let node = &arena.nodes[id];

    match &node.data {
        NodeData::Document | NodeData::DocumentFragment => {
            // Serialize children only
            for child in arena.children(id) {
                serialize_node(arena, child, output);
            }
        }
        NodeData::Doctype { name, .. } => {
            output.push_str("<!DOCTYPE ");
            output.push_str(name);
            output.push('>');
        }
        NodeData::Element(data) => {
            let tag = &*data.name.local;
            output.push('<');
            output.push_str(tag);

            for attr in &data.attrs {
                output.push(' ');
                // Handle prefixed attributes
                if let Some(ref prefix) = attr.name.prefix {
                    output.push_str(prefix);
                    output.push(':');
                }
                output.push_str(&attr.name.local);
                output.push_str("=\"");
                escape_attribute(&attr.value, output);
                output.push('"');
            }
            output.push('>');

            let is_void = VOID_ELEMENTS.contains(&tag);
            if !is_void {
                // <template> elements: serialize template_contents, not direct children
                if tag == "template" {
                    if let Some(content_id) = data.template_contents {
                        for child in arena.children(content_id) {
                            serialize_node(arena, child, output);
                        }
                    }
                } else {
                    // If element has a shadow root, serialize shadow content first
                    // (SSR: render shadow DOM inline for the composed output)
                    if let Some(shadow_id) = data.shadow_root {
                        for child in arena.children(shadow_id) {
                            serialize_node(arena, child, output);
                        }
                    }

                    let is_raw = RAW_TEXT_ELEMENTS.contains(&tag);
                    for child in arena.children(id) {
                        if is_raw {
                            serialize_raw_child(arena, child, output);
                        } else {
                            serialize_node(arena, child, output);
                        }
                    }
                }
                output.push_str("</");
                output.push_str(tag);
                output.push('>');
            }
        }
        NodeData::Text(text) => {
            escape_text(text, output);
        }
        NodeData::Comment(text) => {
            output.push_str("<!--");
            output.push_str(text);
            output.push_str("-->");
        }
    }
}

/// Serialize a child of a raw text element (no escaping).
fn serialize_raw_child(arena: &Arena, id: NodeId, output: &mut String) {
    match &arena.nodes[id].data {
        NodeData::Text(text) => output.push_str(text),
        _ => serialize_node(arena, id, output),
    }
}

fn escape_text(input: &str, output: &mut String) {
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(c),
        }
    }
}

fn escape_attribute(input: &str, output: &mut String) {
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(c),
        }
    }
}

#[cfg(test)]
#[path = "serialize_tests.rs"]
mod tests;

