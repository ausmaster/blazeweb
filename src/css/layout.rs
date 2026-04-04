//! Taffy box layout — computes element positions and sizes.
//!
//! Implements Taffy's tree traits on a `LayoutTree` wrapper around our Arena,
//! with bidirectional NodeId mapping (slotmap ↔ taffy).

use std::collections::HashMap;

use style::dom::TElement;
use style::values::computed::length_percentage::CalcLengthPercentage;
use style::values::computed::CSSPixelLength;
use style::Atom;
use taffy::prelude::*;

use super::convert;
use super::stylo_bridge::StyloNode;
use super::text::TextBrush;
use crate::dom::arena::{Arena, NodeId as DomNodeId};
use crate::dom::node::NodeData;

use super::FONT_CTX;

// ─── LayoutTree wrapper ──────────────────────────────────────────────────

/// Wrapper around Arena that implements Taffy's tree traits.
///
/// Taffy uses `taffy::NodeId` (a usize), but our arena uses generational
/// `slotmap::NodeId`. We build bidirectional mappings per layout pass.
pub struct LayoutTree<'a> {
    pub arena: &'a mut Arena,
    /// Map from our DomNodeId → taffy index
    dom_to_taffy: HashMap<DomNodeId, usize>,
    /// Map from taffy index → our DomNodeId
    taffy_to_dom: Vec<DomNodeId>,
    /// Parley font context for text measurement
    font_ctx: parley::FontContext,
    /// Parley layout context (per-layout-pass scratch space)
    layout_ctx: parley::LayoutContext<TextBrush>,
}

impl<'a> LayoutTree<'a> {
    /// Build a LayoutTree by walking the arena and assigning taffy IDs.
    fn new(arena: &'a mut Arena, font_ctx: parley::FontContext) -> Self {
        let mut dom_to_taffy = HashMap::new();
        let mut taffy_to_dom = Vec::new();

        // DFS walk to assign sequential taffy IDs to all nodes
        let mut stack = vec![arena.document];
        while let Some(dom_id) = stack.pop() {
            let taffy_idx = taffy_to_dom.len();
            dom_to_taffy.insert(dom_id, taffy_idx);
            taffy_to_dom.push(dom_id);

            // Push children in reverse order so first child is processed first
            let mut children = Vec::new();
            let mut child = arena.nodes[dom_id].first_child;
            while let Some(c) = child {
                children.push(c);
                child = arena.nodes[c].next_sibling;
            }
            for c in children.into_iter().rev() {
                stack.push(c);
            }
        }

        Self {
            arena,
            dom_to_taffy,
            taffy_to_dom,
            font_ctx,
            layout_ctx: parley::LayoutContext::new(),
        }
    }

    #[inline]
    fn dom_id(&self, taffy_id: taffy::NodeId) -> DomNodeId {
        self.taffy_to_dom[usize::from(taffy_id)]
    }

    #[inline]
    fn taffy_id(&self, dom_id: DomNodeId) -> taffy::NodeId {
        taffy::NodeId::from(self.dom_to_taffy[&dom_id])
    }

    fn node(&self, taffy_id: taffy::NodeId) -> &crate::dom::arena::Node {
        &self.arena.nodes[self.dom_id(taffy_id)]
    }

    fn node_mut(&mut self, taffy_id: taffy::NodeId) -> &mut crate::dom::arena::Node {
        let dom_id = self.dom_id(taffy_id);
        &mut self.arena.nodes[dom_id]
    }
}

// ─── TraversePartialTree + TraverseTree ──────────────────────────────────

/// Iterator over a node's children, yielding taffy::NodeId values.
pub struct ChildIter {
    children: Vec<taffy::NodeId>,
    idx: usize,
}

impl Iterator for ChildIter {
    type Item = taffy::NodeId;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.children.get(self.idx)?;
        self.idx += 1;
        Some(*item)
    }
}

impl TraversePartialTree for LayoutTree<'_> {
    type ChildIter<'c> = ChildIter where Self: 'c;

    fn child_ids(&self, node_id: taffy::NodeId) -> Self::ChildIter<'_> {
        let dom_id = self.dom_id(node_id);
        let mut children = Vec::new();
        let mut child = self.arena.nodes[dom_id].first_child;
        while let Some(c) = child {
            // Only include nodes that have a taffy ID (they should all have one)
            if let Some(&taffy_idx) = self.dom_to_taffy.get(&c) {
                children.push(taffy::NodeId::from(taffy_idx));
            }
            child = self.arena.nodes[c].next_sibling;
        }
        ChildIter { children, idx: 0 }
    }

    fn child_count(&self, node_id: taffy::NodeId) -> usize {
        let dom_id = self.dom_id(node_id);
        let mut count = 0;
        let mut child = self.arena.nodes[dom_id].first_child;
        while let Some(c) = child {
            if self.dom_to_taffy.contains_key(&c) {
                count += 1;
            }
            child = self.arena.nodes[c].next_sibling;
        }
        count
    }

    fn get_child_id(&self, node_id: taffy::NodeId, index: usize) -> taffy::NodeId {
        let dom_id = self.dom_id(node_id);
        let mut i = 0;
        let mut child = self.arena.nodes[dom_id].first_child;
        while let Some(c) = child {
            if self.dom_to_taffy.contains_key(&c) {
                if i == index {
                    return taffy::NodeId::from(self.dom_to_taffy[&c]);
                }
                i += 1;
            }
            child = self.arena.nodes[c].next_sibling;
        }
        panic!("child index {index} out of bounds for node {:?}", node_id);
    }
}

impl TraverseTree for LayoutTree<'_> {}

// ─── LayoutPartialTree ───────────────────────────────────────────────────

fn resolve_calc_value(calc_ptr: *const (), parent_size: f32) -> f32 {
    let calc = unsafe { &*(calc_ptr as *const CalcLengthPercentage) };
    let result = calc.resolve(CSSPixelLength::new(parent_size));
    result.px()
}

impl LayoutPartialTree for LayoutTree<'_> {
    type CoreContainerStyle<'c> = &'c taffy::Style<Atom> where Self: 'c;
    type CustomIdent = Atom;

    fn get_core_container_style(&self, node_id: taffy::NodeId) -> &taffy::Style<Atom> {
        &self.node(node_id).taffy_style
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &Layout) {
        self.node_mut(node_id).taffy_unrounded = *layout;
    }

    fn resolve_calc_value(&self, calc_ptr: *const (), parent_size: f32) -> f32 {
        resolve_calc_value(calc_ptr, parent_size)
    }

    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            tree.compute_child_layout_inner(node_id, inputs)
        })
    }
}

impl LayoutTree<'_> {
    /// Inner layout dispatch: choose algorithm based on node type and display.
    fn compute_child_layout_inner(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let dom_id = self.dom_id(node_id);

        // Determine node type and display without holding a long-lived borrow.
        // This avoids borrow conflicts with &mut self methods called below.
        let is_text = matches!(&self.arena.nodes[dom_id].data, NodeData::Text(_));
        let is_element = matches!(&self.arena.nodes[dom_id].data, NodeData::Element(_));
        let is_doc = matches!(
            &self.arena.nodes[dom_id].data,
            NodeData::Document | NodeData::DocumentFragment
        );
        let display = self.arena.nodes[dom_id].taffy_style.display;

        if is_text {
            return self.measure_text_node(dom_id, inputs);
        }

        if is_element {
            log::trace!("Layout element {:?} as {:?}", dom_id, display);
            return match display {
                Display::Block => taffy::compute_block_layout(self, node_id, inputs, None),
                Display::Flex => taffy::compute_flexbox_layout(self, node_id, inputs),
                Display::Grid => taffy::compute_grid_layout(self, node_id, inputs),
                Display::None => taffy::LayoutOutput::HIDDEN,
            };
        }

        if is_doc {
            return taffy::compute_block_layout(self, node_id, inputs, None);
        }

        // Comment, Doctype — hidden
        taffy::LayoutOutput::HIDDEN
    }

    /// Measure a text node using Parley for real font metrics.
    fn measure_text_node(
        &mut self,
        dom_id: DomNodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        // Extract text content and parent ID without holding a borrow across &mut self calls
        let text = match &self.arena.nodes[dom_id].data {
            NodeData::Text(t) => t.as_str(),
            _ => return taffy::LayoutOutput::HIDDEN,
        };
        let parent_id = self.arena.nodes[dom_id].parent;

        // Get parent's computed style for font properties
        unsafe { super::stylo_bridge::set_arena(self.arena) };
        let text_style = parent_id.and_then(|pid| {
            let parent_node = StyloNode::new(pid);
            let data = parent_node.borrow_data()?;
            let style = data.styles.get_primary()?;
            Some(super::text::to_text_style(0, style))
        });

        // Build Parley layout using tree_builder with the full TextStyle as root.
        let default_style = parley::style::TextStyle::default();
        let has_parent_style = text_style.is_some();
        let root_style = text_style.as_ref().unwrap_or(&default_style);
        if !has_parent_style {
            log::trace!(
                "Text node {:?} using default text style (no parent computed style)",
                dom_id
            );
        }
        let mut builder = self.layout_ctx.tree_builder(
            &mut self.font_ctx,
            1.0,
            false,
            root_style,
        );
        builder.push_text(text);
        let (mut layout, _built_text) = builder.build();

        // Compute content widths for min/max content sizing
        let content_widths = layout.calculate_content_widths();
        log::trace!(
            "Text {:?} '{}' measured: min={:.1} max={:.1}",
            dom_id,
            &text[..text.len().min(30)],
            content_widths.min,
            content_widths.max
        );
        let taffy_style = &self.arena.nodes[dom_id].taffy_style;

        taffy::compute_leaf_layout(
            inputs,
            taffy_style,
            resolve_calc_value,
            |known_dimensions, available_space| {
                let max_width = match available_space.width {
                    AvailableSpace::Definite(w) => w,
                    AvailableSpace::MinContent => content_widths.min,
                    AvailableSpace::MaxContent => content_widths.max,
                };

                let width = known_dimensions.width.unwrap_or(content_widths.max.min(max_width));

                // Break lines at the resolved width to get correct height
                layout.break_all_lines(Some(width));
                let height = known_dimensions.height.unwrap_or(layout.height());

                taffy::Size { width, height }
            },
        )
    }
}

// ─── CacheTree ───────────────────────────────────────────────────────────

impl taffy::CacheTree for LayoutTree<'_> {
    fn cache_get(
        &self,
        node_id: taffy::NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: taffy::RunMode,
    ) -> Option<taffy::LayoutOutput> {
        self.node(node_id)
            .taffy_cache
            .get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: taffy::NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: taffy::RunMode,
        layout_output: taffy::LayoutOutput,
    ) {
        self.node_mut(node_id).taffy_cache.store(
            known_dimensions,
            available_space,
            run_mode,
            layout_output,
        );
    }

    fn cache_clear(&mut self, node_id: taffy::NodeId) {
        self.node_mut(node_id).taffy_cache.clear();
    }
}

// ─── Block/Flex/Grid container traits ────────────────────────────────────

impl taffy::LayoutBlockContainer for LayoutTree<'_> {
    type BlockContainerStyle<'c> = &'c taffy::Style<Atom> where Self: 'c;
    type BlockItemStyle<'c> = &'c taffy::Style<Atom> where Self: 'c;

    fn get_block_container_style(&self, node_id: taffy::NodeId) -> Self::BlockContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_block_child_style(&self, child_node_id: taffy::NodeId) -> Self::BlockItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }

    fn compute_block_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self, node_id, inputs, |tree, node_id, inputs| {
            // For block children, we need to propagate the block context
            let dom_id = tree.dom_id(node_id);
            let node = &tree.arena.nodes[dom_id];
            match &node.data {
                NodeData::Element(_) => {
                    let display = node.taffy_style.display;
                    match display {
                        Display::Block => {
                            taffy::compute_block_layout(tree, node_id, inputs, block_ctx)
                        }
                        Display::Flex => taffy::compute_flexbox_layout(tree, node_id, inputs),
                        Display::Grid => taffy::compute_grid_layout(tree, node_id, inputs),
                        Display::None => taffy::LayoutOutput::HIDDEN,
                    }
                }
                NodeData::Text(_) => tree.compute_child_layout_inner(node_id, inputs),
                _ => taffy::LayoutOutput::HIDDEN,
            }
        })
    }
}

impl taffy::LayoutFlexboxContainer for LayoutTree<'_> {
    type FlexboxContainerStyle<'c> = &'c taffy::Style<Atom> where Self: 'c;
    type FlexboxItemStyle<'c> = &'c taffy::Style<Atom> where Self: 'c;

    fn get_flexbox_container_style(
        &self,
        node_id: taffy::NodeId,
    ) -> Self::FlexboxContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_flexbox_child_style(&self, child_node_id: taffy::NodeId) -> Self::FlexboxItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

impl taffy::LayoutGridContainer for LayoutTree<'_> {
    type GridContainerStyle<'c> = &'c taffy::Style<Atom> where Self: 'c;
    type GridItemStyle<'c> = &'c taffy::Style<Atom> where Self: 'c;

    fn get_grid_container_style(&self, node_id: taffy::NodeId) -> Self::GridContainerStyle<'_> {
        self.get_core_container_style(node_id)
    }

    fn get_grid_child_style(&self, child_node_id: taffy::NodeId) -> Self::GridItemStyle<'_> {
        self.get_core_container_style(child_node_id)
    }
}

// ─── RoundTree ───────────────────────────────────────────────────────────

impl taffy::RoundTree for LayoutTree<'_> {
    fn get_unrounded_layout(&self, node_id: taffy::NodeId) -> Layout {
        self.node(node_id).taffy_unrounded
    }

    fn set_final_layout(&mut self, node_id: taffy::NodeId, layout: &Layout) {
        self.node_mut(node_id).taffy_layout = *layout;
    }
}

// ─── PrintTree (debugging) ───────────────────────────────────────────────

impl taffy::PrintTree for LayoutTree<'_> {
    fn get_debug_label(&self, node_id: taffy::NodeId) -> &'static str {
        let node = self.node(node_id);
        match &node.data {
            NodeData::Document => "DOCUMENT",
            NodeData::DocumentFragment => "FRAGMENT",
            NodeData::Text(_) => "TEXT",
            NodeData::Comment(_) => "COMMENT",
            NodeData::Doctype { .. } => "DOCTYPE",
            NodeData::Element(_) => match node.taffy_style.display {
                Display::Flex => "ELEMENT (FLEX)",
                Display::Grid => "ELEMENT (GRID)",
                Display::Block => "ELEMENT (BLOCK)",
                Display::None => "ELEMENT (NONE)",
            },
        }
    }

    fn get_final_layout(&self, node_id: taffy::NodeId) -> Layout {
        self.node(node_id).taffy_layout
    }
}

// ─── Public API ──────────────────────────────────────────────────────────

/// Compute box layout for all elements in the arena.
///
/// 1. Converts Stylo ComputedValues → Taffy Style for each element
/// 2. Runs Taffy's layout algorithms (block, flex, grid)
/// 3. Stores final Layout (position + size) on each node
pub fn compute_layout(arena: &mut Arena) {
    let t0 = std::time::Instant::now();
    let node_count = arena.nodes.len();
    log::info!("Layout computation starting ({} nodes)", node_count);

    // Set TLS arena for StyloNode access during style conversion
    unsafe { super::stylo_bridge::set_arena(arena) };

    // Step 1: Convert Stylo ComputedValues → taffy::Style for all elements
    let node_ids: Vec<DomNodeId> = arena.nodes.keys().collect();
    let mut styled_count = 0u32;
    for &dom_id in &node_ids {
        let stylo_node = StyloNode::new(dom_id);
        if let Some(data) = stylo_node.borrow_data()
            && let Some(style) = data.styles.get_primary()
        {
            let taffy_style = convert::to_taffy_style(style);
            log::trace!(
                "Converted style for {:?}: display={:?}",
                dom_id, taffy_style.display
            );
            drop(data);
            arena.nodes[dom_id].taffy_style = taffy_style;
            styled_count += 1;
            continue;
        }
        // Non-element nodes (text, comment, document) get default style
        arena.nodes[dom_id].taffy_style = Default::default();
    }
    log::debug!("Converted {} Stylo styles to Taffy", styled_count);

    // Step 2: Find root element (<html>) — Taffy layout starts from the root element,
    // not the Document node (matching Blitz's pattern).
    let root_element = {
        let mut child = arena.nodes[arena.document].first_child;
        let mut found = None;
        while let Some(id) = child {
            if matches!(&arena.nodes[id].data, NodeData::Element(_)) {
                found = Some(id);
                break;
            }
            child = arena.nodes[id].next_sibling;
        }
        match found {
            Some(id) => id,
            None => {
                log::warn!("No root element found for layout computation");
                return;
            }
        }
    };

    // Step 3: Build LayoutTree with NodeId mappings.
    // Take FontContext from thread-local (reused across renders to avoid
    // repeated system font discovery), and return it when done.
    let font_ctx = FONT_CTX.with(|fc| fc.replace(parley::FontContext::default()));
    let mut tree = LayoutTree::new(arena, font_ctx);
    let root_taffy = tree.taffy_id(root_element);
    log::debug!("Built layout tree ({} mapped nodes)", tree.taffy_to_dom.len());

    // Step 4: Compute layout with 1920x1080 viewport
    let available_space = taffy::Size {
        width: AvailableSpace::Definite(1920.0),
        height: AvailableSpace::Definite(1080.0),
    };
    taffy::compute_root_layout(&mut tree, root_taffy, available_space);
    taffy::round_layout(&mut tree, root_taffy);

    // Return FontContext to thread-local for reuse
    FONT_CTX.with(|fc| fc.replace(tree.font_ctx));

    log::info!(
        "Layout computation complete in {:?} ({} nodes, {} styled)",
        t0.elapsed(), node_count, styled_count
    );
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
