use super::*;

// ─── Generic font family conversion ──────────────────────────────────────

#[test]
fn generic_family_serif() {
    assert!(matches!(
        generic_font_family(stylo::GenericFontFamily::Serif),
        parley::style::GenericFamily::Serif
    ));
}

#[test]
fn generic_family_sans_serif() {
    assert!(matches!(
        generic_font_family(stylo::GenericFontFamily::SansSerif),
        parley::style::GenericFamily::SansSerif
    ));
}

#[test]
fn generic_family_monospace() {
    assert!(matches!(
        generic_font_family(stylo::GenericFontFamily::Monospace),
        parley::style::GenericFamily::Monospace
    ));
}

#[test]
fn generic_family_cursive() {
    assert!(matches!(
        generic_font_family(stylo::GenericFontFamily::Cursive),
        parley::style::GenericFamily::Cursive
    ));
}

#[test]
fn generic_family_fantasy() {
    assert!(matches!(
        generic_font_family(stylo::GenericFontFamily::Fantasy),
        parley::style::GenericFamily::Fantasy
    ));
}

#[test]
fn generic_family_system_ui() {
    assert!(matches!(
        generic_font_family(stylo::GenericFontFamily::SystemUi),
        parley::style::GenericFamily::SystemUi
    ));
}

#[test]
fn generic_family_none_defaults_to_sans_serif() {
    assert!(matches!(
        generic_font_family(stylo::GenericFontFamily::None),
        parley::style::GenericFamily::SansSerif
    ));
}

// ─── Font weight ─────────────────────────────────────────────────────────

#[test]
fn font_weight_normal() {
    let w = font_weight(stylo::FontWeight::normal());
    assert_eq!(w.value(), 400.0);
}

#[test]
fn font_weight_bold() {
    let w = font_weight(stylo::FontWeight::BOLD);
    assert_eq!(w.value(), 700.0);
}

// ─── Font style ──────────────────────────────────────────────────────────

#[test]
fn font_style_normal() {
    assert!(matches!(
        font_style(stylo::FontStyle::NORMAL),
        parley::style::FontStyle::Normal
    ));
}

#[test]
fn font_style_italic() {
    assert!(matches!(
        font_style(stylo::FontStyle::ITALIC),
        parley::style::FontStyle::Italic
    ));
}


// ─── Full style conversion ───────────────────────────────────────────────

#[test]
fn to_text_style_uses_initial_values() {
    // Use Stylo's initial ComputedValues (default for unstyled elements)
    let font = style::properties::style_structs::Font::initial_values();
    let computed = style::properties::ComputedValues::initial_values_with_font_override(font);
    let style = to_text_style(42, &computed);

    // Should have default font size (16px from initial values)
    assert!(style.font_size > 0.0, "font_size should be > 0");
    // Brush should carry the span ID
    assert_eq!(style.brush.id, 42);
}

// ─── Query font family ──────────────────────────────────────────────────

#[test]
fn query_font_family_generic() {
    let family = stylo::SingleFontFamily::Generic(stylo::GenericFontFamily::Monospace);
    let result = query_font_family(&family);
    assert!(matches!(
        result,
        parley::fontique::QueryFamily::Generic(parley::style::GenericFamily::Monospace)
    ));
}
