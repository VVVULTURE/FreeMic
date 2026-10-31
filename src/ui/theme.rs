//! Colors, fonts and widget styling. FreeMic has its own look (warm graphite
//! with a coral accent) in both a dark and a light variant.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Vec2,
    epaint::text::{FontTweak, VariationCoords},
};

/// Colors that are not part of egui's own `Visuals`.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
    /// Sound pads without a color of their own.
    pub pad: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(21, 22, 26),
    surface: Color32::from_rgb(30, 32, 37),
    raised: Color32::from_rgb(40, 43, 50),
    border: Color32::from_rgb(52, 56, 64),
    text: Color32::from_rgb(234, 231, 226),
    dim: Color32::from_rgb(160, 163, 171),
    faint: Color32::from_rgb(112, 117, 126),
    accent: Color32::from_rgb(240, 122, 74),
    on_accent: Color32::from_rgb(28, 18, 12),
    good: Color32::from_rgb(82, 196, 138),
    warn: Color32::from_rgb(232, 170, 66),
    bad: Color32::from_rgb(234, 96, 84),
    pad: Color32::from_rgb(46, 49, 57),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(245, 243, 239),
    surface: Color32::from_rgb(255, 255, 255),
    raised: Color32::from_rgb(236, 233, 228),
    border: Color32::from_rgb(218, 214, 207),
    text: Color32::from_rgb(31, 31, 35),
    dim: Color32::from_rgb(92, 95, 102),
    faint: Color32::from_rgb(134, 137, 144),
    accent: Color32::from_rgb(214, 86, 40),
    on_accent: Color32::WHITE,
    good: Color32::from_rgb(36, 150, 98),
    warn: Color32::from_rgb(178, 112, 10),
    bad: Color32::from_rgb(200, 58, 48),
    pad: Color32::from_rgb(232, 229, 223),
};

pub fn pal(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode { &DARK } else { &LIGHT }
}

/// Pad colors offered in the sound menu. They work with white text in both themes.
pub const PAD_COLORS: &[[u8; 3]] = &[
    [74, 108, 178],  // slate blue
    [36, 132, 134],  // teal
    [52, 136, 88],   // green
    [156, 104, 22],  // amber
    [196, 88, 64],   // coral
    [180, 76, 116],  // rose
    [122, 90, 180],  // violet
    [94, 98, 108],   // graphite
];

/// Text color that stays readable on `bg` (WCAG relative luminance).
pub fn text_on(bg: Color32) -> Color32 {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * lin(bg.r()) + 0.7152 * lin(bg.g()) + 0.0722 * lin(bg.b());
    // Contrast against white vs. against near-black (#1C1C20, L = 0.0116).
    if (1.05 / (l + 0.05)) >= ((l + 0.05) / 0.0616) { Color32::WHITE } else { Color32::from_rgb(28, 28, 32) }
}

/// Mix two colors (t = 0 → a, t = 1 → b).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn font_bold(size: f32) -> FontId {
    FontId::new(size, semibold())
}

/// Text style for small captions and section descriptions.
pub fn caption() -> TextStyle {
    TextStyle::Small
}

static INTER: &[u8] = include_bytes!("../../assets/fonts/InterVariable.ttf");

/// Inter (SIL OFL 1.1) as the UI font: one variable font file used at two weights.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let weight = |w: f32| FontTweak { coords: VariationCoords::new([("wght", w)]), ..Default::default() };
    fonts.font_data.insert("inter".into(), Arc::new(FontData::from_static(INTER).tweak(weight(430.0))));
    fonts.font_data.insert("inter-semibold".into(), Arc::new(FontData::from_static(INTER).tweak(weight(600.0))));
    // egui's bundled fonts stay as fallbacks for symbols, emoji and scripts Inter lacks.
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut regular = vec!["inter".to_string()];
    regular.extend(fallbacks.iter().cloned());
    let mut bold = vec!["inter-semibold".to_string()];
    bold.extend(fallbacks);
    fonts.families.insert(FontFamily::Proportional, regular);
    fonts.families.insert(semibold(), bold);
    ctx.set_fonts(fonts);
}

/// Install fonts and both theme variants.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_style_of(egui::Theme::Dark, style(&DARK, true));
    ctx.set_style_of(egui::Theme::Light, style(&LIGHT, false));
}

fn style(p: &Palette, dark: bool) -> egui::Style {
    let mut s = egui::Style::default();
    s.text_styles = [
        (TextStyle::Heading, font_bold(19.0)),
        (TextStyle::Body, font(14.0)),
        (TextStyle::Button, font(14.0)),
        (TextStyle::Small, font(12.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();

    let sp = &mut s.spacing;
    sp.item_spacing = Vec2::new(8.0, 8.0);
    sp.button_padding = Vec2::new(12.0, 6.0);
    sp.interact_size = Vec2::new(32.0, 30.0);
    sp.slider_width = 140.0;
    sp.slider_rail_height = 4.0;
    sp.combo_height = 300.0;
    sp.menu_margin = Margin::same(6);
    sp.window_margin = Margin::same(18);
    sp.icon_width = 16.0;
    sp.icon_width_inner = 8.0;
    sp.icon_spacing = 8.0;
    sp.tooltip_width = 320.0;
    sp.scroll.floating = false;
    sp.scroll.bar_width = 8.0;

    s.interaction.selectable_labels = false;

    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    let shadow_alpha = if dark { 110 } else { 40 };
    v.window_shadow = Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(shadow_alpha) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(shadow_alpha) };
    v.extreme_bg_color = if dark { Color32::from_rgb(17, 18, 21) } else { Color32::WHITE };
    v.faint_bg_color = p.surface;
    v.code_bg_color = p.raised;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.bad;
    v.hyperlink_color = p.accent;
    v.override_text_color = None;
    v.weak_text_color = Some(p.dim);
    v.selection.bg_fill = mix(p.accent, p.bg, 0.55);
    v.selection.stroke = Stroke::new(1.5, p.accent);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    v.indent_has_left_vline = false;

    let r = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.weak_bg_fill = p.raised; // button background
    w.inactive.bg_fill = if dark { Color32::from_rgb(58, 62, 71) } else { Color32::from_rgb(214, 210, 203) }; // slider rail, checkbox
    // Outlines text fields, check boxes and drop-downs (needed on white cards in the light theme).
    w.inactive.bg_stroke = Stroke::new(1.0, if dark { p.border } else { mix(p.border, p.text, 0.12) });
    w.inactive.fg_stroke = Stroke::new(1.5, p.text);
    w.inactive.corner_radius = r;

    let hover = mix(p.raised, p.text, 0.08);
    w.hovered.weak_bg_fill = hover;
    w.hovered.bg_fill = hover;
    w.hovered.bg_stroke = Stroke::new(1.0, mix(p.border, p.text, 0.25));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    let active = mix(p.raised, p.text, 0.14);
    w.active.weak_bg_fill = active;
    w.active.bg_fill = active;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.weak_bg_fill = active;
    w.open.bg_fill = active;
    w.open.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.fg_stroke = Stroke::new(1.5, p.text);
    w.open.corner_radius = r;

    s.visuals = v;
    s
}
