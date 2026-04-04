//! Conversion functions from Stylo computed style types to Taffy equivalents.
//!
//! Ported from Blitz's `stylo_taffy` crate (convert.rs + wrapper.rs).
//! Same Stylo 0.12 types — direct port with minimal changes.

/// Private module of type aliases for cleaner Stylo type references.
pub(crate) mod stylo {
    pub(crate) use style::Atom;
    pub(crate) use style::properties::ComputedValues;
    pub(crate) use style::properties::generated::longhands::box_sizing::computed_value::T as BoxSizing;
    pub(crate) use style::properties::longhands::aspect_ratio::computed_value::T as AspectRatio;
    pub(crate) use style::properties::longhands::position::computed_value::T as Position;
    pub(crate) use style::values::computed::length_percentage::CalcLengthPercentage;
    pub(crate) use style::values::computed::length_percentage::Unpacked as UnpackedLengthPercentage;
    pub(crate) use style::values::computed::{BorderSideWidth, LengthPercentage, Percentage};
    pub(crate) use style::values::generics::NonNegative;
    pub(crate) use style::values::generics::length::{
        GenericLengthPercentageOrNormal, GenericMargin, GenericMaxSize, GenericSize,
    };
    pub(crate) use style::values::generics::position::{Inset as GenericInset, PreferredRatio};
    pub(crate) use style::values::specified::align::{AlignFlags, ContentDistribution};
    pub(crate) use style::values::specified::border::BorderStyle;
    pub(crate) use style::values::specified::box_::{
        Display, DisplayInside, DisplayOutside, Overflow,
    };
    pub(crate) use style::values::specified::position::GridTemplateAreas;
    pub(crate) use stylo_atoms::atom;
    pub(crate) type MarginVal = GenericMargin<LengthPercentage>;
    pub(crate) type InsetVal = GenericInset<Percentage, LengthPercentage>;
    pub(crate) type Size = GenericSize<NonNegative<LengthPercentage>>;
    pub(crate) type MaxSize = GenericMaxSize<NonNegative<LengthPercentage>>;
    pub(crate) type Gap = GenericLengthPercentageOrNormal<NonNegative<LengthPercentage>>;

    pub(crate) use style::{
        computed_values::{flex_direction::T as FlexDirection, flex_wrap::T as FlexWrap},
        values::generics::flex::GenericFlexBasis,
    };
    pub(crate) type FlexBasis = GenericFlexBasis<Size>;

    pub(crate) use style::values::computed::text::TextAlign;
    pub(crate) use style::{
        computed_values::grid_auto_flow::T as GridAutoFlow,
        values::{
            computed::{GridLine, GridTemplateComponent, ImplicitGridTracks},
            generics::grid::{RepeatCount, TrackBreadth, TrackListValue, TrackSize},
            specified::GenericGridTemplateComponent,
        },
    };
}

use stylo::Atom;
use taffy::CompactLength;
use taffy::style_helpers::*;

// ─── Primitive conversions ───────────────────────────────────────────────

#[inline]
pub fn length_percentage(val: &stylo::LengthPercentage) -> taffy::LengthPercentage {
    match val.unpack() {
        stylo::UnpackedLengthPercentage::Calc(calc_ptr) => {
            let val =
                CompactLength::calc(calc_ptr as *const stylo::CalcLengthPercentage as *const ());
            unsafe { taffy::LengthPercentage::from_raw(val) }
        }
        stylo::UnpackedLengthPercentage::Length(len) => length(len.px()),
        stylo::UnpackedLengthPercentage::Percentage(percentage) => percent(percentage.0),
    }
}

#[inline]
pub fn dimension(val: &stylo::Size) -> taffy::Dimension {
    match val {
        stylo::Size::LengthPercentage(val) => length_percentage(&val.0).into(),
        stylo::Size::Auto => taffy::Dimension::AUTO,
        stylo::Size::MaxContent
        | stylo::Size::MinContent
        | stylo::Size::FitContent
        | stylo::Size::FitContentFunction(_)
        | stylo::Size::Stretch
        | stylo::Size::WebkitFillAvailable => taffy::Dimension::AUTO,
        // Anchor positioning not supported — fall back to auto
        stylo::Size::AnchorSizeFunction(_) | stylo::Size::AnchorContainingCalcFunction(_) => {
            taffy::Dimension::AUTO
        }
    }
}

#[inline]
pub fn max_size_dimension(val: &stylo::MaxSize) -> taffy::Dimension {
    match val {
        stylo::MaxSize::LengthPercentage(val) => length_percentage(&val.0).into(),
        stylo::MaxSize::None => taffy::Dimension::AUTO,
        stylo::MaxSize::MaxContent
        | stylo::MaxSize::MinContent
        | stylo::MaxSize::FitContent
        | stylo::MaxSize::FitContentFunction(_)
        | stylo::MaxSize::Stretch
        | stylo::MaxSize::WebkitFillAvailable => taffy::Dimension::AUTO,
        // Anchor positioning not supported — fall back to auto
        stylo::MaxSize::AnchorSizeFunction(_) | stylo::MaxSize::AnchorContainingCalcFunction(_) => {
            taffy::Dimension::AUTO
        }
    }
}

#[inline]
pub fn margin(val: &stylo::MarginVal) -> taffy::LengthPercentageAuto {
    match val {
        stylo::MarginVal::Auto => taffy::LengthPercentageAuto::AUTO,
        stylo::MarginVal::LengthPercentage(val) => length_percentage(val).into(),
        // Anchor positioning not supported — fall back to auto
        stylo::MarginVal::AnchorSizeFunction(_)
        | stylo::MarginVal::AnchorContainingCalcFunction(_) => taffy::LengthPercentageAuto::AUTO,
    }
}

#[inline]
pub fn border(
    width: &stylo::BorderSideWidth,
    style: stylo::BorderStyle,
) -> taffy::LengthPercentage {
    if style.none_or_hidden() {
        return taffy::style_helpers::zero();
    }
    taffy::style_helpers::length(width.0.to_f32_px())
}

#[inline]
pub fn inset(val: &stylo::InsetVal) -> taffy::LengthPercentageAuto {
    match val {
        stylo::InsetVal::Auto => taffy::LengthPercentageAuto::AUTO,
        stylo::InsetVal::LengthPercentage(val) => length_percentage(val).into(),
        // Anchor positioning not supported — fall back to auto
        stylo::InsetVal::AnchorSizeFunction(_)
        | stylo::InsetVal::AnchorFunction(_)
        | stylo::InsetVal::AnchorContainingCalcFunction(_) => taffy::LengthPercentageAuto::AUTO,
    }
}

// ─── Display / position / overflow ───────────────────────────────────────

#[inline]
pub fn display(input: stylo::Display) -> taffy::Display {
    let mut display = match input.inside() {
        stylo::DisplayInside::None => taffy::Display::None,
        stylo::DisplayInside::Flex => taffy::Display::Flex,
        stylo::DisplayInside::Grid => taffy::Display::Grid,
        stylo::DisplayInside::Flow => taffy::Display::Block,
        stylo::DisplayInside::FlowRoot => taffy::Display::Block,
        stylo::DisplayInside::TableCell => taffy::Display::Block,
        stylo::DisplayInside::Table => taffy::Display::Grid,
        _ => taffy::Display::DEFAULT,
    };

    if matches!(input.outside(), stylo::DisplayOutside::None) {
        display = taffy::Display::None;
    }

    display
}

#[inline]
pub fn box_sizing(input: stylo::BoxSizing) -> taffy::BoxSizing {
    match input {
        stylo::BoxSizing::BorderBox => taffy::BoxSizing::BorderBox,
        stylo::BoxSizing::ContentBox => taffy::BoxSizing::ContentBox,
    }
}

#[inline]
pub fn position(input: stylo::Position) -> taffy::Position {
    match input {
        stylo::Position::Relative | stylo::Position::Static | stylo::Position::Sticky => {
            taffy::Position::Relative
        }
        stylo::Position::Absolute | stylo::Position::Fixed => taffy::Position::Absolute,
    }
}

#[inline]
pub fn overflow(input: stylo::Overflow) -> taffy::Overflow {
    match input {
        stylo::Overflow::Visible => taffy::Overflow::Visible,
        stylo::Overflow::Clip => taffy::Overflow::Clip,
        stylo::Overflow::Hidden => taffy::Overflow::Hidden,
        stylo::Overflow::Scroll | stylo::Overflow::Auto => taffy::Overflow::Scroll,
    }
}

#[inline]
pub fn aspect_ratio(input: stylo::AspectRatio) -> Option<f32> {
    match input.ratio {
        stylo::PreferredRatio::None => None,
        stylo::PreferredRatio::Ratio(val) => Some(val.0.0 / val.1.0),
    }
}

// ─── Alignment ───────────────────────────────────────────────────────────

#[inline]
pub fn content_alignment(input: stylo::ContentDistribution) -> Option<taffy::AlignContent> {
    match input.primary().value() {
        stylo::AlignFlags::NORMAL | stylo::AlignFlags::AUTO => None,
        stylo::AlignFlags::START | stylo::AlignFlags::LEFT => Some(taffy::AlignContent::Start),
        stylo::AlignFlags::END | stylo::AlignFlags::RIGHT => Some(taffy::AlignContent::End),
        stylo::AlignFlags::FLEX_START => Some(taffy::AlignContent::FlexStart),
        stylo::AlignFlags::STRETCH => Some(taffy::AlignContent::Stretch),
        stylo::AlignFlags::FLEX_END => Some(taffy::AlignContent::FlexEnd),
        stylo::AlignFlags::CENTER => Some(taffy::AlignContent::Center),
        stylo::AlignFlags::SPACE_BETWEEN => Some(taffy::AlignContent::SpaceBetween),
        stylo::AlignFlags::SPACE_AROUND => Some(taffy::AlignContent::SpaceAround),
        stylo::AlignFlags::SPACE_EVENLY => Some(taffy::AlignContent::SpaceEvenly),
        _ => None,
    }
}

#[inline]
pub fn item_alignment(input: stylo::AlignFlags) -> Option<taffy::AlignItems> {
    match input.value() {
        stylo::AlignFlags::AUTO => None,
        stylo::AlignFlags::NORMAL | stylo::AlignFlags::STRETCH => {
            Some(taffy::AlignItems::Stretch)
        }
        stylo::AlignFlags::FLEX_START => Some(taffy::AlignItems::FlexStart),
        stylo::AlignFlags::FLEX_END => Some(taffy::AlignItems::FlexEnd),
        stylo::AlignFlags::SELF_START | stylo::AlignFlags::START | stylo::AlignFlags::LEFT => {
            Some(taffy::AlignItems::Start)
        }
        stylo::AlignFlags::SELF_END | stylo::AlignFlags::END | stylo::AlignFlags::RIGHT => {
            Some(taffy::AlignItems::End)
        }
        stylo::AlignFlags::CENTER => Some(taffy::AlignItems::Center),
        stylo::AlignFlags::BASELINE => Some(taffy::AlignItems::Baseline),
        _ => None,
    }
}

#[inline]
pub fn gap(input: &stylo::Gap) -> taffy::LengthPercentage {
    match input {
        stylo::Gap::Normal => taffy::LengthPercentage::ZERO,
        stylo::Gap::LengthPercentage(val) => length_percentage(&val.0),
    }
}

// ─── Text ────────────────────────────────────────────────────────────────

#[inline]
pub(crate) fn text_align(input: stylo::TextAlign) -> taffy::TextAlign {
    match input {
        stylo::TextAlign::MozLeft => taffy::TextAlign::LegacyLeft,
        stylo::TextAlign::MozRight => taffy::TextAlign::LegacyRight,
        stylo::TextAlign::MozCenter => taffy::TextAlign::LegacyCenter,
        _ => taffy::TextAlign::Auto,
    }
}

// ─── Flexbox ─────────────────────────────────────────────────────────────

#[inline]
pub fn flex_basis(input: &stylo::FlexBasis) -> taffy::Dimension {
    match input {
        stylo::FlexBasis::Content => taffy::Dimension::AUTO,
        stylo::FlexBasis::Size(size) => dimension(size),
    }
}

#[inline]
pub fn flex_direction(input: stylo::FlexDirection) -> taffy::FlexDirection {
    match input {
        stylo::FlexDirection::Row => taffy::FlexDirection::Row,
        stylo::FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
        stylo::FlexDirection::Column => taffy::FlexDirection::Column,
        stylo::FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
    }
}

#[inline]
pub fn flex_wrap(input: stylo::FlexWrap) -> taffy::FlexWrap {
    match input {
        stylo::FlexWrap::Wrap => taffy::FlexWrap::Wrap,
        stylo::FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
        stylo::FlexWrap::Nowrap => taffy::FlexWrap::NoWrap,
    }
}

// ─── Grid ────────────────────────────────────────────────────────────────

#[inline]
pub fn grid_auto_flow(input: stylo::GridAutoFlow) -> taffy::GridAutoFlow {
    let is_row = input.contains(stylo::GridAutoFlow::ROW);
    let is_dense = input.contains(stylo::GridAutoFlow::DENSE);
    match (is_row, is_dense) {
        (true, false) => taffy::GridAutoFlow::Row,
        (true, true) => taffy::GridAutoFlow::RowDense,
        (false, false) => taffy::GridAutoFlow::Column,
        (false, true) => taffy::GridAutoFlow::ColumnDense,
    }
}

#[inline]
pub fn grid_line(input: &stylo::GridLine) -> taffy::GridPlacement<Atom> {
    if input.is_auto() {
        taffy::GridPlacement::Auto
    } else if input.is_span {
        if input.ident.0 != stylo::atom!("") {
            taffy::GridPlacement::NamedSpan(
                input.ident.0.clone(),
                input.line_num.try_into().unwrap(),
            )
        } else {
            taffy::GridPlacement::Span(input.line_num as u16)
        }
    } else if input.ident.0 != stylo::atom!("") {
        taffy::GridPlacement::NamedLine(input.ident.0.clone(), input.line_num as i16)
    } else if input.line_num != 0 {
        taffy::style_helpers::line(input.line_num as i16)
    } else {
        taffy::GridPlacement::Auto
    }
}

#[inline]
pub fn grid_auto_tracks(input: &stylo::ImplicitGridTracks) -> Vec<taffy::TrackSizingFunction> {
    input.0.iter().map(track_size).collect()
}

#[inline]
pub fn track_repeat(input: stylo::RepeatCount<i32>) -> taffy::RepetitionCount {
    match input {
        stylo::RepeatCount::Number(val) => taffy::RepetitionCount::Count(val.try_into().unwrap()),
        stylo::RepeatCount::AutoFill => taffy::RepetitionCount::AutoFill,
        stylo::RepeatCount::AutoFit => taffy::RepetitionCount::AutoFit,
    }
}

#[inline]
pub fn track_size(
    input: &stylo::TrackSize<stylo::LengthPercentage>,
) -> taffy::TrackSizingFunction {
    match input {
        stylo::TrackSize::Breadth(breadth) => taffy::MinMax {
            min: min_track(breadth),
            max: max_track(breadth),
        },
        stylo::TrackSize::Minmax(min, max) => taffy::MinMax {
            min: min_track(min),
            max: max_track(max),
        },
        stylo::TrackSize::FitContent(limit) => taffy::MinMax {
            min: taffy::MinTrackSizingFunction::AUTO,
            max: match limit {
                stylo::TrackBreadth::Breadth(lp) => {
                    taffy::MaxTrackSizingFunction::fit_content(length_percentage(lp))
                }
                // fit-content with non-length limit — fall back to auto
                _ => taffy::MaxTrackSizingFunction::AUTO,
            },
        },
    }
}

#[inline]
pub fn min_track(
    input: &stylo::TrackBreadth<stylo::LengthPercentage>,
) -> taffy::MinTrackSizingFunction {
    match input {
        stylo::TrackBreadth::Breadth(lp) => {
            taffy::MinTrackSizingFunction::from(length_percentage(lp))
        }
        stylo::TrackBreadth::Fr(_) => taffy::MinTrackSizingFunction::AUTO,
        stylo::TrackBreadth::Auto => taffy::MinTrackSizingFunction::AUTO,
        stylo::TrackBreadth::MinContent => taffy::MinTrackSizingFunction::MIN_CONTENT,
        stylo::TrackBreadth::MaxContent => taffy::MinTrackSizingFunction::MAX_CONTENT,
    }
}

#[inline]
pub fn max_track(
    input: &stylo::TrackBreadth<stylo::LengthPercentage>,
) -> taffy::MaxTrackSizingFunction {
    match input {
        stylo::TrackBreadth::Breadth(lp) => {
            taffy::MaxTrackSizingFunction::from(length_percentage(lp))
        }
        stylo::TrackBreadth::Fr(val) => taffy::MaxTrackSizingFunction::from_fr(*val),
        stylo::TrackBreadth::Auto => taffy::MaxTrackSizingFunction::AUTO,
        stylo::TrackBreadth::MinContent => taffy::MaxTrackSizingFunction::MIN_CONTENT,
        stylo::TrackBreadth::MaxContent => taffy::MaxTrackSizingFunction::MAX_CONTENT,
    }
}

#[inline]
pub fn grid_template_tracks(
    input: &stylo::GridTemplateComponent,
) -> Vec<taffy::GridTemplateComponent<Atom>> {
    match input {
        stylo::GenericGridTemplateComponent::None => Vec::new(),
        stylo::GenericGridTemplateComponent::TrackList(list) => list
            .values
            .iter()
            .map(|track| match track {
                stylo::TrackListValue::TrackSize(size) => {
                    taffy::GridTemplateComponent::Single(track_size(size))
                }
                stylo::TrackListValue::TrackRepeat(repeat) => {
                    taffy::GridTemplateComponent::Repeat(taffy::GridTemplateRepetition {
                        count: track_repeat(repeat.count),
                        tracks: repeat.track_sizes.iter().map(track_size).collect(),
                        line_names: repeat
                            .line_names
                            .iter()
                            .map(|line_name_set| {
                                line_name_set.iter().map(|ident| ident.0.clone()).collect()
                            })
                            .collect(),
                    })
                }
            })
            .collect(),
        stylo::GenericGridTemplateComponent::Subgrid(_)
        | stylo::GenericGridTemplateComponent::Masonry => Vec::new(),
    }
}

fn grid_template_areas(
    input: &stylo::GridTemplateAreas,
) -> Vec<taffy::GridTemplateArea<Atom>> {
    match input {
        stylo::GridTemplateAreas::None => Vec::new(),
        stylo::GridTemplateAreas::Areas(template_areas_arc) => template_areas_arc
            .0
            .areas
            .iter()
            .map(|area| taffy::GridTemplateArea {
                name: area.name.clone(),
                row_start: area.rows.start as u16,
                row_end: area.rows.end as u16,
                column_start: area.columns.start as u16,
                column_end: area.columns.end as u16,
            })
            .collect(),
    }
}

// ─── Main conversion function ────────────────────────────────────────────

/// Eagerly convert Stylo `ComputedValues` into a `taffy::Style`.
pub fn to_taffy_style(style: &stylo::ComputedValues) -> taffy::Style<Atom> {
    let cv_display = style.clone_display();
    let pos = style.get_position();
    let cv_margin = style.get_margin();
    let padding = style.get_padding();
    let cv_border = style.get_border();

    taffy::Style {
        dummy: core::marker::PhantomData,
        display: self::display(cv_display),
        box_sizing: self::box_sizing(style.clone_box_sizing()),
        item_is_table: cv_display.inside() == stylo::DisplayInside::Table,
        item_is_replaced: false,
        position: self::position(style.clone_position()),
        overflow: taffy::Point {
            x: self::overflow(style.clone_overflow_x()),
            y: self::overflow(style.clone_overflow_y()),
        },
        scrollbar_width: 0.0,

        size: taffy::Size {
            width: self::dimension(&pos.width),
            height: self::dimension(&pos.height),
        },
        min_size: taffy::Size {
            width: self::dimension(&pos.min_width),
            height: self::dimension(&pos.min_height),
        },
        max_size: taffy::Size {
            width: self::max_size_dimension(&pos.max_width),
            height: self::max_size_dimension(&pos.max_height),
        },
        aspect_ratio: self::aspect_ratio(pos.aspect_ratio),

        inset: taffy::Rect {
            left: self::inset(&pos.left),
            right: self::inset(&pos.right),
            top: self::inset(&pos.top),
            bottom: self::inset(&pos.bottom),
        },
        margin: taffy::Rect {
            left: self::margin(&cv_margin.margin_left),
            right: self::margin(&cv_margin.margin_right),
            top: self::margin(&cv_margin.margin_top),
            bottom: self::margin(&cv_margin.margin_bottom),
        },
        padding: taffy::Rect {
            left: self::length_percentage(&padding.padding_left.0),
            right: self::length_percentage(&padding.padding_right.0),
            top: self::length_percentage(&padding.padding_top.0),
            bottom: self::length_percentage(&padding.padding_bottom.0),
        },
        border: taffy::Rect {
            left: self::border(&cv_border.border_left_width, cv_border.border_left_style),
            right: self::border(&cv_border.border_right_width, cv_border.border_right_style),
            top: self::border(&cv_border.border_top_width, cv_border.border_top_style),
            bottom: self::border(&cv_border.border_bottom_width, cv_border.border_bottom_style),
        },

        // Gap
        gap: taffy::Size {
            width: self::gap(&pos.column_gap),
            height: self::gap(&pos.row_gap),
        },

        // Alignment
        align_content: self::content_alignment(pos.align_content),
        justify_content: self::content_alignment(pos.justify_content),
        align_items: self::item_alignment(pos.align_items.0),
        align_self: self::item_alignment(pos.align_self.0),
        justify_items: self::item_alignment((pos.justify_items.computed.0).0),
        justify_self: self::item_alignment(pos.justify_self.0),
        text_align: self::text_align(style.clone_text_align()),

        // Flexbox
        flex_direction: self::flex_direction(pos.flex_direction),
        flex_wrap: self::flex_wrap(pos.flex_wrap),
        flex_grow: pos.flex_grow.0,
        flex_shrink: pos.flex_shrink.0,
        flex_basis: self::flex_basis(&pos.flex_basis),

        // Grid
        grid_auto_flow: self::grid_auto_flow(pos.grid_auto_flow),
        grid_template_rows: self::grid_template_tracks(&pos.grid_template_rows),
        grid_template_columns: self::grid_template_tracks(&pos.grid_template_columns),
        grid_template_row_names: {
            match &pos.grid_template_rows {
                stylo::GenericGridTemplateComponent::TrackList(list) => list
                    .line_names
                    .iter()
                    .map(|names| names.iter().map(|ident| ident.0.clone()).collect())
                    .collect(),
                _ => Vec::new(),
            }
        },
        grid_template_column_names: {
            match &pos.grid_template_columns {
                stylo::GenericGridTemplateComponent::TrackList(list) => list
                    .line_names
                    .iter()
                    .map(|names| names.iter().map(|ident| ident.0.clone()).collect())
                    .collect(),
                _ => Vec::new(),
            }
        },
        grid_template_areas: self::grid_template_areas(&pos.grid_template_areas),
        grid_auto_rows: self::grid_auto_tracks(&pos.grid_auto_rows),
        grid_auto_columns: self::grid_auto_tracks(&pos.grid_auto_columns),
        grid_row: taffy::Line {
            start: self::grid_line(&pos.grid_row_start),
            end: self::grid_line(&pos.grid_row_end),
        },
        grid_column: taffy::Line {
            start: self::grid_line(&pos.grid_column_start),
            end: self::grid_line(&pos.grid_column_end),
        },
    }
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
