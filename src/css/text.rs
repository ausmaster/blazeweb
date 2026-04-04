//! Stylo → Parley font/text property conversion.
//!
//! Ported from Blitz's `stylo_to_parley.rs`. Converts Stylo computed values
//! into Parley text styles for real text measurement.

use std::borrow::Cow;

use style::values::computed::Length;

/// Type aliases for Stylo computed types.
pub(crate) mod stylo {
    pub(crate) use style::computed_values::text_wrap_mode::T as TextWrapMode;
    pub(crate) use style::properties::ComputedValues;
    pub(crate) use style::values::computed::OverflowWrap;
    pub(crate) use style::values::computed::WordBreak;
    pub(crate) use style::values::computed::font::FontStretch;
    pub(crate) use style::values::computed::font::FontStyle;
    pub(crate) use style::values::computed::font::FontVariationSettings;
    pub(crate) use style::values::computed::font::FontWeight;
    pub(crate) use style::values::computed::font::GenericFontFamily;
    pub(crate) use style::values::computed::font::LineHeight;
    pub(crate) use style::values::computed::font::SingleFontFamily;
}

/// Our text brush — carries a span ID (node index) for lazy style lookups.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextBrush {
    pub id: usize,
}

impl TextBrush {
    pub fn from_id(id: usize) -> Self {
        Self { id }
    }
}

// Parley requires Brush to be Clone + PartialEq + Default + Debug (all derived above)

// ─── Individual property conversions ──────────────────────────────────────

pub fn generic_font_family(input: stylo::GenericFontFamily) -> parley::style::GenericFamily {
    match input {
        stylo::GenericFontFamily::None => parley::style::GenericFamily::SansSerif,
        stylo::GenericFontFamily::Serif => parley::style::GenericFamily::Serif,
        stylo::GenericFontFamily::SansSerif => parley::style::GenericFamily::SansSerif,
        stylo::GenericFontFamily::Monospace => parley::style::GenericFamily::Monospace,
        stylo::GenericFontFamily::Cursive => parley::style::GenericFamily::Cursive,
        stylo::GenericFontFamily::Fantasy => parley::style::GenericFamily::Fantasy,
        stylo::GenericFontFamily::SystemUi => parley::style::GenericFamily::SystemUi,
    }
}

pub fn query_font_family(input: &stylo::SingleFontFamily) -> parley::fontique::QueryFamily<'_> {
    match input {
        stylo::SingleFontFamily::FamilyName(name) => {
            parley::fontique::QueryFamily::Named(name.name.as_ref())
        }
        stylo::SingleFontFamily::Generic(generic) => {
            parley::fontique::QueryFamily::Generic(generic_font_family(*generic))
        }
    }
}

pub fn font_weight(input: stylo::FontWeight) -> parley::style::FontWeight {
    parley::style::FontWeight::new(input.value())
}

pub fn font_width(input: stylo::FontStretch) -> parley::style::FontWidth {
    parley::style::FontWidth::from_percentage(input.0.to_float())
}

pub fn font_style(input: stylo::FontStyle) -> parley::style::FontStyle {
    match input {
        stylo::FontStyle::NORMAL => parley::style::FontStyle::Normal,
        stylo::FontStyle::ITALIC => parley::style::FontStyle::Italic,
        val => parley::style::FontStyle::Oblique(Some(val.oblique_degrees())),
    }
}

pub fn font_variations(input: &stylo::FontVariationSettings) -> Vec<parley::FontVariation> {
    input
        .0
        .iter()
        .map(|v| parley::FontVariation {
            tag: parley::setting::Tag::from_bytes(v.tag.0.to_be_bytes()),
            value: v.value,
        })
        .collect()
}

// ─── Full TextStyle conversion ────────────────────────────────────────────

/// Convert Stylo ComputedValues into a Parley TextStyle for text measurement.
pub fn to_text_style(
    span_id: usize,
    style: &stylo::ComputedValues,
) -> parley::style::TextStyle<'static, 'static, TextBrush> {
    let font_styles = style.get_font();
    let itext_styles = style.get_inherited_text();

    let font_size = font_styles.font_size.used_size.0.px();
    let line_height = match font_styles.line_height {
        stylo::LineHeight::Normal => parley::style::LineHeight::FontSizeRelative(1.2),
        stylo::LineHeight::Number(num) => parley::style::LineHeight::FontSizeRelative(num.0),
        stylo::LineHeight::Length(value) => parley::style::LineHeight::Absolute(value.0.px()),
    };

    let letter_spacing = itext_styles
        .letter_spacing
        .0
        .resolve(Length::new(font_size))
        .px();

    let font_wt = self::font_weight(font_styles.font_weight);
    let font_st = self::font_style(font_styles.font_style);
    let font_wi = self::font_width(font_styles.font_stretch);
    let font_var = self::font_variations(&font_styles.font_variation_settings);

    let families: Vec<_> = font_styles
        .font_family
        .families
        .list
        .iter()
        .map(|family| match family {
            stylo::SingleFontFamily::FamilyName(name) => {
                parley::style::FontFamilyName::Named(Cow::Owned(name.name.as_ref().to_string()))
            }
            stylo::SingleFontFamily::Generic(generic) => {
                parley::style::FontFamilyName::Generic(generic_font_family(*generic))
            }
        })
        .collect();

    let word_break = match itext_styles.word_break {
        stylo::WordBreak::Normal => parley::style::WordBreak::Normal,
        stylo::WordBreak::BreakAll => parley::style::WordBreak::BreakAll,
        stylo::WordBreak::KeepAll => parley::style::WordBreak::KeepAll,
    };
    let overflow_wrap = match itext_styles.overflow_wrap {
        stylo::OverflowWrap::Normal => parley::style::OverflowWrap::Normal,
        stylo::OverflowWrap::BreakWord => parley::style::OverflowWrap::BreakWord,
        stylo::OverflowWrap::Anywhere => parley::style::OverflowWrap::Anywhere,
    };
    let text_wrap_mode = match itext_styles.text_wrap_mode {
        stylo::TextWrapMode::Wrap => parley::style::TextWrapMode::Wrap,
        stylo::TextWrapMode::Nowrap => parley::style::TextWrapMode::NoWrap,
    };

    log::trace!(
        "to_text_style(span={}): size={:.1}px weight={} families={}",
        span_id,
        font_size,
        font_wt.value(),
        families.len()
    );

    parley::style::TextStyle {
        font_family: parley::style::FontFamily::List(Cow::Owned(families)),
        font_size,
        font_width: font_wi,
        font_style: font_st,
        font_weight: font_wt,
        font_variations: parley::style::FontVariations::List(Cow::Owned(font_var)),
        font_features: parley::style::FontFeatures::List(Cow::Borrowed(&[])),
        locale: Default::default(),
        line_height,
        word_spacing: Default::default(),
        letter_spacing,
        text_wrap_mode,
        overflow_wrap,
        word_break,
        brush: TextBrush::from_id(span_id),
        has_underline: Default::default(),
        underline_offset: Default::default(),
        underline_size: Default::default(),
        underline_brush: Default::default(),
        has_strikethrough: Default::default(),
        strikethrough_offset: Default::default(),
        strikethrough_size: Default::default(),
        strikethrough_brush: Default::default(),
    }
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
