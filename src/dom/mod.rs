pub mod arena;
pub mod node;
pub mod selector;
pub mod serialize;
pub mod treesink;

pub use arena::{Arena, NodeId};
#[allow(unused_imports)]
pub use node::NodeData;

/// Parse an HTML document string into an Arena.
pub fn parse_document(html: &str) -> Arena {
    treesink::parse(html)
}

/// Serialize an Arena back to an HTML string.
pub fn serialize(arena: &Arena) -> String {
    serialize::serialize_document(arena)
}
