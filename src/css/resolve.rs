//! Style resolution pipeline — runs the Stylo cascade on our DOM tree.
//!
//! Orchestrates: configure Device → parse stylesheets → build Stylist → traverse DOM.

use style::context::{
    QuirksMode, RegisteredSpeculativePainter, RegisteredSpeculativePainters, SharedStyleContext,
    StyleContext,
};
use style::dom::{TDocument, TElement, TNode};
use style::global_style_data::GLOBAL_STYLE_DATA;
use style::media_queries::MediaType;
use style::properties::style_structs::Font;
use style::properties::ComputedValues;
use style::selector_parser::SnapshotMap;
use style::shared_lock::StylesheetGuards;
use style::stylesheets::{AllowImportRules, DocumentStyleSheet, Origin, Stylesheet};
use style::stylist::Stylist;
use style::thread_state::ThreadState;
use style::traversal::{DomTraversal, PerLevelTraversalData};
use style::traversal_flags::TraversalFlags;
use style::Atom;
use style_traits::{CSSPixel, DevicePixel};

use super::stylo_bridge::StyloNode;
use crate::dom::arena::{Arena, NodeId};
use crate::dom::node::NodeData;

// ─── Stub implementations ────────────────────────────────────────────────────

/// Stub font metrics provider — returns zero metrics for SSR.
/// Real font metrics require Parley integration (Phase 4).
#[derive(Debug)]
struct StubFontMetrics;

impl style::servo::media_queries::FontMetricsProvider for StubFontMetrics {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        _font: &Font,
        _base_size: style::values::computed::CSSPixelLength,
        _flags: style::values::specified::font::QueryFontMetricsFlags,
    ) -> style::font_metrics::FontMetrics {
        Default::default()
    }

    fn base_size_for_generic(
        &self,
        _generic: style::values::computed::font::GenericFontFamily,
    ) -> style::values::computed::Length {
        // Default 16px base font size
        style::values::computed::Length::new(16.0)
    }
}

/// Stub speculative painters — no CSS Paint API support.
struct StubPainters;
impl RegisteredSpeculativePainters for StubPainters {
    fn get(&self, _name: &Atom) -> Option<&dyn RegisteredSpeculativePainter> {
        None
    }
}

// ─── DomTraversal impl ──────────────────────────────────────────────────────

pub struct RecalcStyle<'a> {
    context: SharedStyleContext<'a>,
}

impl<'a> RecalcStyle<'a> {
    pub fn new(context: SharedStyleContext<'a>) -> Self {
        Self { context }
    }
}

impl<E> DomTraversal<E> for RecalcStyle<'_>
where
    E: TElement,
{
    fn process_preorder<F: FnMut(E::ConcreteNode)>(
        &self,
        traversal_data: &PerLevelTraversalData,
        context: &mut StyleContext<E>,
        node: E::ConcreteNode,
        note_child: F,
    ) {
        if let Some(el) = node.as_element() {
            let mut data = unsafe { el.ensure_data() };
            style::traversal::recalc_style_at(self, traversal_data, context, el, &mut data, note_child);
            unsafe { el.unset_dirty_descendants() }
        }
    }

    #[inline]
    fn needs_postorder_traversal() -> bool {
        false
    }

    fn process_postorder(&self, _style_context: &mut StyleContext<E>, _node: E::ConcreteNode) {
        // Not used (needs_postorder_traversal is false).
    }

    #[inline]
    fn shared_context(&self) -> &SharedStyleContext<'_> {
        &self.context
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Resolve CSS styles for all elements in the arena.
///
/// This runs the full Stylo cascade: UA stylesheet + author stylesheets from <style> elements.
/// After this call, each element's `stylo_element_data` contains its `ComputedValues`.
pub fn resolve_styles(arena: &Arena) {
    let t0 = std::time::Instant::now();

    // Enter Stylo's LAYOUT thread state (required by Stylo internals).
    style::thread_state::enter(ThreadState::LAYOUT);

    let guard = &arena.guard;
    let guards = StylesheetGuards {
        author: &guard.read(),
        ua_or_user: &guard.read(),
    };

    // Create Device for a standard 1920x1080 viewport (SSR default).
    let viewport_size = euclid::Size2D::new(1920.0f32, 1080.0f32);
    let device_pixel_ratio = euclid::Scale::<f32, CSSPixel, DevicePixel>::new(1.0);
    let default_values = ComputedValues::initial_values_with_font_override(Font::initial_values());
    let device = style::media_queries::Device::new(
        MediaType::screen(),
        QuirksMode::NoQuirks,
        viewport_size,
        device_pixel_ratio,
        Box::new(StubFontMetrics),
        default_values,
        style::queries::values::PrefersColorScheme::Light,
    );

    // Create Stylist and register stylesheets.
    let mut stylist = Stylist::new(device, QuirksMode::NoQuirks);

    // 1. Add UA stylesheet (browser defaults).
    let ua_css = include_str!("ua.css");
    let ua_sheet = make_stylesheet(ua_css, Origin::UserAgent, guard);
    stylist.append_stylesheet(ua_sheet, &guard.read());

    // 2. Collect author stylesheets from <style> elements in the DOM.
    collect_and_add_author_stylesheets(arena, &mut stylist, guard);

    // Flush the stylist to process pending stylesheet changes.
    stylist.flush(&guards);

    // Find the root element (<html>).
    let doc_node = StyloNode::new(arena, arena.document);
    let root_element = match TDocument::as_node(&doc_node)
        .first_child()
        .and_then(|n| {
            // Walk children to find the first element (skip doctype, etc.)
            let mut current = Some(n);
            while let Some(node) = current {
                if node.as_element().is_some() {
                    return Some(node);
                }
                current = node.next_sibling();
            }
            None
        })
        .and_then(|n| n.as_element())
    {
        Some(el) => el,
        None => {
            log::warn!("No root element found for style resolution");
            style::thread_state::exit(ThreadState::LAYOUT);
            return;
        }
    };

    // Build empty snapshot map (no incremental restyling for SSR).
    let snapshots = SnapshotMap::new();

    // Create shared style context.
    let animations = Default::default();
    let context = SharedStyleContext {
        traversal_flags: TraversalFlags::empty(),
        stylist: &stylist,
        options: GLOBAL_STYLE_DATA.options.clone(),
        guards,
        visited_styles_enabled: false,
        animations: animations,
        current_time_for_animations: 0.0,
        snapshot_map: &snapshots,
        registered_speculative_painters: &StubPainters,
    };

    // Pre-traverse to get a traversal token.
    let token = RecalcStyle::pre_traverse(root_element, &context);
    if token.should_traverse() {
        let traverser = RecalcStyle::new(context);
        let pool = style::global_style_data::STYLE_THREAD_POOL.pool();
        style::driver::traverse_dom(&traverser, token, pool.as_ref());
    }

    // Garbage collect the rule tree.
    stylist.rule_tree().maybe_gc();

    style::thread_state::exit(ThreadState::LAYOUT);

    log::info!("Style resolution complete in {:?}", t0.elapsed());
}

/// Collect CSS from <style> elements and add them to the Stylist.
fn collect_and_add_author_stylesheets(
    arena: &Arena,
    stylist: &mut Stylist,
    guard: &style::shared_lock::SharedRwLock,
) {
    // Walk the DOM looking for <style> elements.
    let mut style_nodes = Vec::new();
    collect_style_elements(arena, arena.document, &mut style_nodes);

    for node_id in style_nodes {
        // Get the text content of the <style> element.
        let css = get_text_content(arena, node_id);
        if css.is_empty() {
            continue;
        }
        let sheet = make_stylesheet(&css, Origin::Author, guard);
        stylist.append_stylesheet(sheet, &guard.read());
    }
}

/// Recursively collect <style> element NodeIds from the DOM.
fn collect_style_elements(arena: &Arena, node_id: NodeId, result: &mut Vec<NodeId>) {
    if let NodeData::Element(ref data) = arena.nodes[node_id].data {
        if &*data.name.local == "style" && data.name.ns == markup5ever::ns!(html) {
            result.push(node_id);
        }
    }
    let mut child = arena.nodes[node_id].first_child;
    while let Some(id) = child {
        collect_style_elements(arena, id, result);
        child = arena.nodes[id].next_sibling;
    }
}

/// Get the concatenated text content of a node's children.
fn get_text_content(arena: &Arena, node_id: NodeId) -> String {
    let mut result = String::new();
    let mut child = arena.nodes[node_id].first_child;
    while let Some(id) = child {
        if let NodeData::Text(ref text) = arena.nodes[id].data {
            result.push_str(text);
        }
        child = arena.nodes[id].next_sibling;
    }
    result
}

/// Create a parsed Stylesheet wrapped in DocumentStyleSheet.
fn make_stylesheet(
    css: &str,
    origin: Origin,
    guard: &style::shared_lock::SharedRwLock,
) -> DocumentStyleSheet {
    let url = url::Url::parse("about:blank").unwrap();
    let url_extra = style::stylesheets::UrlExtraData::from(url);
    let media = style::servo_arc::Arc::new(guard.wrap(style::media_queries::MediaList::empty()));
    let sheet = Stylesheet::from_str(
        css,
        url_extra,
        origin,
        media,
        guard.clone(),
        None, // No stylesheet loader (no @import support yet)
        None,
        QuirksMode::NoQuirks,
        AllowImportRules::Yes,
    );
    DocumentStyleSheet(style::servo_arc::Arc::new(sheet))
}
