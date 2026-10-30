//! The FreeMic icon, drawn procedurally (no image files needed at runtime).

use eframe::egui::IconData;

/// Render the icon as RGBA at `size` x `size` (a microphone on a rounded blue tile).
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    let sdf_round_rect = |x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32| {
        let dx = (x - cx).abs() - (hw - r);
        let dy = (y - cy).abs() - (hh - r);
        let ox = dx.max(0.0);
        let oy = dy.max(0.0);
        (ox * ox + oy * oy).sqrt() + dx.max(dy).min(0.0) - r
    };
    for j in 0..size {
        for i in 0..size {
            let (x, y) = (i as f32 + 0.5, j as f32 + 0.5);
            let aa = |d: f32| (0.5 - d * size as f32 / 64.0).clamp(0.0, 1.0);
            // Background tile.
            let bg = aa(sdf_round_rect(x, y, s / 2.0, s / 2.0, s * 0.48, s * 0.48, s * 0.2) / (s / 64.0));
            let mut c = [0.0f32; 4];
            if bg > 0.0 {
                let t = y / s;
                c = [0.23 + 0.10 * t, 0.42 - 0.10 * t, 0.86 - 0.15 * t, bg];
            }
            // Microphone capsule.
            let cap = aa(sdf_round_rect(x, y, s * 0.5, s * 0.38, s * 0.13, s * 0.22, s * 0.13) / (s / 64.0));
            // Holder arc (ring segment below the capsule).
            let dx = x - s * 0.5;
            let dy = y - s * 0.42;
            let ring = ((dx * dx + dy * dy).sqrt() - s * 0.22).abs() - s * 0.03;
            let arc = if dy > 0.0 { aa(ring / (s / 64.0)) } else { 0.0 };
            // Stand.
            let stand = aa(sdf_round_rect(x, y, s * 0.5, s * 0.72, s * 0.03, s * 0.08, s * 0.01) / (s / 64.0));
            let base = aa(sdf_round_rect(x, y, s * 0.5, s * 0.8, s * 0.14, s * 0.03, s * 0.02) / (s / 64.0));
            let fg = cap.max(arc).max(stand).max(base) * bg;
            for k in 0..3 {
                c[k] = c[k] * (1.0 - fg) + 0.97 * fg;
            }
            let o = ((j * size + i) * 4) as usize;
            px[o] = (c[0] * 255.0) as u8;
            px[o + 1] = (c[1] * 255.0) as u8;
            px[o + 2] = (c[2] * 255.0) as u8;
            px[o + 3] = (c[3] * 255.0) as u8;
        }
    }
    px
}

pub fn window_icon() -> IconData {
    IconData { rgba: rgba(64), width: 64, height: 64 }
}
