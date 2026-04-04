use super::*;
use style::dom::TElement;

// ─── Display conversion ──────────────────────────────────────────────────

#[test]
fn display_none() {
    assert!(matches!(display(stylo::Display::None), taffy::Display::None));
}

#[test]
fn display_block() {
    assert!(matches!(display(stylo::Display::Block), taffy::Display::Block));
}

#[test]
fn display_flex() {
    assert!(matches!(display(stylo::Display::Flex), taffy::Display::Flex));
}

#[test]
fn display_grid() {
    assert!(matches!(display(stylo::Display::Grid), taffy::Display::Grid));
}

#[test]
fn display_inline_block_is_block() {
    // InlineBlock has DisplayInside::FlowRoot
    assert!(matches!(display(stylo::Display::InlineBlock), taffy::Display::Block));
}


// ─── Box sizing ──────────────────────────────────────────────────────────

#[test]
fn box_sizing_border_box() {
    assert!(matches!(
        box_sizing(stylo::BoxSizing::BorderBox),
        taffy::BoxSizing::BorderBox
    ));
}

#[test]
fn box_sizing_content_box() {
    assert!(matches!(
        box_sizing(stylo::BoxSizing::ContentBox),
        taffy::BoxSizing::ContentBox
    ));
}

// ─── Position ────────────────────────────────────────────────────────────

#[test]
fn position_relative() {
    assert!(matches!(position(stylo::Position::Relative), taffy::Position::Relative));
}

#[test]
fn position_static_maps_to_relative() {
    assert!(matches!(position(stylo::Position::Static), taffy::Position::Relative));
}

#[test]
fn position_absolute() {
    assert!(matches!(position(stylo::Position::Absolute), taffy::Position::Absolute));
}

#[test]
fn position_fixed_maps_to_absolute() {
    assert!(matches!(position(stylo::Position::Fixed), taffy::Position::Absolute));
}

#[test]
fn position_sticky_maps_to_relative() {
    assert!(matches!(position(stylo::Position::Sticky), taffy::Position::Relative));
}

// ─── Overflow ────────────────────────────────────────────────────────────

#[test]
fn overflow_visible() {
    assert!(matches!(overflow(stylo::Overflow::Visible), taffy::Overflow::Visible));
}

#[test]
fn overflow_hidden() {
    assert!(matches!(overflow(stylo::Overflow::Hidden), taffy::Overflow::Hidden));
}

#[test]
fn overflow_scroll() {
    assert!(matches!(overflow(stylo::Overflow::Scroll), taffy::Overflow::Scroll));
}

#[test]
fn overflow_auto_maps_to_scroll() {
    assert!(matches!(overflow(stylo::Overflow::Auto), taffy::Overflow::Scroll));
}

// ─── Flexbox ─────────────────────────────────────────────────────────────

#[test]
fn flex_direction_row() {
    assert!(matches!(flex_direction(stylo::FlexDirection::Row), taffy::FlexDirection::Row));
}

#[test]
fn flex_direction_column() {
    assert!(matches!(flex_direction(stylo::FlexDirection::Column), taffy::FlexDirection::Column));
}

#[test]
fn flex_direction_row_reverse() {
    assert!(matches!(
        flex_direction(stylo::FlexDirection::RowReverse),
        taffy::FlexDirection::RowReverse
    ));
}

#[test]
fn flex_direction_column_reverse() {
    assert!(matches!(
        flex_direction(stylo::FlexDirection::ColumnReverse),
        taffy::FlexDirection::ColumnReverse
    ));
}

#[test]
fn flex_wrap_nowrap() {
    assert!(matches!(flex_wrap(stylo::FlexWrap::Nowrap), taffy::FlexWrap::NoWrap));
}

#[test]
fn flex_wrap_wrap() {
    assert!(matches!(flex_wrap(stylo::FlexWrap::Wrap), taffy::FlexWrap::Wrap));
}

#[test]
fn flex_wrap_reverse() {
    assert!(matches!(flex_wrap(stylo::FlexWrap::WrapReverse), taffy::FlexWrap::WrapReverse));
}

// ─── Alignment ───────────────────────────────────────────────────────────

#[test]
fn content_alignment_normal_is_none() {
    let input = stylo::ContentDistribution::new(stylo::AlignFlags::NORMAL);
    assert!(content_alignment(input).is_none());
}

#[test]
fn content_alignment_center() {
    let input = stylo::ContentDistribution::new(stylo::AlignFlags::CENTER);
    assert!(matches!(content_alignment(input), Some(taffy::AlignContent::Center)));
}

#[test]
fn content_alignment_space_between() {
    let input = stylo::ContentDistribution::new(stylo::AlignFlags::SPACE_BETWEEN);
    assert!(matches!(content_alignment(input), Some(taffy::AlignContent::SpaceBetween)));
}

#[test]
fn item_alignment_auto_is_none() {
    assert!(item_alignment(stylo::AlignFlags::AUTO).is_none());
}

#[test]
fn item_alignment_center() {
    assert!(matches!(item_alignment(stylo::AlignFlags::CENTER), Some(taffy::AlignItems::Center)));
}

#[test]
fn item_alignment_stretch() {
    assert!(matches!(item_alignment(stylo::AlignFlags::STRETCH), Some(taffy::AlignItems::Stretch)));
}

#[test]
fn item_alignment_flex_start() {
    assert!(matches!(
        item_alignment(stylo::AlignFlags::FLEX_START),
        Some(taffy::AlignItems::FlexStart)
    ));
}

#[test]
fn item_alignment_baseline() {
    assert!(matches!(
        item_alignment(stylo::AlignFlags::BASELINE),
        Some(taffy::AlignItems::Baseline)
    ));
}

// ─── Box generation mode ─────────────────────────────────────────────────

// ─── Grid ────────────────────────────────────────────────────────────────

#[test]
fn grid_auto_flow_row() {
    let input = stylo::GridAutoFlow::ROW;
    assert!(matches!(grid_auto_flow(input), taffy::GridAutoFlow::Row));
}

#[test]
fn grid_auto_flow_column_dense() {
    let input = stylo::GridAutoFlow::DENSE; // no ROW flag = column
    assert!(matches!(grid_auto_flow(input), taffy::GridAutoFlow::ColumnDense));
}

#[test]
fn grid_auto_flow_row_dense() {
    let input = stylo::GridAutoFlow::ROW | stylo::GridAutoFlow::DENSE;
    assert!(matches!(grid_auto_flow(input), taffy::GridAutoFlow::RowDense));
}

// ─── Text align ──────────────────────────────────────────────────────────

#[test]
fn text_align_auto_default() {
    assert!(matches!(text_align(stylo::TextAlign::Start), taffy::TextAlign::Auto));
}

#[test]
fn text_align_moz_left() {
    assert!(matches!(text_align(stylo::TextAlign::MozLeft), taffy::TextAlign::LegacyLeft));
}

// ─── to_taffy_style integration ──────────────────────────────────────────
// These test the full Stylo→Taffy conversion through the real pipeline,
// which is more robust than constructing individual Stylo types manually.

#[test]
fn to_taffy_style_flex_display() {
    let mut arena = crate::dom::parse_document(
        "<html><head><style>div { display: flex; }</style></head><body><div></div></body></html>",
    );
    crate::css::resolve::resolve_styles(&mut arena);
    unsafe { crate::css::stylo_bridge::set_arena(&arena) };

    let div = arena.find_element(arena.document, "div").unwrap();
    let node = crate::css::stylo_bridge::StyloNode::new(div);
    let data = node.borrow_data().unwrap();
    let style = data.styles.get_primary().unwrap();
    let taffy = to_taffy_style(style);
    assert!(matches!(taffy.display, taffy::Display::Flex));
}

#[test]
fn to_taffy_style_grid_display() {
    let mut arena = crate::dom::parse_document(
        "<html><head><style>main { display: grid; }</style></head><body><main></main></body></html>",
    );
    crate::css::resolve::resolve_styles(&mut arena);
    unsafe { crate::css::stylo_bridge::set_arena(&arena) };

    let main = arena.find_element(arena.document, "main").unwrap();
    let node = crate::css::stylo_bridge::StyloNode::new(main);
    let data = node.borrow_data().unwrap();
    let style = data.styles.get_primary().unwrap();
    let stylo_display = style.clone_display();
    let taffy = to_taffy_style(style);
    assert!(
        matches!(taffy.display, taffy::Display::Grid),
        "expected Grid, got {:?} (stylo inside={:?} outside={:?})",
        taffy.display,
        stylo_display.inside(),
        stylo_display.outside()
    );
}

#[test]
fn to_taffy_style_explicit_dimensions() {
    let mut arena = crate::dom::parse_document(
        "<html><head><style>div { width: 100px; height: 50px; }</style></head><body><div></div></body></html>",
    );
    crate::css::resolve::resolve_styles(&mut arena);
    crate::css::layout::compute_layout(&mut arena);

    // Verify through layout output (the real test of conversion correctness)
    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    assert_eq!(layout.size.width, 100.0);
    assert_eq!(layout.size.height, 50.0);
}

#[test]
fn to_taffy_style_margin_auto_centers() {
    let mut arena = crate::dom::parse_document(
        "<html><head><style>body{margin:0}div{width:200px;margin-left:auto;margin-right:auto}</style></head><body><div></div></body></html>",
    );
    crate::css::resolve::resolve_styles(&mut arena);
    crate::css::layout::compute_layout(&mut arena);

    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    assert_eq!(layout.size.width, 200.0);
    // margin:auto should center — x should be (1920-200)/2 = 860
    assert!(
        layout.location.x > 0.0,
        "margin:auto should offset from left, got x={}",
        layout.location.x
    );
}

#[test]
fn to_taffy_style_border_none_is_zero() {
    let mut arena = crate::dom::parse_document(
        "<html><head><style>div{width:100px;height:50px;border:none}</style></head><body><div></div></body></html>",
    );
    crate::css::resolve::resolve_styles(&mut arena);
    crate::css::layout::compute_layout(&mut arena);

    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    assert_eq!(layout.border.left, 0.0);
    assert_eq!(layout.border.right, 0.0);
}

#[test]
fn to_taffy_style_border_solid_has_width() {
    let mut arena = crate::dom::parse_document(
        "<html><head><style>div{width:100px;height:50px;border:3px solid black;box-sizing:content-box}</style></head><body><div></div></body></html>",
    );
    crate::css::resolve::resolve_styles(&mut arena);
    crate::css::layout::compute_layout(&mut arena);

    let div = arena.find_element(arena.document, "div").unwrap();
    let layout = &arena.nodes[div].taffy_layout;
    // content-box: total width = 100 + 3*2 = 106
    assert_eq!(layout.size.width, 106.0);
    assert_eq!(layout.border.left, 3.0);
    assert_eq!(layout.border.right, 3.0);
}
