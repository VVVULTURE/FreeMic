//! FreeMic's own widgets: buttons with icons, the navigation tabs, cards,
//! settings rows and helpers for text that has to fit a given width.

use std::sync::Arc;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Galley, Layout, Margin, Rect, Response, RichText, Sense, Stroke, StrokeKind,
    Ui, Vec2, text::LayoutJob, vec2,
};

use super::icons::{self, Icon};
use super::theme::{self, Palette};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Filled with the accent color: the one main action on a screen.
    Primary,
    /// Neutral filled button.
    Secondary,
    /// No background until hovered (toolbars).
    Ghost,
    /// Destructive action.
    Danger,
}

/// Lay out `text` on one line; elide with "…" if it is wider than `max_width`.
pub fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    rows(ui, text, font, color, max_width, 1)
}

/// Lay out `text` wrapped to `max_width`, at most `max_rows` rows, eliding the rest.
pub fn rows(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32, max_rows: usize) -> Arc<Galley> {
    let mut job = LayoutJob::simple(text.to_owned(), font, color, max_width.max(1.0));
    job.wrap.max_rows = max_rows.max(1);
    job.wrap.break_anywhere = max_rows <= 1;
    job.wrap.overflow_character = Some('…');
    ui.painter().layout_job(job)
}

/// Width of `text` in `font` without wrapping.
pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font, Color32::WHITE).size().x
}

/// A button with an optional icon. `label` may be empty for icon-only buttons.
pub fn button(ui: &mut Ui, kind: Kind, icon: Option<Icon>, label: &str) -> Response {
    button_sized(ui, kind, icon, label, None)
}

pub fn button_sized(ui: &mut Ui, kind: Kind, icon: Option<Icon>, label: &str, min_width: Option<f32>) -> Response {
    let p = *theme::pal(ui);
    let font = theme::font_bold(13.5);
    let h = 32.0;
    let pad = if label.is_empty() { 8.0 } else { 13.0 };
    let icon_w = if icon.is_some() { 16.0 } else { 0.0 };
    let gap = if icon.is_some() && !label.is_empty() { 7.0 } else { 0.0 };
    let text_w = if label.is_empty() { 0.0 } else { text_width(ui, label, font.clone()) };
    let mut w = pad * 2.0 + icon_w + gap + text_w;
    if let Some(m) = min_width {
        w = w.max(m);
    }
    let enabled = ui.is_enabled();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered() && enabled;
        let down = resp.is_pointer_button_down_on() && enabled;
        let (mut fill, mut fg, stroke) = match kind {
            Kind::Primary => (p.accent, p.on_accent, Stroke::NONE),
            Kind::Secondary => (p.raised, p.text, Stroke::NONE),
            Kind::Ghost => (Color32::TRANSPARENT, p.text, Stroke::NONE),
            Kind::Danger => (p.bad, Color32::WHITE, Stroke::NONE),
        };
        if hovered {
            fill = match kind {
                Kind::Ghost => p.raised,
                _ => theme::mix(fill, p.text, if ui.visuals().dark_mode { 0.10 } else { 0.06 }),
            };
        }
        if down {
            fill = theme::mix(fill, p.bg, 0.15);
        }
        if !enabled {
            fill = fill.gamma_multiply(0.5);
            fg = fg.gamma_multiply(0.45);
        }
        let painter = ui.painter();
        painter.rect(rect, CornerRadius::same(8), fill, stroke, StrokeKind::Inside);
        if resp.has_focus() {
            painter.rect_stroke(rect.expand(2.0), CornerRadius::same(10), Stroke::new(1.5, p.accent), StrokeKind::Outside);
        }
        let content_w = icon_w + gap + text_w;
        let mut x = rect.center().x - content_w / 2.0;
        if let Some(i) = icon {
            icons::paint(painter, Rect::from_min_size(egui::pos2(x, rect.center().y - 8.0), Vec2::splat(16.0)), i, fg);
            x += icon_w + gap;
        }
        if !label.is_empty() {
            let g = painter.layout_no_wrap(label.to_owned(), font, fg);
            painter.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, fg);
        }
    }
    let info_label = label.to_owned();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &info_label));
    resp
}

/// Navigation tabs. Returns the index that was clicked.
pub fn tabs(ui: &mut Ui, items: &[(Icon, &str)], selected: usize, show_labels: bool) -> Option<usize> {
    let p = *theme::pal(ui);
    let font = theme::font_bold(13.5);
    let mut clicked = None;
    let frame = egui::Frame::new().fill(p.surface).corner_radius(10).inner_margin(Margin::same(3)).stroke(Stroke::new(1.0, p.border));
    frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (i, (icon, label)) in items.iter().enumerate() {
                let tw = if show_labels { text_width(ui, label, font.clone()) + 7.0 } else { 0.0 };
                let (rect, resp) = ui.allocate_exact_size(vec2(16.0 + tw + 22.0, 28.0), Sense::click());
                let sel = i == selected;
                let painter = ui.painter();
                if sel {
                    painter.rect_filled(rect, 7.0, p.raised);
                } else if resp.hovered() {
                    painter.rect_filled(rect, 7.0, theme::mix(p.surface, p.raised, 0.6));
                }
                let fg = if sel { p.text } else { p.dim };
                let x = rect.left() + 11.0;
                icons::paint(painter, Rect::from_min_size(egui::pos2(x, rect.center().y - 7.5), Vec2::splat(15.0)), *icon, if sel { p.accent } else { fg });
                if show_labels {
                    let g = painter.layout_no_wrap(label.to_string(), font.clone(), fg);
                    painter.galley(egui::pos2(x + 16.0 + 7.0, rect.center().y - g.size().y / 2.0), g, fg);
                }
                let resp = if show_labels { resp } else { resp.on_hover_text(*label) };
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, sel, *label));
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
        });
    });
    clicked
}

/// Total width of `tabs(...)`.
pub fn tabs_width(ui: &Ui, items: &[(Icon, &str)], show_labels: bool) -> f32 {
    let font = theme::font_bold(13.5);
    let mut w = 6.0 + 2.0;
    for (_, l) in items {
        let tw = if show_labels { text_width(ui, l, font.clone()) + 7.0 } else { 0.0 };
        w += 16.0 + tw + 22.0 + 2.0;
    }
    w
}

/// A rounded panel with a title (and optional description) for a group of settings.
pub fn card<R>(ui: &mut Ui, title: &str, description: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = *theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(18, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).font(theme::font_bold(15.5)).color(p.text));
            if let Some(d) = description {
                ui.add_space(-4.0);
                ui.label(RichText::new(d).text_style(theme::caption()).color(p.dim));
            }
            ui.add_space(4.0);
            add(ui)
        })
        .inner
}

/// "Label: control" row. Side by side when there is room, stacked otherwise.
pub fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = *theme::pal(ui);
    let label_w = 170.0;
    if ui.available_width() >= 470.0 {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(label_w, 30.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(label_w);
                ui.add_space(6.0);
                ui.add(egui::Label::new(RichText::new(label).color(p.dim)).wrap());
            });
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 30.0), Layout::left_to_right(Align::Center).with_main_wrap(true), add)
                .inner
        })
        .inner
    } else {
        ui.add(egui::Label::new(RichText::new(label).color(p.dim)).wrap());
        ui.add_space(-4.0);
        ui.horizontal_wrapped(add).inner
    }
}

/// Small wrapped hint text.
pub fn hint(ui: &mut Ui, text: impl Into<String>) {
    let p = *theme::pal(ui);
    ui.add(egui::Label::new(RichText::new(text.into()).text_style(theme::caption()).color(p.faint)).wrap());
}

/// A colored dot followed by wrapped text.
pub fn status_line(ui: &mut Ui, color: Color32, text: impl Into<String>) {
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::hover());
        ui.painter().circle_filled(egui::pos2(r.center().x, r.top() + 9.0), 4.0, color);
        ui.add(egui::Label::new(text.into()).wrap());
    });
}

/// Horizontal level meter (peak, linear 0..1) on a dB scale from -60 to 0.
pub fn meter(ui: &mut Ui, p: &Palette, peak: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, p.raised);
    let db = 20.0 * peak.max(1e-5).log10();
    let frac = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    let col = if db > -1.5 { p.bad } else if db > -9.0 { p.warn } else { p.good };
    if frac > 0.0 {
        let mut fill = rect;
        fill.set_width((rect.width() * frac).max(8.0));
        painter.rect_filled(fill, 4.0, col);
    }
}

/// A slider: thin rail, accent fill, round handle. Click, drag, or use the
/// arrow keys when focused (Shift = bigger steps).
pub fn slider(ui: &mut Ui, value: &mut f32, min: f32, max: f32, width: f32) -> Response {
    let p = *theme::pal(ui);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width.max(40.0), 22.0), Sense::click_and_drag());
    let r = 7.0;
    let rail = Rect::from_min_max(egui::pos2(rect.left() + r, rect.center().y - 2.0), egui::pos2(rect.right() - r, rect.center().y + 2.0));
    let before = *value;
    if ui.is_enabled() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on() {
                let t = ((pos.x - rail.left()) / rail.width()).clamp(0.0, 1.0);
                *value = (min + t * (max - min)).round();
            }
        }
        if resp.has_focus() {
            let step = if ui.input(|i| i.modifiers.shift) { 10.0 } else { 1.0 };
            let d = ui.input(|i| {
                (i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp)) as i32 as f32
                    - (i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown)) as i32 as f32
            });
            if d != 0.0 {
                *value = (*value + d * step).clamp(min, max);
            }
        }
    }
    if *value != before {
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
        let x = rail.left() + t * rail.width();
        let enabled = ui.is_enabled();
        let accent = if enabled { p.accent } else { p.faint };
        painter.rect_filled(rail, 2.0, if ui.visuals().dark_mode { theme::mix(p.raised, p.text, 0.08) } else { p.border });
        painter.rect_filled(Rect::from_min_max(rail.min, egui::pos2(x, rail.max.y)), 2.0, accent);
        let hot = (resp.hovered() || resp.dragged()) && enabled;
        let c = egui::pos2(x, rect.center().y);
        painter.circle_filled(c + vec2(0.0, 1.0), r + 1.0, Color32::from_black_alpha(50));
        let knob = if ui.visuals().dark_mode { Color32::from_rgb(244, 241, 236) } else { Color32::WHITE };
        painter.circle(c, if hot { r + 1.0 } else { r }, if enabled { knob } else { p.raised }, Stroke::new(1.0, theme::mix(p.border, accent, 0.4)));
        if resp.has_focus() {
            painter.circle_stroke(c, r + 4.0, Stroke::new(1.5, p.accent));
        }
    }
    let v = *value as f64;
    resp.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), v, ""));
    resp
}

/// Volume slider with a fixed-width percentage after it.
pub fn volume(ui: &mut Ui, value: &mut f32, max: f32, width: f32) -> bool {
    let p = *theme::pal(ui);
    let changed = slider(ui, value, 0.0, max, width).changed();
    let g = one_line(ui, &format!("{:.0}%", *value), theme::font(12.5), p.dim, 40.0);
    let (r, _) = ui.allocate_exact_size(vec2(40.0, 20.0), Sense::hover());
    ui.painter().galley(egui::pos2(r.right() - g.size().x, r.center().y - g.size().y / 2.0), g, p.dim);
    changed
}

/// A capsule with a colored dot and text (status indicator in the header).
pub fn pill(ui: &mut Ui, color: Color32, text: &str) -> Response {
    let p = *theme::pal(ui);
    let font = theme::font_bold(12.5);
    let tw = if text.is_empty() { 0.0 } else { text_width(ui, text, font.clone()) + 6.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(12.0 + 8.0 + tw + 10.0, 26.0), Sense::click());
    let painter = ui.painter();
    let fill = if resp.hovered() { theme::mix(p.surface, p.raised, 0.8) } else { p.surface };
    painter.rect(rect, 13.0, fill, Stroke::new(1.0, theme::mix(p.border, color, 0.35)), StrokeKind::Inside);
    painter.circle_filled(egui::pos2(rect.left() + 14.0, rect.center().y), 4.0, color);
    if !text.is_empty() {
        let g = painter.layout_no_wrap(text.to_owned(), font, p.text);
        painter.galley(egui::pos2(rect.left() + 24.0, rect.center().y - g.size().y / 2.0), g, p.text);
    }
    resp
}

pub fn pill_width(ui: &Ui, text: &str) -> f32 {
    let tw = if text.is_empty() { 0.0 } else { text_width(ui, text, theme::font_bold(12.5)) + 6.0 };
    12.0 + 8.0 + tw + 10.0
}

/// The FreeMic logo mark (accent tile with a microphone).
pub fn logo(ui: &mut Ui, size: f32) {
    let p = *theme::pal(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(rect, size * 0.28, p.accent);
    icons::paint(ui.painter(), rect.shrink(size * 0.2), Icon::Mic, Color32::WHITE);
}
