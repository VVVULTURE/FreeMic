//! Small vector icons, painted with lines and shapes so they look the same on
//! every system and scale cleanly (no icon font needed).

use std::f32::consts::{PI, TAU};

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Plus,
    Play,
    Stop,
    Loop,
    Gear,
    Mic,
    MicOff,
    Speaker,
    Search,
    Download,
    Grid,
    Close,
    Check,
    Warning,
    Music,
    Folder,
}

/// Paint `icon` centered in `rect` (drawn on a 16×16 grid, scaled to the rect).
pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 16.0;
    let o = rect.center() - vec2(8.0, 8.0) * s;
    let at = |x: f32, y: f32| o + vec2(x, y) * s;
    let w = (1.6 * s).max(1.0);
    let line = Stroke::new(w, color);
    match icon {
        Icon::Plus => {
            p.line_segment([at(8.0, 3.0), at(8.0, 13.0)], line);
            p.line_segment([at(3.0, 8.0), at(13.0, 8.0)], line);
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![at(4.5, 2.5), at(13.0, 8.0), at(4.5, 13.5)], color, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_min_max(at(3.5, 3.5), at(12.5, 12.5)), 1.5 * s, color);
        }
        Icon::Loop => {
            // Two arcs with arrow heads.
            arc(p, at(8.0, 8.0), 5.0 * s, -PI * 0.95, -PI * 0.15, line);
            arc(p, at(8.0, 8.0), 5.0 * s, PI * 0.05, PI * 0.85, line);
            let head = |tip: Pos2, dir: Vec2| {
                let n = vec2(-dir.y, dir.x);
                p.add(Shape::convex_polygon(vec![tip + dir * 2.6 * s, tip + n * 2.2 * s, tip - n * 2.2 * s], color, Stroke::NONE));
            };
            let a1 = -PI * 0.15;
            head(at(8.0, 8.0) + vec2(a1.cos(), a1.sin()) * 5.0 * s, vec2(-a1.sin(), a1.cos()));
            let a2 = PI * 0.85;
            head(at(8.0, 8.0) + vec2(a2.cos(), a2.sin()) * 5.0 * s, vec2(-a2.sin(), a2.cos()));
        }
        Icon::Gear => {
            let c = at(8.0, 8.0);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * 4.0 * s, c + d * 6.6 * s], Stroke::new(2.2 * s, color));
            }
            p.circle_stroke(c, 4.4 * s, line);
            p.circle_filled(c, 1.6 * s, color);
        }
        Icon::Mic | Icon::MicOff => {
            p.rect_stroke(
                Rect::from_min_max(at(5.75, 1.5), at(10.25, 10.0)),
                2.25 * s,
                line,
                eframe::egui::StrokeKind::Middle,
            );
            arc(p, at(8.0, 7.5), 4.6 * s, 0.0, PI, line);
            p.line_segment([at(8.0, 12.1), at(8.0, 14.5)], line);
            p.line_segment([at(5.5, 14.5), at(10.5, 14.5)], line);
            if icon == Icon::MicOff {
                p.line_segment([at(2.5, 2.5), at(13.5, 13.5)], Stroke::new(w * 1.1, color));
            }
        }
        Icon::Speaker => {
            p.add(Shape::convex_polygon(
                vec![at(2.0, 6.0), at(5.0, 6.0), at(9.0, 2.5), at(9.0, 13.5), at(5.0, 10.0), at(2.0, 10.0)],
                color,
                Stroke::NONE,
            ));
            arc(p, at(9.0, 8.0), 3.0 * s, -PI * 0.3, PI * 0.3, line);
            arc(p, at(9.0, 8.0), 5.6 * s, -PI * 0.3, PI * 0.3, line);
        }
        Icon::Search => {
            p.circle_stroke(at(7.0, 7.0), 4.4 * s, line);
            p.line_segment([at(10.3, 10.3), at(14.0, 14.0)], Stroke::new(w * 1.2, color));
        }
        Icon::Download => {
            p.line_segment([at(8.0, 2.0), at(8.0, 10.5)], line);
            p.line_segment([at(4.5, 7.0), at(8.0, 10.5)], line);
            p.line_segment([at(11.5, 7.0), at(8.0, 10.5)], line);
            p.line_segment([at(3.0, 13.5), at(13.0, 13.5)], line);
        }
        Icon::Grid => {
            for (x, y) in [(2.5, 2.5), (9.0, 2.5), (2.5, 9.0), (9.0, 9.0)] {
                p.rect_filled(Rect::from_min_size(at(x, y), vec2(4.5, 4.5) * s), 1.2 * s, color);
            }
        }
        Icon::Close => {
            p.line_segment([at(3.5, 3.5), at(12.5, 12.5)], line);
            p.line_segment([at(12.5, 3.5), at(3.5, 12.5)], line);
        }
        Icon::Check => {
            p.add(Shape::line(vec![at(3.0, 8.5), at(6.5, 12.0), at(13.0, 4.5)], line));
        }
        Icon::Warning => {
            p.add(Shape::closed_line(vec![at(8.0, 2.0), at(14.5, 13.5), at(1.5, 13.5)], line));
            p.line_segment([at(8.0, 6.0), at(8.0, 9.5)], line);
            p.circle_filled(at(8.0, 11.5), 0.9 * s, color);
        }
        Icon::Music => {
            p.circle_filled(at(5.0, 12.0), 2.3 * s, color);
            p.circle_filled(at(12.0, 10.5), 2.3 * s, color);
            p.line_segment([at(7.1, 12.0), at(7.1, 3.5)], line);
            p.line_segment([at(14.1, 10.5), at(14.1, 2.0)], line);
            p.line_segment([at(7.1, 3.5), at(14.1, 2.0)], Stroke::new(w * 1.4, color));
        }
        Icon::Folder => {
            p.add(Shape::closed_line(
                vec![at(1.5, 3.5), at(6.0, 3.5), at(7.5, 5.0), at(14.5, 5.0), at(14.5, 13.0), at(1.5, 13.0)],
                line,
            ));
        }
    }
}

fn arc(p: &Painter, c: Pos2, r: f32, from: f32, to: f32, stroke: Stroke) {
    let n = 16;
    let pts = (0..=n).map(|i| {
        let a = from + (to - from) * i as f32 / n as f32;
        c + vec2(a.cos(), a.sin()) * r
    });
    p.add(Shape::line(pts.collect(), stroke));
}

