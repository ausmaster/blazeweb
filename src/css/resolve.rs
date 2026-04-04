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

// ─── Font metrics provider (Parley-backed) ──────────────────────────────────

/// Font metrics provider backed by Parley's font context.
/// Queries real glyph metrics (ascent, x-height, cap-height) for CSS unit
/// resolution (ex, ch, em). Ported from Blitz's BlitzFontMetricsProvider.
struct ParleyFontMetrics {
    font_ctx: std::sync::Arc<std::sync::Mutex<parley::FontContext>>,
}

impl std::fmt::Debug for ParleyFontMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ParleyFontMetrics")
    }
}

impl style::servo::media_queries::FontMetricsProvider for ParleyFontMetrics {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        font_styles: &Font,
        font_size: style::values::computed::CSSPixelLength,
        _flags: style::values::specified::font::QueryFontMetricsFlags,
    ) -> style::font_metrics::FontMetrics {
        use parley::fontique::{Attributes, QueryStatus};
        use skrifa::MetadataProvider as _;
        use skrifa::instance::{LocationRef, Size};
        use skrifa::metrics::GlyphMetrics;

        // Lock the shared font context. Destructure to avoid double-borrow.
        let mut font_ctx = self.font_ctx.lock().unwrap();
        let parley::FontContext {
            ref mut collection,
            ref mut source_cache,
        } = *font_ctx;

        let mut query = collection.query(source_cache);
        let families = font_styles
            .font_family
            .families
            .iter()
            .map(super::text::query_font_family);
        query.set_families(families);
        query.set_attributes(Attributes {
            width: super::text::font_width(font_styles.font_stretch),
            weight: super::text::font_weight(font_styles.font_weight),
            style: super::text::font_style(font_styles.font_style),
        });

        let variations = super::text::font_variations(&font_styles.font_variation_settings);
        let sz = Size::new(font_size.px());

        // Find a font that has the '0' character for ch-unit measurement
        let mut zero_advance = None;
        let mut ascent = 0.0f32;
        let mut x_height = None;
        let mut cap_height = None;

        query.matches_with(|q_font| {
            let Ok(font_ref) = skrifa::FontRef::from_index(q_font.blob.as_ref(), q_font.index)
            else {
                return QueryStatus::Continue;
            };

            let location = font_ref.axes().location(
                variations
                    .iter()
                    .map(|v| (skrifa::Tag::from_be_bytes(v.tag.to_bytes()), v.value)),
            );
            let location_ref = LocationRef::from(&location);
            let metrics = skrifa::metrics::Metrics::new(&font_ref, sz, location_ref);
            ascent = metrics.ascent;
            x_height = metrics.x_height;
            cap_height = metrics.cap_height;

            // Measure '0' advance width for ch unit
            let charmap = skrifa::charmap::Charmap::new(&font_ref);
            if let Some(glyph_id) = charmap.map('0') {
                let glyph_metrics = GlyphMetrics::new(&font_ref, sz, location_ref);
                zero_advance = glyph_metrics.advance_width(glyph_id);
            }

            QueryStatus::Stop
        });

        log::trace!(
            "Font metrics query: size={:.1}px ascent={:.1} x_height={:?} zero_advance={:?}",
            font_size.px(), ascent, x_height, zero_advance
        );

        use style::values::computed::CSSPixelLength;
        style::font_metrics::FontMetrics {
            ascent: CSSPixelLength::new(ascent),
            x_height: x_height.filter(|xh| *xh != 0.0).map(CSSPixelLength::new),
            cap_height: cap_height.map(CSSPixelLength::new),
            zero_advance_measure: zero_advance.map(CSSPixelLength::new),
            ic_width: None, // CJK ideographic advance — skip for now
            script_percent_scale_down: None,
            script_script_percent_scale_down: None,
        }
    }

    fn base_size_for_generic(
        &self,
        generic: style::values::computed::font::GenericFontFamily,
    ) -> style::values::computed::Length {
        let size = match generic {
            style::values::computed::font::GenericFontFamily::Monospace => 13.0,
            _ => 16.0,
        };
        style::values::computed::Length::new(size)
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

// ─── Stylo feature prefs ─────────────────────────────────────────────────────

/// Enable CSS features in Stylo that are behind servo prefs.
/// Must be called before creating any Stylist. Uses `Once` to run only once per process.
fn ensure_stylo_prefs() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        style_config::set_bool("layout.flexbox.enabled", true);
        style_config::set_bool("layout.grid.enabled", true);
        style_config::set_bool("layout.columns.enabled", true);
        style_config::set_bool("layout.legacy_layout", true);
        style_config::set_bool("layout.unimplemented", true);
        style_config::set_bool("layout.container-queries.enabled", true);
        log::debug!("Stylo CSS prefs initialized (grid, flexbox, columns, containers enabled)");
        log::info!(
            "Note: CJK/Thai text line-breaking uses character boundaries \
             (Parley upstream uses non-complex-script segmenter)"
        );
    });
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Resolve CSS styles for all elements in the arena.
///
/// This runs the full Stylo cascade: UA stylesheet + author stylesheets from <style> elements.
/// After this call, each element's `stylo_element_data` contains its `ComputedValues`.
pub fn resolve_styles(arena: &mut Arena) {
    let t0 = std::time::Instant::now();
    let node_count = arena.nodes.len();
    log::info!("Style resolution starting ({} nodes)", node_count);

    // Ensure CSS features (grid, flexbox, etc.) are enabled in Stylo
    ensure_stylo_prefs();

    // Prepare nodes for Stylo: cache interned atoms and parse inline styles.
    prepare_for_stylo(arena);
    log::debug!("Prepared {} nodes for Stylo (cached atoms + inline styles)", node_count);

    // Enter Stylo's LAYOUT thread state (required by Stylo internals).
    style::thread_state::enter(ThreadState::LAYOUT);

    // Set the thread-local arena pointer so StyloNode can access the arena.
    unsafe { super::stylo_bridge::set_arena(arena) };

    let guard = &arena.guard;
    let guards = StylesheetGuards {
        author: &guard.read(),
        ua_or_user: &guard.read(),
    };

    // Create Device with Parley-backed font metrics.
    // Take FontContext from thread-local (reused across renders).
    // We keep an Arc clone so we can recover the FontContext after Stylist drops.
    let font_ctx_arc = std::sync::Arc::new(std::sync::Mutex::new(
        super::FONT_CTX.with(|fc| fc.replace(parley::FontContext::default())),
    ));
    let viewport_size = euclid::Size2D::new(1920.0f32, 1080.0f32);
    let device_pixel_ratio = euclid::Scale::<f32, CSSPixel, DevicePixel>::new(1.0);
    let default_values = ComputedValues::initial_values_with_font_override(Font::initial_values());
    let device = style::media_queries::Device::new(
        MediaType::screen(),
        QuirksMode::NoQuirks,
        viewport_size,
        device_pixel_ratio,
        Box::new(ParleyFontMetrics {
            font_ctx: font_ctx_arc.clone(),
        }),
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
    let author_count = collect_and_add_author_stylesheets(arena, &mut stylist, guard);
    log::debug!("Registered {} author stylesheet(s) + 1 UA stylesheet", author_count);

    // Flush the stylist to process pending stylesheet changes.
    stylist.flush(&guards);

    // Find the root element (<html>).
    let doc_node = StyloNode::new(arena.document);
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
        animations,
        current_time_for_animations: 0.0,
        snapshot_map: &snapshots,
        registered_speculative_painters: &StubPainters,
    };

    // Traverse the DOM to compute styles.
    let token = RecalcStyle::pre_traverse(root_element, &context);
    if token.should_traverse() {
        log::debug!("Starting sequential style traversal");
        let traverser = RecalcStyle::new(context);
        style::driver::traverse_dom::<_, _>(&traverser, token, None);
    } else {
        log::debug!("Style traversal skipped (pre_traverse returned no-op token)");
    }

    // Garbage collect the rule tree.
    stylist.rule_tree().maybe_gc();

    // Recover FontContext from Arc<Mutex> and return to thread-local for reuse.
    drop(stylist);
    if let Ok(font_ctx) = std::sync::Arc::try_unwrap(font_ctx_arc) {
        log::trace!("FontContext recovered to thread-local");
        super::FONT_CTX.with(|fc| fc.replace(font_ctx.into_inner().unwrap()));
    } else {
        log::warn!("FontContext could not be recovered (Arc still shared)");
    }

    style::thread_state::exit(ThreadState::LAYOUT);

    log::info!("Style resolution complete in {:?} ({} nodes)", t0.elapsed(), node_count);
}

/// Collect CSS from <style> elements and add them to the Stylist.
fn collect_and_add_author_stylesheets(
    arena: &Arena,
    stylist: &mut Stylist,
    guard: &style::shared_lock::SharedRwLock,
) -> usize {
    let mut style_nodes = Vec::new();
    collect_style_elements(arena, arena.document, &mut style_nodes);

    let mut count = 0;
    for node_id in style_nodes {
        let css = get_text_content(arena, node_id);
        if css.is_empty() {
            continue;
        }
        log::trace!("Parsing author stylesheet ({} bytes)", css.len());
        let sheet = make_stylesheet(&css, Origin::Author, guard);
        stylist.append_stylesheet(sheet, &guard.read());
        count += 1;
    }
    count
}

/// Recursively collect <style> element NodeIds from the DOM.
fn collect_style_elements(arena: &Arena, node_id: NodeId, result: &mut Vec<NodeId>) {
    if let NodeData::Element(ref data) = arena.nodes[node_id].data
        && &*data.name.local == "style"
        && data.name.ns == markup5ever::ns!(html)
    {
        result.push(node_id);
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

/// Prepare all nodes for Stylo traversal:
/// 1. Cache interned atoms (local_name, namespace, id) so TElement methods
///    can return stable references without Box::leak.
/// 2. Parse inline style="" attributes into PropertyDeclarationBlocks
///    so Stylo's cascade sees them.
fn prepare_for_stylo(arena: &mut Arena) {
    let url = url::Url::parse("about:blank").unwrap();
    let url_extra = style::stylesheets::UrlExtraData::from(url);

    let mut element_count = 0u32;
    let mut inline_style_count = 0u32;
    let ids: Vec<NodeId> = arena.nodes.keys().collect();
    for id in ids {
        // Extract element info before mutating
        let elem_info = match &arena.nodes[id].data {
            NodeData::Element(data) => Some((
                web_atoms::LocalName::from(&*data.name.local),
                web_atoms::Namespace::from(&*data.name.ns),
                data.get_attribute("id")
                    .filter(|s| !s.is_empty())
                    .map(style::Atom::from),
                data.get_attribute("style")
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            )),
            _ => None,
        };

        if let Some((local_name, namespace, id_atom, style_css)) = elem_info {
            element_count += 1;
            let node = &mut arena.nodes[id];
            node.cached_atom_local_name = Some(local_name);
            node.cached_atom_namespace = Some(namespace);
            node.cached_atom_id = id_atom;

            if let Some(css) = style_css {
                inline_style_count += 1;
                let block = style::properties::parse_style_attribute(
                    &css,
                    &url_extra,
                    None,
                    QuirksMode::NoQuirks,
                    style::stylesheets::CssRuleType::Style,
                );
                let guard = &arena.guard;
                node.parsed_style_attribute =
                    Some(style::servo_arc::Arc::new(guard.wrap(block)));
            }
        }
    }
    log::debug!(
        "Cached atoms for {} elements, parsed {} inline style attributes",
        element_count, inline_style_count
    );
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

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
