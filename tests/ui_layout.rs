//! Layout checks for the whole window, rendered headless (no GPU, no audio
//! devices, no global hotkeys) at many window sizes and both themes.
//!
//! For every screen it checks that
//! * no two pieces of visible text overlap,
//! * no text is cut off at the side or runs outside the window,
//! * no two clickable controls overlap, and no text half-covers one.

mod common;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Pos2, Rect, Shape, Vec2};
use freemic::audio::Shared;
use freemic::audio::backend::{BackendKind, DeviceInfo, VirtualMicInfo};
use freemic::audio::engine::{DevStatus, Engine, Status};
use freemic::config::{Board, PlayMode, Settings, Theme};
use freemic::paths::Paths;
use freemic::ui::{FreeMicApp, StartOptions, Targets, View};
use freemic::youtube::{Busy, ToolState, Video};

const LONG_NAME: &str = "A really long sound name that keeps going and going until it cannot possibly fit on one button";
const SIZES: &[(f32, f32)] = &[(480.0, 400.0), (520.0, 420.0), (600.0, 460.0), (699.0, 500.0), (700.0, 500.0), (820.0, 560.0), (1024.0, 700.0), (1280.0, 800.0), (1920.0, 1080.0)];

struct Rig {
    app: FreeMicApp,
    ctx: egui::Context,
    time: f64,
    dir: PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.app.shutdown();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn rig(name: &str, sounds: bool, theme: Theme) -> Rig {
    let dir = std::env::temp_dir().join(format!("freemic-layout-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let paths = Paths { portable: true, config_dir: dir.clone(), data_dir: dir.clone() };
    let mut board = Board::default();
    if sounds {
        let wav = common::fixture("mono_s16_8k.wav");
        let names = [
            "Airhorn",
            "Applause",
            "Drum roll",
            "Sad trombone",
            LONG_NAME,
            "Crickets",
            "Supercalifragilisticexpialidocious",
            "Elevator music",
            "Ding",
            "Wilhelm scream (extended remix, 2026)",
            "日本語のサウンド",
            "x",
            "Boing",
            "Victory fanfare",
        ];
        for (i, n) in names.iter().enumerate() {
            let id = board.add(n.to_string(), wav.clone(), if i % 4 == 3 { PlayMode::Loop } else { PlayMode::Once });
            let s = board.get_mut(id).unwrap();
            s.duration = Some([1.2, 3.0, 64.0, 7.5, 600.0][i % 5]);
            if i % 3 == 0 {
                s.color = Some(freemic::ui::theme::PAD_COLORS[i % freemic::ui::theme::PAD_COLORS.len()]);
            }
            if i % 2 == 0 {
                s.hotkey = Some(["control+shift+F1", "control+alt+shift+KeyM", "F9", "control+shift+Digit7"][i / 2 % 4].to_string());
            }
        }
    }
    let settings = Settings { theme, stop_hotkey: Some("control+shift+F12".into()), ..Settings::default() };
    let shared = Arc::new(Shared::default());
    FreeMicApp::apply_gains(&settings, &shared);
    let engine = Arc::new(Engine::start(BackendKind::Null, FreeMicApp::engine_config(&settings), shared, Arc::new(|| {})));
    let targets: Targets = Arc::default();
    let ctx = egui::Context::default();
    let opts = StartOptions { start_minimized: false, load_warnings: Vec::new(), global_hotkeys: false };
    let app = FreeMicApp::new(&ctx, paths, settings, board, engine, targets, opts);
    Rig { app, ctx, time: 0.0, dir }
}

struct Text {
    text: String,
    ink: Rect,
    clip: Rect,
}

struct Frame {
    texts: Vec<Text>,
    clickable: Vec<(Rect, egui::LayerId)>,
    screen: Rect,
}

impl Rig {
    fn frame(&mut self, size: (f32, f32), ppp: f32) -> Frame {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(size.0, size.1));
        let mut out = None;
        // Several passes (1.2 s) so measured sizes and animations (scroll bars fading) settle.
        for _ in 0..12 {
            self.time += 0.1;
            let mut raw = egui::RawInput { screen_rect: Some(screen), time: Some(self.time), ..Default::default() };
            raw.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
            let app = &mut self.app;
            out = Some(self.ctx.run_ui(raw, |ui| app.draw(ui)));
        }
        let out = out.unwrap();
        let mut texts = Vec::new();
        for cs in &out.shapes {
            collect(&cs.shape, cs.clip_rect, &mut texts);
        }
        let clickable = self.ctx.viewport(|v| {
            let w = &v.prev_pass.widgets;
            let mut r = Vec::new();
            for layer in w.layer_ids() {
                for wr in w.get_layer(layer) {
                    if wr.visible && wr.sense.senses_click() && wr.interact_rect.is_positive() {
                        r.push((wr.interact_rect, layer));
                    }
                }
            }
            r
        });
        Frame { texts, clickable, screen }
    }
}

fn collect(shape: &Shape, clip: Rect, out: &mut Vec<Text>) {
    match shape {
        Shape::Text(t) => {
            let s = t.galley.text().trim();
            if !s.is_empty() && t.galley.rows.iter().any(|r| !r.glyphs.is_empty()) {
                out.push(Text { text: t.galley.text().to_string(), ink: t.visual_bounding_rect(), clip });
            }
        }
        Shape::Vec(v) => v.iter().for_each(|s| collect(s, clip, out)),
        _ => {}
    }
}

fn area(r: Rect) -> f32 {
    if r.is_positive() { r.width() * r.height() } else { 0.0 }
}

/// Problems in one rendered frame; `only` restricts the overlap check to those texts.
fn problems(f: &Frame, only: Option<&HashSet<usize>>) -> Vec<String> {
    let mut errs = Vec::new();
    let visible: Vec<(usize, Rect)> = f
        .texts
        .iter()
        .enumerate()
        .filter(|(i, _)| only.is_none_or(|o| o.contains(i)))
        .map(|(i, t)| (i, t.ink.intersect(t.clip)))
        .filter(|(_, r)| r.is_positive())
        .collect();
    for &(i, vis) in &visible {
        let t = &f.texts[i];
        // A single-line text field scrolls long text sideways; that is expected.
        let in_text_field = t.text == LONG_NAME;
        if !in_text_field && (t.ink.left() < t.clip.left() - 0.5 || t.ink.right() > t.clip.right() + 0.5) {
            errs.push(format!("text cut off at the side: {:?} ink {:?} clip {:?}", t.text, t.ink, t.clip));
        }
        if vis.left() < f.screen.left() - 0.5 || vis.right() > f.screen.right() + 0.5 || vis.bottom() > f.screen.bottom() + 0.5 {
            errs.push(format!("text outside the window: {:?} at {:?}", t.text, vis));
        }
    }
    for (a, &(i, ra)) in visible.iter().enumerate() {
        for &(j, rb) in &visible[a + 1..] {
            let ov = area(ra.intersect(rb));
            if ov > 2.0 {
                errs.push(format!("texts overlap ({ov:.0} px²): {:?} {:?} and {:?} {:?}", f.texts[i].text, ra, f.texts[j].text, rb));
            }
        }
    }
    if only.is_none() {
        for (a, &(ra, la)) in f.clickable.iter().enumerate() {
            for &(rb, lb) in &f.clickable[a + 1..] {
                let inter = ra.intersect(rb);
                let contained = area(inter) >= area(ra).min(area(rb)) - 1.0;
                if la == lb && area(inter) > 1.0 && !contained {
                    errs.push(format!("controls overlap: {ra:?} and {rb:?}"));
                }
            }
        }
        for &(i, vis) in &visible {
            for &(r, _) in &f.clickable {
                let ov = area(vis.intersect(r));
                // Glyph ink may stick out of a widget by under a pixel (e.g. the tail of a final "y").
                let slack = vis.width().max(vis.height()).max(2.0);
                if ov > 2.0 && ov < area(vis) - slack {
                    errs.push(format!("text half-covers a control: {:?} {:?} and control {:?}", f.texts[i].text, vis, r));
                }
            }
        }
    }
    errs
}

fn check(label: &str, f: &Frame, only: Option<&HashSet<usize>>, all: &mut Vec<String>) {
    for e in problems(f, only) {
        all.push(format!("[{label}] {e}"));
    }
}

fn status_without_virtual_mic() -> Status {
    Status {
        output: DevStatus::Error("No virtual cable was found. Install VB-CABLE (see Settings) and click Re-check devices.".into()),
        mic: DevStatus::Running { name: "Microphone (Logitech G432 Gaming Headset with a long name)".into(), rate: 48000, latency_ms: 10.0 },
        monitor: DevStatus::Off,
        virtual_mic: VirtualMicInfo {
            feed: None,
            mic_name: None,
            problem: Some("No virtual audio cable was found on this computer. FreeMic needs one to act as a microphone.".into()),
        },
        mic_notice: None,
    }
}

fn status_running() -> Status {
    Status {
        output: DevStatus::Running { name: "CABLE Input (VB-Audio Virtual Cable)".into(), rate: 48000, latency_ms: 10.0 },
        mic: DevStatus::Running { name: "Microphone (Logitech G432 Gaming Headset)".into(), rate: 48000, latency_ms: 10.0 },
        monitor: DevStatus::Off,
        virtual_mic: VirtualMicInfo {
            feed: Some(DeviceInfo { id: "cable".into(), name: "CABLE Input (VB-Audio Virtual Cable)".into() }),
            mic_name: Some("CABLE Output (VB-Audio Virtual Cable)".into()),
            problem: None,
        },
        mic_notice: None,
    }
}

#[test]
fn board_and_settings_fit_every_window_size() {
    let mut errs = Vec::new();
    for theme in [Theme::Dark, Theme::Light] {
        let mut r = rig(&format!("main-{theme:?}"), true, theme);
        for status in [status_running(), status_without_virtual_mic()] {
            r.app.test_status(Some(status));
            for &size in SIZES {
                for ppp in [1.0, 1.5] {
                    for view in [View::Board, View::YouTube, View::Settings] {
                        r.app.open_view(view);
                        let f = r.frame(size, ppp);
                        assert!(f.texts.len() > 5, "nothing rendered");
                        check(&format!("{theme:?} {view:?} {size:?} @{ppp}"), &f, None, &mut errs);
                    }
                }
            }
        }
    }
    assert!(errs.is_empty(), "{} layout problems:\n{}", errs.len(), errs.join("\n"));
}

#[test]
fn playing_state_fits() {
    let mut errs = Vec::new();
    let mut r = rig("playing", true, Theme::Dark);
    r.app.test_status(Some(status_running()));
    // Sound ids start at 1; #5 has the very long name, #4 loops.
    for id in [5, 4] {
        r.app.test_play(id);
        std::thread::sleep(std::time::Duration::from_millis(300));
        for &size in SIZES {
            let f = r.frame(size, 1.0);
            let shown = f.texts.iter().any(|t| t.text.contains(" / ") || t.text.contains("looping"));
            assert!(shown, "now-playing time not shown for sound {id} at {size:?}");
            check(&format!("playing {id} {size:?}"), &f, None, &mut errs);
        }
        r.app.test_play(id); // toggles it off
    }
    assert!(errs.is_empty(), "{} layout problems:\n{}", errs.len(), errs.join("\n"));
}

#[test]
fn empty_board_fits() {
    let mut errs = Vec::new();
    let mut r = rig("empty", false, Theme::Light);
    for status in [status_running(), status_without_virtual_mic()] {
        r.app.test_status(Some(status));
        for &size in SIZES {
            let f = r.frame(size, 1.0);
            assert!(f.texts.iter().any(|t| t.text == "Your soundboard is empty"), "{:?}", f.texts.iter().map(|t| &t.text).collect::<Vec<_>>());
            check(&format!("empty {size:?}"), &f, None, &mut errs);
        }
    }
    assert!(errs.is_empty(), "{} layout problems:\n{}", errs.len(), errs.join("\n"));
}

#[test]
fn dialogs_and_notifications_fit() {
    let mut errs = Vec::new();
    let mut r = rig("dialogs", true, Theme::Dark);
    r.app.test_status(Some(status_running()));
    for &size in SIZES {
        let base = r.frame(size, 1.0);
        let known: HashSet<(String, [i32; 4])> = base.texts.iter().map(|t| (t.text.clone(), key(t.ink))).collect();
        r.app.test_open_rename(5);
        r.app.test_toast("Could not add C:\\Users\\someone\\Music\\a file with a very long name that needs to wrap.mp3: unsupported format", true);
        let f = r.frame(size, 1.0);
        let overlay: HashSet<usize> = f.texts.iter().enumerate().filter(|(_, t)| !known.contains(&(t.text.clone(), key(t.ink)))).map(|(i, _)| i).collect();
        assert!(f.texts.iter().any(|t| t.text == "Rename sound"), "rename dialog not shown at {size:?}");
        check(&format!("dialogs {size:?}"), &f, Some(&overlay), &mut errs);
        r.app.test_close_dialogs();
    }
    assert!(errs.is_empty(), "{} layout problems:\n{}", errs.len(), errs.join("\n"));
}

fn key(r: Rect) -> [i32; 4] {
    [r.min.x.round() as i32, r.min.y.round() as i32, r.max.x.round() as i32, r.max.y.round() as i32]
}

fn yt_results() -> Vec<Video> {
    let v = |id: &str, title: &str, channel: &str, d: Option<f64>, score: Option<i32>| Video { id: id.into(), title: title.into(), channel: channel.into(), duration: d, score };
    vec![
        v("dQw4w9WgXcQ", "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)", "Rick Astley", Some(214.0), Some(45)),
        v("aaaaaaaaaaa", "An extremely long video title that goes on and on and on, with ALL CAPS WORDS and emoji 🎺🎺 and more words to force wrapping onto several lines", "A channel with a really long name that should be cut off", Some(36000.0), Some(3)),
        v("bbbbbbbbbbb", "x", "", None, None),
    ]
}

#[test]
fn youtube_page_fits_in_every_state() {
    let mut errs = Vec::new();
    let mut r = rig("youtube", true, Theme::Dark);
    r.app.test_status(Some(status_running()));
    r.app.open_view(View::YouTube);
    type Setup = fn(&mut freemic::youtube::State);
    let states: [(&str, Setup); 8] = [
        ("not set up", |s| {
            s.tool = None;
            s.error = Some("could not reach GitHub: io: Connection refused (os error 10061) while fetching https://github.com/yt-dlp/yt-dlp/releases/latest".into());
        }),
        ("results", |s| {
            s.tool = Some(ToolState { version: "2026.10.26".into(), checked: 0, latest: None });
            s.results = yt_results();
            s.ranked = true;
            s.error = None;
        }),
        ("downloading", |s| {
            s.busy = Some(Busy::Download("aaaaaaaaaaa".into()));
            s.progress = Some(0.4);
        }),
        ("error", |s| {
            s.busy = None;
            s.progress = None;
            s.error = Some("YouTube blocked this attempt (bot check or session error). Try again in a moment. (ERROR: [youtube] dQw4w9WgXcQ: Sign in to confirm you're not a bot. Use --cookies-from-browser or --cookies for the authentication.)".into());
        }),
        ("setting up", |s| {
            s.tool = None;
            s.busy = Some(Busy::Setup);
            s.progress = Some(0.7);
            s.error = None;
        }),
        ("update available", |s| {
            s.tool = Some(ToolState { version: "2026.10.26".into(), checked: 1, latest: Some("2026.11.30".into()) });
            s.busy = None;
            s.progress = None;
            s.results = yt_results();
        }),
        ("updating", |s| {
            s.busy = Some(Busy::Update);
        }),
        ("update failed", |s| {
            s.busy = None;
            s.error = Some("yt-dlp can't update itself because FreeMic's folder is read-only for your account (ERROR: Insufficient permissions to write to C:\\Program Files\\FreeMic\\yt-dlp.exe). Install the newest FreeMic instead.".into());
        }),
    ];
    for (name, setup) in states {
        r.app.test_youtube(setup);
        // The YouTube page, and the yt-dlp card in Settings.
        for view in [View::YouTube, View::Settings] {
            r.app.open_view(view);
            for &size in SIZES {
                for ppp in [1.0, 1.5] {
                    let f = r.frame(size, ppp);
                    let title = if view == View::YouTube { "Get sounds from YouTube" } else { "YouTube downloads" };
                    assert!(f.texts.iter().any(|t| t.text == title), "{name} {view:?} {size:?}");
                    check(&format!("youtube {name} {view:?} {size:?} @{ppp}"), &f, None, &mut errs);
                }
            }
        }
    }
    assert!(errs.is_empty(), "{} layout problems:
{}", errs.len(), errs.join("
"));
}
