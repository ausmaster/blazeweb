//! CSS style resolution and layout via Stylo + Taffy + Parley.
//!
//! This module integrates Mozilla's Stylo CSS engine, the Taffy layout engine,
//! and Parley text shaping to compute CSS styles and layout for every DOM element.

use std::cell::RefCell;

pub mod convert;
pub mod layout;
pub mod resolve;
pub mod stylo_bridge;
pub mod text;

// Shared Parley FontContext — expensive to create (system font discovery),
// so we create once per thread and reuse across style + layout passes.
thread_local! {
    pub(crate) static FONT_CTX: RefCell<parley::FontContext> =
        RefCell::new(parley::FontContext::new());
}
