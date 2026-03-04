//! CSS style resolution via the Stylo engine.
//!
//! This module integrates Mozilla's Stylo CSS engine to compute CSS styles per DOM element.
//! It bridges our SlotMap-based arena with Stylo's `TDocument`/`TNode`/`TElement` traits.

pub mod resolve;
pub mod stylo_bridge;
