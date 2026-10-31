//! The user interface (egui). The soundboard is the main screen; audio and
//! application settings live on a separate screen so the board stays clean.

mod icons;
pub mod theme;
mod widgets;
mod youtube_view;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, Key, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
    pos2, vec2,
};

use crate::audio::backend::DeviceInfo;
use crate::audio::engine::{DevStatus, Engine, EngineConfig, Status};
use crate::audio::{Shared, percent_to_gain};
use crate::config::{self, Board, DeviceRef, PlayMode, Settings, Theme};
use crate::hotkeys::{self, Action, Hotkeys};
use crate::paths::Paths;
use crate::{library, platform};
use icons::Icon;
use theme::Palette;
use widgets::Kind;

/// Below this window width the bottom bar uses two rows.
const DOCK_TWO_ROWS_BELOW: f32 = 700.0;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum View {
    Board,
    YouTube,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HotkeyTarget {
    Sound(u64),
    Stop,
}

struct Toast {
    text: String,
    error: bool,
    until: Instant,
}

/// What a hotkey or command needs to start a sound without the UI thread.
#[derive(Clone)]
pub struct PlayTarget {
    pub name: String,
    pub path: PathBuf,
    pub looped: bool,
    /// 1-based position of the button on the board.
    pub position: usize,
}

pub type Targets = Arc<Mutex<HashMap<u64, PlayTarget>>>;

/// Find a sound for `freemic --play`: by name (case-insensitive), else by its
/// button position as "3" or "#3".
pub fn find_target(targets: &HashMap<u64, PlayTarget>, arg: &str) -> Option<u64> {
    let arg = arg.trim();
    if let Some((id, _)) = targets.iter().find(|(_, t)| t.name.eq_ignore_ascii_case(arg)) {
        return Some(*id);
    }
    let n: usize = arg.strip_prefix('#').unwrap_or(arg).trim().parse().ok()?;
    targets.iter().find(|(_, t)| t.position == n).map(|(id, _)| *id)
}

/// Toggle a sound: play it, or stop it if it is the one currently playing.
pub fn trigger(engine: &Engine, targets: &Targets, action: Action) {
    match action {
        Action::Stop => engine.stop_sound(),
        Action::Play(id) => {
            if engine.now_playing().map(|(np, _)| np.sound_id) == Some(id) {
                engine.stop_sound();
                return;
            }
            let t = targets.lock().unwrap().get(&id).cloned();
            if let Some(t) = t {
                engine.play(id, t.path, t.looped);
            }
        }
    }
}

/// Options for starting the UI.
pub struct StartOptions {
    pub start_minimized: bool,
    pub load_warnings: Vec<String>,
    /// Register system-wide hotkeys (off in automated layout tests).
    pub global_hotkeys: bool,
}

pub struct FreeMicApp {
    paths: Paths,
    settings: Settings,
    board: Board,
    engine: Arc<Engine>,
    targets: Targets,
    hotkeys: Hotkeys,
    hotkey_problems: Vec<String>,
    view: View,
    filter: String,
    toasts: Vec<Toast>,
    rename: Option<(u64, String, bool)>,
    capturing: Option<HotkeyTarget>,
    confirm_remove: Option<u64>,
    drag_from: Option<usize>,
    dirty_settings: bool,
    dirty_board: bool,
    last_save: Instant,
    inputs: Vec<DeviceInfo>,
    outputs: Vec<DeviceInfo>,
    meter: [f32; 3],
    start_minimized: bool,
    first_frame: bool,
    dock_height: f32,
    status_override: Option<Status>,
    yt: crate::youtube::Service,
    yt_form: youtube_view::YtForm,
}

impl FreeMicApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ctx: &egui::Context,
        paths: Paths,
        settings: Settings,
        board: Board,
        engine: Arc<Engine>,
        targets: Targets,
        opts: StartOptions,
    ) -> Self {
        theme::install(ctx);
        let hotkeys = if opts.global_hotkeys {
            let (e2, t2, c2) = (engine.clone(), targets.clone(), ctx.clone());
            Hotkeys::new(Arc::new(move |a| {
                trigger(&e2, &t2, a);
                c2.request_repaint();
            }))
        } else {
            Hotkeys::disabled("Global hotkeys are turned off for this test run.")
        };
        let yt = {
            let c = ctx.clone();
            crate::youtube::Service::start(paths.data_dir.join("tools"), paths.sounds_dir().join("youtube"), Arc::new(move || c.request_repaint()))
        };
        let mut app = FreeMicApp {
            paths,
            settings,
            board,
            engine,
            targets,
            hotkeys,
            hotkey_problems: Vec::new(),
            view: View::Board,
            filter: String::new(),
            toasts: Vec::new(),
            rename: None,
            capturing: None,
            confirm_remove: None,
            drag_from: None,
            dirty_settings: false,
            dirty_board: false,
            last_save: Instant::now(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            meter: [0.0; 3],
            start_minimized: opts.start_minimized,
            first_frame: true,
            dock_height: 64.0,
            status_override: None,
            yt,
            yt_form: Default::default(),
        };
        app.apply_theme(ctx);
        app.sync_targets();
        // Hooks for automated UI checks (screenshots); harmless when unset.
        match std::env::var("FREEMIC_TEST_VIEW").as_deref() {
            Ok("settings") => app.open_view(View::Settings),
            Ok("youtube") => app.open_view(View::YouTube),
            _ => {}
        }
        if let Some(id) = std::env::var("FREEMIC_TEST_PLAY").ok().and_then(|v| v.parse().ok()) {
            app.play(id);
        }
        if let Ok(q) = std::env::var("FREEMIC_TEST_YT_SEARCH") {
            app.yt_form.query = q;
            app.yt_submit();
        }
        for w in opts.load_warnings {
            app.toast(w, true);
        }
        app
    }

    pub fn open_view(&mut self, v: View) {
        self.view = v;
        if v == View::Settings {
            self.refresh_devices();
        }
    }

    fn status(&self) -> Status {
        self.status_override.clone().unwrap_or_else(|| self.engine.status())
    }

    fn shared(&self) -> &Shared {
        &self.engine.shared
    }

    fn toast(&mut self, text: impl Into<String>, error: bool) {
        let text = text.into();
        if error {
            log::warn!("{text}");
        }
        self.toasts.push(Toast { text, error, until: Instant::now() + Duration::from_secs(if error { 8 } else { 4 }) });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(match self.settings.theme {
            Theme::System => egui::ThemePreference::System,
            Theme::Dark => egui::ThemePreference::Dark,
            Theme::Light => egui::ThemePreference::Light,
        });
    }

    /// Push volumes/mute into the engine.
    pub fn apply_gains(settings: &Settings, shared: &Shared) {
        shared.sound_gain.store(percent_to_gain(settings.sound_volume));
        shared.mic_gain.store(percent_to_gain(settings.mic_volume));
        shared.master_gain.store(percent_to_gain(settings.master_volume));
        shared.monitor_gain.store(percent_to_gain(settings.monitor_volume));
        shared.mic_muted.store(settings.mic_muted, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn engine_config(settings: &Settings) -> EngineConfig {
        EngineConfig {
            mic_device: settings.mic_device.as_ref().map(|d| d.id.clone()),
            mic_enabled: settings.mic_enabled,
            output_device: settings.output_device.as_ref().map(|d| d.id.clone()),
            monitor_device: settings.monitor_device.as_ref().map(|d| d.id.clone()),
        }
    }

    fn settings_changed(&mut self) {
        Self::apply_gains(&self.settings, self.shared());
        let cfg = Self::engine_config(&self.settings);
        if cfg != self.engine.config() {
            self.engine.set_config(cfg);
        }
        self.dirty_settings = true;
    }

    fn board_changed(&mut self) {
        self.dirty_board = true;
        self.sync_targets();
    }

    /// Refresh the data used by hotkeys and re-register them.
    fn sync_targets(&mut self) {
        {
            let mut t = self.targets.lock().unwrap();
            t.clear();
            for (i, s) in self.board.sounds.iter().enumerate() {
                t.insert(
                    s.id,
                    PlayTarget {
                        name: s.name.clone(),
                        path: self.paths.resolve(&s.file),
                        looped: s.mode == PlayMode::Loop,
                        position: i + 1,
                    },
                );
            }
        }
        let mut bindings: Vec<(String, Action)> =
            self.board.sounds.iter().filter_map(|s| s.hotkey.clone().map(|h| (h, Action::Play(s.id)))).collect();
        if let Some(h) = &self.settings.stop_hotkey {
            bindings.push((h.clone(), Action::Stop));
        }
        self.hotkey_problems = self.hotkeys.sync(&bindings);
    }

    fn save_now(&mut self) {
        if self.dirty_settings {
            if let Err(e) = config::save_json(&self.paths.settings_file(), &self.settings) {
                self.toast(e, true);
            }
            self.dirty_settings = false;
        }
        if self.dirty_board {
            if let Err(e) = config::save_json(&self.paths.board_file(), &self.board) {
                self.toast(e, true);
            }
            self.dirty_board = false;
        }
        self.last_save = Instant::now();
    }

    fn play(&mut self, id: u64) {
        let Some(s) = self.board.get(id) else { return };
        let path = self.paths.resolve(&s.file);
        if !path.exists() {
            let msg = format!("\"{}\": the audio file is missing ({}). Use Replace audio to pick it again.", s.name, path.display());
            self.toast(msg, true);
            return;
        }
        trigger(&self.engine, &self.targets, Action::Play(id));
    }

    fn import_dialog(&mut self) {
        let files = rfd::FileDialog::new()
            .set_title("Add sounds")
            .add_filter("Audio files", crate::audio::decode::SUPPORTED_EXTENSIONS)
            .pick_files();
        if let Some(files) = files {
            self.import(files);
        }
    }

    fn import(&mut self, files: Vec<PathBuf>) {
        let rep = library::import(&self.paths, &mut self.board, &files, self.settings.copy_imports, self.settings.default_mode);
        if !rep.added.is_empty() {
            let n = rep.added.len();
            self.toast(format!("Added {n} sound{}", if n == 1 { "" } else { "s" }), false);
            self.board_changed();
            self.save_now();
        }
        for e in rep.errors {
            self.toast(format!("Could not add {e}"), true);
        }
    }

    fn replace_dialog(&mut self, id: u64) {
        let file = rfd::FileDialog::new()
            .set_title("Choose the new audio file")
            .add_filter("Audio files", crate::audio::decode::SUPPORTED_EXTENSIONS)
            .pick_file();
        if let Some(f) = file {
            if self.engine.now_playing().map(|(n, _)| n.sound_id) == Some(id) {
                self.engine.stop_sound();
            }
            match library::replace(&self.paths, &mut self.board, id, &f, self.settings.copy_imports) {
                Ok(()) => {
                    self.toast("Audio replaced", false);
                    self.board_changed();
                    self.save_now();
                }
                Err(e) => self.toast(format!("Could not use {}: {e}", f.display()), true),
            }
        }
    }

    fn refresh_devices(&mut self) {
        self.inputs = self.engine.list_inputs();
        self.outputs = self.engine.list_outputs();
    }

    // ---------------------------------------------------------------- header

    /// Color, long text, short text and explanation of the virtual-mic state.
    fn vmic_summary(st: &Status) -> (fn(&Palette) -> Color32, &'static str, &'static str, String) {
        match &st.output {
            DevStatus::Running { name, .. } => {
                let mic = st.virtual_mic.mic_name.clone().unwrap_or_else(|| name.clone());
                (|p| p.good, "Virtual mic on", "On", format!("FreeMic is live. In Discord, games and other apps choose the microphone \"{mic}\"."))
            }
            DevStatus::Starting => (|p| p.warn, "Starting…", "…", "The virtual microphone is starting.".into()),
            DevStatus::Error(e) => (|p| p.bad, "Set up virtual mic", "Set up", format!("Virtual microphone unavailable: {e}")),
            DevStatus::Off => (|p| p.faint, "Virtual mic off", "Off", "The virtual microphone is off.".into()),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let p = *theme::pal(ui);
        let st = self.status();
        let (col, long, short, explain) = Self::vmic_summary(&st);
        let col = col(&p);
        let items = [(Icon::Grid, "Soundboard"), (Icon::Download, "YouTube"), (Icon::Gear, "Settings")];
        let views = [View::Board, View::YouTube, View::Settings];
        let selected = views.iter().position(|v| *v == self.view).unwrap_or(0);

        // Pick the most detailed variant that fits the window width.
        let avail = ui.available_width();
        let brand_w = 30.0 + 9.0 + widgets::text_width(ui, "FreeMic", theme::font_bold(16.5));
        let variants: [(bool, bool, &str); 5] = [(true, true, long), (false, true, long), (false, true, short), (false, false, short), (false, false, "")];
        let (show_brand, show_labels, pill_text) = variants
            .into_iter()
            .find(|(b, l, t)| {
                let w = if *b { brand_w + 18.0 } else { 30.0 + 14.0 } + widgets::tabs_width(ui, &items, *l) + 14.0 + widgets::pill_width(ui, t);
                w <= avail
            })
            .unwrap_or((false, false, ""));

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            widgets::logo(ui, 30.0);
            if show_brand {
                ui.add_space(9.0);
                ui.label(RichText::new("FreeMic").font(theme::font_bold(16.5)).color(p.text));
                ui.add_space(18.0);
            } else {
                ui.add_space(14.0);
            }
            if let Some(i) = widgets::tabs(ui, &items, selected, show_labels) {
                self.open_view(views[i]);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::pill(ui, col, pill_text).on_hover_text(explain).clicked() {
                    self.open_view(View::Settings);
                }
            });
        });
    }

    // ---------------------------------------------------------------- bottom bar

    fn dock(&mut self, ui: &mut egui::Ui) {
        let p = *theme::pal(ui);
        let avail = ui.available_width();
        let playing = self.engine.now_playing();
        let stop_w = 84.0;
        let gap = 12.0;
        let vol_fixed = 18.0 + 6.0 + 4.0 + 38.0 + 18.0 + 32.0 + 6.0 + 4.0 + 38.0;
        let mut changed = false;
        if avail >= DOCK_TWO_ROWS_BELOW {
            let slider_w = ((avail - 280.0 - stop_w - vol_fixed - 3.0 * gap) / 2.0).clamp(80.0, 130.0);
            let np_w = avail - stop_w - vol_fixed - 2.0 * slider_w - 3.0 * gap;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                self.now_playing(ui, &p, playing.as_ref(), np_w);
                ui.add_space(gap);
                self.stop_button(ui, playing.is_some(), stop_w);
                ui.add_space(gap);
                changed |= self.volumes(ui, &p, slider_w);
            });
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                self.now_playing(ui, &p, playing.as_ref(), avail - stop_w - gap);
                ui.add_space(gap);
                self.stop_button(ui, playing.is_some(), stop_w);
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let slider_w = ((avail - vol_fixed) / 2.0).max(60.0);
                changed |= self.volumes(ui, &p, slider_w);
            });
        }
        if changed {
            self.settings_changed();
        }
    }

    fn stop_button(&mut self, ui: &mut egui::Ui, enabled: bool, width: f32) {
        let r = ui.add_enabled_ui(enabled, |ui| widgets::button_sized(ui, Kind::Secondary, Some(Icon::Stop), "Stop", Some(width))).inner;
        let tip = match &self.settings.stop_hotkey {
            Some(h) => format!("Stop the soundboard (Esc, or {} anywhere)", hotkeys::pretty(h)),
            None => "Stop the soundboard (Esc)".to_string(),
        };
        if r.on_hover_text(tip).clicked() {
            self.engine.stop_sound();
        }
    }

    /// Icon tile + name + progress of the sound that is playing.
    fn now_playing(&self, ui: &mut egui::Ui, p: &Palette, playing: Option<&(crate::audio::engine::NowPlaying, f64)>, width: f32) {
        let (rect, _) = ui.allocate_exact_size(vec2(width.max(80.0), 40.0), Sense::hover());
        let painter = ui.painter();
        let tile = Rect::from_min_size(rect.min, Vec2::splat(40.0));
        painter.rect_filled(tile, 10.0, if playing.is_some() { theme::mix(p.accent, p.surface, 0.82) } else { p.raised });
        let text_x = tile.right() + 12.0;
        let text_w = rect.right() - text_x;
        match playing {
            Some((np, pos)) => {
                icons::paint(painter, tile.shrink(12.0), if np.looped { Icon::Loop } else { Icon::Play }, p.accent);
                let sound = self.board.get(np.sound_id);
                let name = sound.map(|s| s.name.clone()).unwrap_or_default();
                let g = widgets::one_line(ui, &name, theme::font_bold(14.0), p.text, text_w);
                painter.galley(pos2(text_x, rect.top() + 2.0), g, p.text);
                let dur = sound.and_then(|s| s.duration).filter(|d| *d > 0.0);
                let time = match (dur, np.looped) {
                    (Some(d), false) => format!("{} / {}", fmt_time(*pos), fmt_time(d)),
                    (Some(d), true) => format!("{} · looping", fmt_time(pos % d)),
                    (None, true) => format!("{} · looping", fmt_time(*pos)),
                    (None, false) => fmt_time(*pos),
                };
                let tg = widgets::one_line(ui, &time, theme::font(12.0), p.dim, text_w);
                let tw = tg.size().x;
                let ty = rect.bottom() - tg.size().y - 1.0;
                painter.galley(pos2(text_x, ty), tg, p.dim);
                // Progress bar after the time, when there is room for it.
                let bar_x = text_x + tw + 10.0;
                let bar_w = (rect.right() - bar_x).min(220.0);
                if let Some(d) = dur {
                    if bar_w >= 40.0 {
                        let frac = ((pos % d) / d).clamp(0.0, 1.0) as f32;
                        let bar = Rect::from_min_size(pos2(bar_x, ty + 6.0), vec2(bar_w, 4.0));
                        painter.rect_filled(bar, 2.0, p.raised);
                        painter.rect_filled(Rect::from_min_size(bar.min, vec2(bar_w * frac, 4.0)), 2.0, p.accent);
                    }
                }
            }
            None => {
                icons::paint(painter, tile.shrink(12.0), Icon::Music, p.faint);
                let g = widgets::one_line(ui, "Nothing playing", theme::font_bold(14.0), p.dim, text_w);
                painter.galley(pos2(text_x, rect.top() + 2.0), g, p.dim);
                let hint = if self.board.sounds.is_empty() { "Add a sound to get started" } else { "Click a sound to play it" };
                let g = widgets::one_line(ui, hint, theme::font(12.0), p.faint, text_w);
                painter.galley(pos2(text_x, rect.bottom() - g.size().y - 1.0), g, p.faint);
            }
        }
    }

    /// Soundboard and microphone volume (+ mute) in the bottom bar.
    fn volumes(&mut self, ui: &mut egui::Ui, p: &Palette, slider_w: f32) -> bool {
        let mut changed = false;
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 32.0), Sense::hover());
        icons::paint(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(17.0)), Icon::Speaker, p.dim);
        ui.add_space(6.0);
        let tip = format!("Soundboard volume {:.0}%", self.settings.sound_volume);
        ui.scope(|ui| {
            changed |= widgets::volume(ui, &mut self.settings.sound_volume, 100.0, slider_w);
        })
        .response
        .on_hover_text(tip);
        ui.add_space(18.0);

        let muted = self.settings.mic_muted;
        let st = self.status();
        let mic_state = match &st.mic {
            DevStatus::Running { name, .. } => format!("Microphone: {name}"),
            DevStatus::Starting => "Microphone starting…".into(),
            DevStatus::Error(e) => format!("Microphone: {e}"),
            DevStatus::Off => "Microphone passthrough is off (Settings)".into(),
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
        let fill = if muted { theme::mix(p.bad, p.surface, 0.75) } else if resp.hovered() { p.raised } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, 8.0, fill);
        icons::paint(ui.painter(), rect.shrink(8.0), if muted { Icon::MicOff } else { Icon::Mic }, if muted { p.bad } else { p.dim });
        let tip = if muted { format!("Microphone muted, click to unmute\n{mic_state}") } else { format!("Mute your microphone (the soundboard keeps playing)\n{mic_state}") };
        let label = if muted { "Unmute microphone" } else { "Mute microphone" };
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, muted, label));
        if resp.on_hover_text(tip).clicked() {
            self.settings.mic_muted = !muted;
            changed = true;
        }
        ui.add_space(6.0);
        let tip = format!("Microphone volume {:.0}%", self.settings.mic_volume);
        ui.scope(|ui| {
            changed |= widgets::volume(ui, &mut self.settings.mic_volume, 200.0, slider_w);
        })
        .response
        .on_hover_text(tip);
        changed
    }

    // ---------------------------------------------------------------- soundboard

    fn setup_banner(&mut self, ui: &mut egui::Ui) {
        let st = self.status();
        if st.virtual_mic.feed.is_some() || matches!(st.output, DevStatus::Starting) {
            return;
        }
        let Some(problem) = st.virtual_mic.problem.clone() else { return };
        let p = *theme::pal(ui);
        egui::Frame::new()
            .fill(theme::mix(p.warn, p.bg, 0.86))
            .stroke(Stroke::new(1.0, theme::mix(p.warn, p.bg, 0.6)))
            .corner_radius(12)
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                    icons::paint(ui.painter(), r, Icon::Warning, p.warn);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("The virtual microphone isn't set up yet").font(theme::font_bold(14.0)).color(p.text));
                        ui.add(egui::Label::new(RichText::new(problem).color(p.dim)).wrap());
                        if widgets::button(ui, Kind::Secondary, None, "Set it up").clicked() {
                            self.open_view(View::Settings);
                        }
                    });
                });
            });
        ui.add_space(12.0);
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let p = *theme::pal(ui);
        ui.vertical_centered(|ui| {
            let roomy = ui.available_height() > 280.0;
            ui.add_space((ui.available_height() * 0.2).clamp(4.0, 160.0));
            if roomy {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                ui.painter().rect_filled(r, 18.0, p.raised);
                icons::paint(ui.painter(), r.shrink(18.0), Icon::Music, p.accent);
                ui.add_space(6.0);
            }
            ui.label(RichText::new("Your soundboard is empty").font(theme::font_bold(19.0)).color(p.text));
            ui.add(egui::Label::new(RichText::new("Add audio files (MP3, WAV, OGG, FLAC or M4A) or drop them onto this window.").color(p.dim)).wrap());
            ui.add_space(6.0);
            if widgets::button(ui, Kind::Primary, Some(Icon::Plus), "Add sounds").clicked() {
                self.import_dialog();
            }
            ui.add_space(12.0);
        });
    }

    fn board_toolbar(&mut self, ui: &mut egui::Ui) {
        let p = *theme::pal(ui);
        let avail = ui.available_width();
        let add_w = 13.0 * 2.0 + 16.0 + 7.0 + widgets::text_width(ui, "Add sounds", theme::font_bold(13.5));
        let n = self.board.sounds.len();
        let count = format!("{n} sound{}", if n == 1 { "" } else { "s" });
        let count_w = widgets::text_width(ui, &count, theme::font(13.0)) + 16.0;
        let show_count = avail - add_w - count_w - 12.0 >= 200.0;
        let search_w = (avail - add_w - 12.0 - if show_count { count_w } else { 0.0 }).clamp(80.0, 300.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let te = egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Search sounds")
                .desired_width(search_w - 38.0)
                .margin(Margin { left: 30, right: 8, top: 7, bottom: 7 })
                .min_size(vec2(search_w, 32.0));
            let r = ui.add(te);
            icons::paint(ui.painter(), Rect::from_min_size(pos2(r.rect.left() + 10.0, r.rect.center().y - 7.0), Vec2::splat(14.0)), Icon::Search, p.faint);
            if !self.filter.is_empty() {
                let cr = Rect::from_center_size(pos2(r.rect.right() - 14.0, r.rect.center().y), Vec2::splat(20.0));
                let c = ui.interact(cr, ui.scope_id().with("clear-search"), Sense::click());
                icons::paint(ui.painter(), cr.shrink(5.0), Icon::Close, if c.hovered() { p.text } else { p.faint });
                if c.on_hover_text("Clear search").clicked() {
                    self.filter.clear();
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::button(ui, Kind::Primary, Some(Icon::Plus), "Add sounds")
                    .on_hover_text("Import MP3, WAV, OGG, FLAC or M4A files (you can also drop files onto the window)")
                    .clicked()
                {
                    self.import_dialog();
                }
                if show_count {
                    ui.add_space(16.0);
                    ui.label(RichText::new(count).font(theme::font(13.0)).color(p.faint));
                }
            });
        });
    }

    fn board_view(&mut self, ui: &mut egui::Ui) {
        if self.board.sounds.is_empty() {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.setup_banner(ui);
                self.empty_state(ui);
            });
            return;
        }
        // Same inset as the pad grid below.
        egui::Frame::new().inner_margin(Margin { left: 6, right: 8, top: 0, bottom: 0 }).show(ui, |ui| {
            self.setup_banner(ui);
            self.board_toolbar(ui);
        });
        ui.add_space(8.0);

        let p = *theme::pal(ui);
        let playing = self.engine.now_playing();
        let filter = self.filter.to_lowercase();
        let visible: Vec<usize> = (0..self.board.sounds.len())
            .filter(|&i| filter.is_empty() || self.board.sounds[i].name.to_lowercase().contains(&filter))
            .collect();
        if visible.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Label::new(RichText::new(format!("No sound matches \"{}\"", self.filter)).color(p.dim)).wrap());
            });
            return;
        }

        let min_w = self.settings.button_width;
        let gap = 12.0;
        let mut action_play: Option<u64> = None;
        let mut menu_action: Option<(u64, MenuAction)> = None;
        let mut drop_target: Option<usize> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Inset so the "playing" ring around a pad is never clipped by the scroll area.
            let inset = 6.0;
            let width = ui.available_width() - 2.0 * inset - 2.0;
            let cols = (((width + gap) / (min_w + gap)).floor() as usize).max(1);
            let bw = ((width - gap * (cols - 1) as f32) / cols as f32).min(min_w * 1.4);
            let bh = (bw * 0.52).clamp(64.0, 112.0);
            let rows = visible.len().div_ceil(cols);
            let (outer, _) = ui.allocate_exact_size(vec2(width + 2.0 * inset, rows as f32 * (bh + gap) - gap + 2.0 * inset), Sense::hover());
            let area = outer.shrink(inset);
            let pointer = ui.ctx().pointer_interact_pos();
            for (n, &idx) in visible.iter().enumerate() {
                let (r, c) = (n / cols, n % cols);
                let rect = Rect::from_min_size(area.min + vec2(c as f32 * (bw + gap), r as f32 * (bh + gap)), vec2(bw, bh));
                let sound = &self.board.sounds[idx];
                let id = ui.make_persistent_id(("sound", sound.id));
                let resp = ui.interact(rect, id, Sense::click_and_drag());
                let is_playing = playing.as_ref().map(|(np, _)| np.sound_id) == Some(sound.id);
                let progress = playing.as_ref().filter(|_| is_playing).map(|(_, pos)| *pos);
                if ui.is_rect_visible(rect) {
                    paint_pad(ui, &p, rect, sound, is_playing, progress, resp.hovered(), self.drag_from == Some(idx));
                }
                let sound_name = sound.name.clone();
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &sound_name));

                if resp.clicked() {
                    action_play = Some(sound.id);
                }
                if resp.drag_started() && filter.is_empty() {
                    self.drag_from = Some(idx);
                }
                if self.drag_from.is_some() && pointer.is_some_and(|p| rect.contains(p)) {
                    drop_target = Some(idx);
                    if self.drag_from != Some(idx) {
                        ui.painter().rect_stroke(rect.expand(4.0), 14.0, Stroke::new(2.0, p.accent), StrokeKind::Outside);
                    }
                }
                let hover = match &sound.hotkey {
                    Some(h) => format!("{}\nClick to play · right-click for options · drag to move\nHotkey: {}", sound.name, hotkeys::pretty(h)),
                    None => format!("{}\nClick to play · right-click for options · drag to move", sound.name),
                };
                let resp = resp.on_hover_text(hover);
                let sid = sound.id;
                let mode = sound.mode;
                let has_hotkey = sound.hotkey.is_some();
                let color = sound.color;
                resp.context_menu(|ui| {
                    if let Some(a) = sound_menu(ui, mode, color, has_hotkey, idx, self.board.sounds.len()) {
                        menu_action = Some((sid, a));
                        ui.close();
                    }
                });
            }
        });

        // Finish a drag: move the dragged button to where it was dropped.
        if self.drag_from.is_some() && ui.ctx().input(|i| i.pointer.any_released()) {
            if let (Some(from), Some(to)) = (self.drag_from, drop_target) {
                if from != to {
                    self.board.move_to(from, to);
                    self.board_changed();
                }
            }
            self.drag_from = None;
        }
        if let Some(id) = action_play {
            self.play(id);
        }
        if let Some((id, a)) = menu_action {
            self.run_menu_action(id, a);
        }
    }

    fn run_menu_action(&mut self, id: u64, a: MenuAction) {
        let Some(idx) = self.board.index_of(id) else { return };
        match a {
            MenuAction::Rename => self.rename = Some((id, self.board.sounds[idx].name.clone(), true)),
            MenuAction::Mode(m) => {
                self.board.sounds[idx].mode = m;
                self.board_changed();
            }
            MenuAction::Color(c) => {
                self.board.sounds[idx].color = c;
                self.board_changed();
            }
            MenuAction::SetHotkey => self.capturing = Some(HotkeyTarget::Sound(id)),
            MenuAction::ClearHotkey => {
                self.board.sounds[idx].hotkey = None;
                self.board_changed();
            }
            MenuAction::Replace => self.replace_dialog(id),
            MenuAction::MoveBy(d) => {
                let to = (idx as i64 + d).clamp(0, self.board.sounds.len() as i64 - 1) as usize;
                self.board.move_to(idx, to);
                self.board_changed();
            }
            MenuAction::Remove => self.confirm_remove = Some(id),
        }
    }

    // ---------------------------------------------------------------- dialogs

    fn dialogs(&mut self, ctx: &egui::Context) {
        // Rename.
        if let Some((id, mut text, first)) = self.rename.take() {
            let mut keep = true;
            let mut commit = false;
            modal(ctx, "rename", |ui, p| {
                ui.label(RichText::new("Rename sound").font(theme::font_bold(17.0)).color(p.text));
                let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(ui.available_width()).char_limit(60).margin(Margin::symmetric(10, 7)));
                if first {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    commit = true;
                }
                ui.add_space(4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, Kind::Primary, None, "Save").clicked() {
                        commit = true;
                    }
                    if widgets::button(ui, Kind::Ghost, None, "Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        keep = false;
                    }
                });
            });
            if commit {
                let name = text.trim().to_string();
                if !name.is_empty() {
                    if let Some(s) = self.board.get_mut(id) {
                        s.name = name;
                    }
                    self.board_changed();
                }
                keep = false;
            }
            if keep {
                self.rename = Some((id, text, false));
            }
        }

        // Confirm removal.
        if let Some(id) = self.confirm_remove {
            let name = self.board.get(id).map(|s| s.name.clone()).unwrap_or_default();
            let mut close = false;
            modal(ctx, "remove", |ui, p| {
                ui.label(RichText::new("Remove this sound?").font(theme::font_bold(17.0)).color(p.text));
                ui.add(egui::Label::new(RichText::new(format!("\"{name}\" will be removed from the soundboard.")).color(p.dim)).wrap());
                ui.add_space(4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, Kind::Danger, None, "Remove").clicked() {
                        if self.engine.now_playing().map(|(n, _)| n.sound_id) == Some(id) {
                            self.engine.stop_sound();
                        }
                        library::remove(&self.paths, &mut self.board, id);
                        self.board_changed();
                        close = true;
                    }
                    if widgets::button(ui, Kind::Ghost, None, "Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        close = true;
                    }
                });
            });
            if close {
                self.confirm_remove = None;
            }
        }

        // Hotkey capture.
        if let Some(target) = self.capturing {
            let mut result: Option<Option<String>> = None;
            modal(ctx, "hotkey", |ui, p| {
                ui.label(RichText::new("Press a key combination").font(theme::font_bold(17.0)).color(p.text));
                ui.add(egui::Label::new(RichText::new("For example Ctrl+Shift+1 or F9. Press Esc to cancel.").color(p.dim)).wrap());
                widgets::hint(ui, "Letters and digits need Ctrl, Alt or Shift so typing in a chat never triggers a sound.");
                ui.input(|i| {
                    for ev in &i.events {
                        if let egui::Event::Key { key, pressed: true, modifiers, .. } = ev {
                            if *key == Key::Escape {
                                result = Some(None);
                            } else if let Some(h) = hotkeys::from_egui(*key, *modifiers) {
                                result = Some(Some(h));
                            }
                        }
                    }
                });
                ui.add_space(4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::button(ui, Kind::Ghost, None, "Cancel").clicked() {
                        result = Some(None);
                    }
                });
            });
            if let Some(r) = result {
                self.capturing = None;
                if let Some(h) = r {
                    // One key, one action: remove it from anything else first.
                    for s in &mut self.board.sounds {
                        if s.hotkey.as_deref() == Some(h.as_str()) {
                            s.hotkey = None;
                        }
                    }
                    if self.settings.stop_hotkey.as_deref() == Some(h.as_str()) {
                        self.settings.stop_hotkey = None;
                    }
                    match target {
                        HotkeyTarget::Sound(id) => {
                            if let Some(s) = self.board.get_mut(id) {
                                s.hotkey = Some(h);
                            }
                        }
                        HotkeyTarget::Stop => self.settings.stop_hotkey = Some(h),
                    }
                    self.dirty_settings = true;
                    self.board_changed();
                    for p in self.hotkey_problems.clone() {
                        self.toast(p, true);
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------- settings

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        let st = self.status();
        let mut changed = false;
        let p = *theme::pal(ui);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let avail = ui.available_width();
            let w = avail.min(720.0);
            ui.horizontal(|ui| {
                ui.add_space(((avail - w) / 2.0).max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 10.0;
                    changed |= self.settings_cards(ui, &st, &p);
                    ui.add_space(8.0);
                });
            });
        });
        if changed {
            self.settings.sanitize();
            self.settings_changed();
        }
    }

    fn combo_width(ui: &egui::Ui, reserve: f32) -> f32 {
        (ui.available_width() - reserve).clamp(150.0, 340.0)
    }

    fn settings_cards(&mut self, ui: &mut egui::Ui, st: &Status, p: &Palette) -> bool {
        let mut changed = false;

        // ---------------- Virtual microphone
        widgets::card(ui, "Virtual microphone", Some("Where FreeMic sends your voice and the soundboard."), |ui| {
            match &st.output {
                DevStatus::Running { name, rate, latency_ms } => {
                    widgets::status_line(ui, p.good, format!("Active: sending audio to \"{name}\" ({rate} Hz, {latency_ms:.0} ms buffer)."));
                    if let Some(mic) = &st.virtual_mic.mic_name {
                        egui::Frame::new().fill(p.raised).corner_radius(8).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(egui::Label::new(RichText::new("In Discord, games and other apps choose this microphone:").color(p.dim)).wrap());
                            ui.add(egui::Label::new(RichText::new(mic).font(theme::font_bold(14.0)).color(p.text)).wrap());
                        });
                    }
                }
                DevStatus::Starting => widgets::status_line(ui, p.warn, "Starting…"),
                DevStatus::Error(e) => widgets::status_line(ui, p.bad, e.clone()),
                DevStatus::Off => {}
            }
            if st.virtual_mic.feed.is_none() {
                setup_instructions(ui, p);
            }
            widgets::row(ui, "Send the mix to", |ui| {
                let current = self.settings.output_device.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "Automatic (detect virtual cable)".into());
                let w = Self::combo_width(ui, 140.0);
                egui::ComboBox::from_id_salt("outdev").selected_text(current).width(w).truncate().show_ui(ui, |ui| {
                    if ui.selectable_label(self.settings.output_device.is_none(), "Automatic (detect virtual cable)").clicked() {
                        self.settings.output_device = None;
                        changed = true;
                    }
                    for d in &self.outputs {
                        let sel = self.settings.output_device.as_ref().map(|x| &x.id) == Some(&d.id);
                        if ui.selectable_label(sel, &d.name).clicked() {
                            self.settings.output_device = Some(DeviceRef { id: d.id.clone(), name: d.name.clone() });
                            changed = true;
                        }
                    }
                });
                if widgets::button(ui, Kind::Secondary, None, "Re-check devices").clicked() {
                    self.engine.rescan();
                    self.refresh_devices();
                }
            });
            if self.settings.output_device.is_some() {
                widgets::hint(ui, "Advanced: only pick the input side of a virtual cable. Picking your speakers would play everything out loud.");
            }
        });

        // ---------------- Microphone
        widgets::card(ui, "Your microphone", Some("Your real microphone is mixed into the virtual microphone."), |ui| {
            changed |= ui.checkbox(&mut self.settings.mic_enabled, "Pass my microphone through").changed();
            ui.add_enabled_ui(self.settings.mic_enabled, |ui| {
                widgets::row(ui, "Microphone", |ui| {
                    let current = self.settings.mic_device.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "System default".into());
                    let avoid = st.virtual_mic.mic_name.clone();
                    let w = Self::combo_width(ui, 0.0);
                    egui::ComboBox::from_id_salt("micdev").selected_text(current).width(w).truncate().show_ui(ui, |ui| {
                        if ui.selectable_label(self.settings.mic_device.is_none(), "System default").clicked() {
                            self.settings.mic_device = None;
                            changed = true;
                        }
                        for d in &self.inputs {
                            if Some(&d.name) == avoid.as_ref() {
                                continue; // never offer the virtual mic itself (feedback loop)
                            }
                            let sel = self.settings.mic_device.as_ref().map(|x| &x.id) == Some(&d.id);
                            if ui.selectable_label(sel, &d.name).clicked() {
                                self.settings.mic_device = Some(DeviceRef { id: d.id.clone(), name: d.name.clone() });
                                changed = true;
                            }
                        }
                    });
                });
                match &st.mic {
                    DevStatus::Running { name, rate, latency_ms } => {
                        widgets::hint(ui, format!("Capturing \"{name}\" at {rate} Hz (about {latency_ms:.0} ms buffer)."));
                    }
                    DevStatus::Error(e) => widgets::status_line(ui, p.bad, e.clone()),
                    _ => {}
                }
                if let Some(n) = &st.mic_notice {
                    widgets::hint(ui, n.clone());
                }
                widgets::row(ui, "Volume", |ui| {
                    let sw = (ui.available_width() - 50.0).clamp(80.0, 260.0);
                    changed |= widgets::volume(ui, &mut self.settings.mic_volume, 200.0, sw);
                });
                changed |= ui.checkbox(&mut self.settings.mic_muted, "Mute my microphone").changed();
            });
        });

        // ---------------- Levels
        widgets::card(ui, "Levels", None, |ui| {
            widgets::row(ui, "Soundboard volume", |ui| {
                let sw = (ui.available_width() - 50.0).clamp(80.0, 260.0);
                changed |= widgets::volume(ui, &mut self.settings.sound_volume, 100.0, sw);
            });
            widgets::row(ui, "Virtual mic output", |ui| {
                let sw = (ui.available_width() - 50.0).clamp(80.0, 260.0);
                changed |= widgets::volume(ui, &mut self.settings.master_volume, 100.0, sw);
            });
            let s = self.engine.shared.clone();
            let lv = [s.mic_peak.take(), s.sound_peak.take(), s.out_peak.take()];
            for (m, v) in self.meter.iter_mut().zip(lv) {
                *m = if v > *m { v } else { *m * 0.85 };
            }
            for (label, v) in [("Microphone level", self.meter[0]), ("Soundboard level", self.meter[1]), ("Output level", self.meter[2])] {
                widgets::row(ui, label, |ui| {
                    let mw = (ui.available_width() - 50.0).clamp(80.0, 300.0);
                    widgets::meter(ui, p, v, mw);
                });
            }
            let lg = s.limiter_gain.load();
            s.limiter_gain.store(1.0);
            if lg < 0.9 {
                widgets::status_line(ui, p.warn, "The peak limiter is preventing clipping. Consider lowering a volume.");
            }
        });

        // ---------------- Monitoring
        widgets::card(
            ui,
            "Hear the soundboard yourself",
            Some("Optional. Plays only the soundboard (never your microphone) on a device you choose, e.g. your headphones."),
            |ui| {
                widgets::row(ui, "Monitor on", |ui| {
                    let current = self.settings.monitor_device.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "Off".into());
                    let feed = st.virtual_mic.feed.as_ref().map(|f| f.id.clone());
                    let w = Self::combo_width(ui, 0.0);
                    egui::ComboBox::from_id_salt("mondev").selected_text(current).width(w).truncate().show_ui(ui, |ui| {
                        if ui.selectable_label(self.settings.monitor_device.is_none(), "Off").clicked() {
                            self.settings.monitor_device = None;
                            changed = true;
                        }
                        for d in &self.outputs {
                            if Some(&d.id) == feed.as_ref() {
                                continue;
                            }
                            let sel = self.settings.monitor_device.as_ref().map(|x| &x.id) == Some(&d.id);
                            if ui.selectable_label(sel, &d.name).clicked() {
                                self.settings.monitor_device = Some(DeviceRef { id: d.id.clone(), name: d.name.clone() });
                                changed = true;
                            }
                        }
                    });
                });
                if self.settings.monitor_device.is_some() {
                    widgets::row(ui, "Monitor volume", |ui| {
                        let sw = (ui.available_width() - 50.0).clamp(80.0, 260.0);
                        changed |= widgets::volume(ui, &mut self.settings.monitor_volume, 100.0, sw);
                    });
                    if let DevStatus::Error(e) = &st.monitor {
                        widgets::status_line(ui, p.bad, e.clone());
                    }
                }
            },
        );

        // ---------------- Hotkeys
        widgets::card(ui, "Hotkeys", Some("Hotkeys work while other apps and games have focus."), |ui| {
            if let Some(u) = &self.hotkeys.unavailable {
                widgets::status_line(ui, p.warn, u.clone());
                #[cfg(target_os = "linux")]
                widgets::hint(ui, "On Wayland, bind keys in your compositor to commands like  freemic --play \"Name\"  or  freemic --stop.");
            }
            widgets::row(ui, "Stop all sounds", |ui| {
                let t = self.settings.stop_hotkey.as_deref().map(hotkeys::pretty).unwrap_or_else(|| "None".into());
                key_chip(ui, p, &t);
                ui.add_space(4.0);
                if widgets::button(ui, Kind::Secondary, None, "Set…").clicked() {
                    self.capturing = Some(HotkeyTarget::Stop);
                }
                if self.settings.stop_hotkey.is_some() && widgets::button(ui, Kind::Ghost, None, "Clear").clicked() {
                    self.settings.stop_hotkey = None;
                    self.dirty_settings = true;
                    self.sync_targets();
                }
            });
            widgets::hint(ui, "For a sound: right-click it on the soundboard and choose Set hotkey.");
            for prob in &self.hotkey_problems {
                widgets::status_line(ui, p.warn, prob.clone());
            }
        });

        self.youtube_settings_card(ui, p);

        // ---------------- General
        widgets::card(ui, "General", None, |ui| {
            widgets::row(ui, "Theme", |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (t, n) in [(Theme::System, "System"), (Theme::Dark, "Dark"), (Theme::Light, "Light")] {
                    let kind = if self.settings.theme == t { Kind::Primary } else { Kind::Secondary };
                    if widgets::button(ui, kind, None, n).clicked() {
                        self.settings.theme = t;
                        self.apply_theme(ui.ctx());
                        changed = true;
                    }
                }
            });
            widgets::row(ui, "New sounds play", |ui| {
                changed |= ui.radio_value(&mut self.settings.default_mode, PlayMode::Once, "Once").changed();
                changed |= ui.radio_value(&mut self.settings.default_mode, PlayMode::Loop, "Looped").changed();
            });
            widgets::row(ui, "Button size", |ui| {
                let sw = (ui.available_width() - 50.0).clamp(80.0, 260.0);
                changed |= widgets::slider(ui, &mut self.settings.button_width, 100.0, 260.0, sw).changed();
            });
            changed |= ui
                .checkbox(&mut self.settings.copy_imports, "Copy added files into FreeMic's library")
                .on_hover_text("Recommended. Keeps the soundboard working if the original files are moved or deleted.")
                .changed();
            let before = self.settings.start_with_system;
            if ui.checkbox(&mut self.settings.start_with_system, "Start FreeMic when I sign in (minimized)").changed() {
                match platform::set_autostart(self.settings.start_with_system) {
                    Ok(()) => changed = true,
                    Err(e) => {
                        self.settings.start_with_system = before;
                        self.toast(format!("Could not change startup setting: {e}"), true);
                    }
                }
            }
            if self.paths.portable && self.settings.start_with_system {
                widgets::hint(ui, "Portable mode: if you move the FreeMic folder, turn this off and on again.");
            }
        });

        widgets::card(ui, "About", None, |ui| {
            ui.add(egui::Label::new(format!("FreeMic {} · audio backend: {}", env!("CARGO_PKG_VERSION"), self.engine.backend_name())).wrap());
            widgets::hint(ui, format!("{} data folder: {}", if self.paths.portable { "Portable" } else { "Installed" }, self.paths.data_dir.display()));
            ui.horizontal_wrapped(|ui| {
                if widgets::button(ui, Kind::Secondary, Some(Icon::Folder), "Open data folder").clicked() {
                    platform::open_folder(&self.paths.data_dir);
                }
                if widgets::button(ui, Kind::Secondary, None, "Open log file").clicked() {
                    platform::open_url(&self.paths.log_file().to_string_lossy());
                }
            });
        });
        changed
    }

    // ---------------------------------------------------------------- toasts

    fn show_toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if self.rename.is_some() || self.confirm_remove.is_some() || self.capturing.is_some() {
            // Keep notifications for after the dialog instead of drawing them over it.
            for t in &mut self.toasts {
                t.until = t.until.max(now + Duration::from_secs(2));
            }
            return;
        }
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let screen = ctx.content_rect();
        let max_w = (screen.width() - 32.0).clamp(160.0, 380.0);
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -(self.dock_height + 12.0)))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let p = *theme::pal(ui);
                ui.spacing_mut().item_spacing.y = 8.0;
                for t in &self.toasts {
                    let accent = if t.error { p.bad } else { p.good };
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(10)
                        .shadow(ui.visuals().popup_shadow)
                        .inner_margin(Margin { left: 14, right: 14, top: 10, bottom: 10 })
                        .show(ui, |ui| {
                            ui.set_max_width(max_w);
                            ui.horizontal_top(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::hover());
                                icons::paint(ui.painter(), Rect::from_min_size(r.min + vec2(0.0, 1.0), Vec2::splat(16.0)), if t.error { Icon::Warning } else { Icon::Check }, accent);
                                ui.add(egui::Label::new(RichText::new(&t.text).color(p.text)).wrap());
                            });
                        });
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    // ---------------------------------------------------------------- frame

    /// Draw one frame of the whole window.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.first_frame {
            self.first_frame = false;
            if self.start_minimized {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }
        for e in self.engine.take_errors() {
            self.toast(e, true);
        }
        self.yt_poll();
        // Files dropped onto the window.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !dropped.is_empty() {
            self.import(dropped);
        }
        // Keyboard: Escape stops the soundboard when the window is focused.
        if self.capturing.is_none() && self.rename.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.engine.stop_sound();
        }

        let p = *theme::pal(ui);
        egui::Panel::top("header")
            .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin { left: 16, right: 16, top: 12, bottom: 12 }))
            .show(ui, |ui| self.header(ui));
        let dock = egui::Panel::bottom("dock")
            .frame(egui::Frame::new().fill(p.surface).inner_margin(Margin { left: 16, right: 16, top: 12, bottom: 12 }))
            .show(ui, |ui| self.dock(ui));
        self.dock_height = dock.response.rect.height();
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin { left: 10, right: 8, top: 14, bottom: 0 }))
            .show(ui, |ui| match self.view {
                View::Board => self.board_view(ui),
                View::YouTube => self.youtube_view(ui),
                View::Settings => self.settings_view(ui),
            });
        self.dialogs(&ctx);
        self.show_toasts(&ctx);

        // Repaint only while something is changing on screen.
        if self.engine.now_playing().is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.view == View::Settings {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.dirty_board || self.dirty_settings {
            if self.last_save.elapsed() > Duration::from_millis(800) && !ctx.input(|i| i.pointer.any_down()) {
                self.save_now();
            } else {
                ctx.request_repaint_after(Duration::from_millis(900));
            }
        }
        if let Some(r) = ctx.input(|i| i.viewport().inner_rect) {
            let s = [r.width().round(), r.height().round()];
            if self.settings.window_size != Some(s) && !ctx.input(|i| i.viewport().minimized.unwrap_or(false)) {
                self.settings.window_size = Some(s);
                self.dirty_settings = true;
            }
        }
    }

    pub fn shutdown(&mut self) {
        self.yt.cancel();
        self.save_now();
        self.engine.shutdown();
    }

    // Test helpers (layout tests).
    #[doc(hidden)]
    pub fn test_open_rename(&mut self, id: u64) {
        self.rename = self.board.get(id).map(|s| (id, s.name.clone(), true));
    }
    #[doc(hidden)]
    pub fn test_status(&mut self, st: Option<Status>) {
        self.status_override = st;
    }
    #[doc(hidden)]
    pub fn test_play(&mut self, id: u64) {
        self.play(id);
    }
    #[doc(hidden)]
    pub fn test_close_dialogs(&mut self) {
        self.rename = None;
        self.toasts.clear();
    }
    #[doc(hidden)]
    pub fn test_youtube(&mut self, f: impl FnOnce(&mut crate::youtube::State)) {
        f(&mut self.yt.state.lock().unwrap());
    }
    #[doc(hidden)]
    pub fn test_toast(&mut self, text: &str, error: bool) {
        self.toast(text, error);
    }
}

/// A centered dialog with FreeMic's frame. `add` gets the palette.
fn modal(ctx: &egui::Context, id: &str, add: impl FnOnce(&mut egui::Ui, &Palette)) {
    let screen = ctx.content_rect();
    let w = (screen.width() - 48.0).clamp(220.0, 380.0);
    let dark = ctx.global_style().visuals.dark_mode;
    let p = if dark { &theme::DARK } else { &theme::LIGHT };
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(14)
        .shadow(ctx.global_style().visuals.window_shadow)
        .inner_margin(Margin::same(20));
    egui::Modal::new(egui::Id::unique(("modal", id))).frame(frame).backdrop_color(Color32::from_black_alpha(120)).show(ctx, |ui| {
        ui.set_width(w);
        ui.spacing_mut().item_spacing.y = 10.0;
        add(ui, p);
    });
}

fn key_chip(ui: &mut egui::Ui, p: &Palette, text: &str) {
    let g = widgets::one_line(ui, text, egui::FontId::monospace(12.5), p.text, 220.0);
    let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 16.0, 26.0), Sense::hover());
    ui.painter().rect(r, 6.0, p.raised, Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.painter().galley(pos2(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, p.text);
}

#[derive(Clone, Copy)]
enum MenuAction {
    Rename,
    Mode(PlayMode),
    Color(Option<[u8; 3]>),
    SetHotkey,
    ClearHotkey,
    Replace,
    MoveBy(i64),
    Remove,
}

fn sound_menu(ui: &mut egui::Ui, mode: PlayMode, color: Option<[u8; 3]>, has_hotkey: bool, idx: usize, len: usize) -> Option<MenuAction> {
    let p = *theme::pal(ui);
    let mut a = None;
    ui.set_min_width(210.0);
    if ui.button("Rename…").clicked() {
        a = Some(MenuAction::Rename);
    }
    ui.separator();
    if ui.radio(mode == PlayMode::Once, "Play once").clicked() {
        a = Some(MenuAction::Mode(PlayMode::Once));
    }
    if ui.radio(mode == PlayMode::Loop, "Loop until stopped").clicked() {
        a = Some(MenuAction::Mode(PlayMode::Loop));
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
        ui.painter().circle(r.center(), 8.0, p.pad, Stroke::new(1.0, p.border));
        if color.is_none() {
            ui.painter().circle_stroke(r.center(), 10.0, Stroke::new(1.5, p.text));
        }
        if resp.on_hover_text("Default color").clicked() {
            a = Some(MenuAction::Color(None));
        }
        for c in theme::PAD_COLORS {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
            ui.painter().circle_filled(r.center(), 8.0, Color32::from_rgb(c[0], c[1], c[2]));
            if color == Some(*c) {
                ui.painter().circle_stroke(r.center(), 10.0, Stroke::new(1.5, p.text));
            }
            if resp.clicked() {
                a = Some(MenuAction::Color(Some(*c)));
            }
        }
    });
    ui.separator();
    if ui.button("Set hotkey…").clicked() {
        a = Some(MenuAction::SetHotkey);
    }
    if has_hotkey && ui.button("Clear hotkey").clicked() {
        a = Some(MenuAction::ClearHotkey);
    }
    if ui.button("Replace audio file…").clicked() {
        a = Some(MenuAction::Replace);
    }
    ui.separator();
    if ui.add_enabled(idx > 0, egui::Button::new("Move left")).clicked() {
        a = Some(MenuAction::MoveBy(-1));
    }
    if ui.add_enabled(idx + 1 < len, egui::Button::new("Move right")).clicked() {
        a = Some(MenuAction::MoveBy(1));
    }
    ui.separator();
    if ui.button(RichText::new("Remove…").color(p.bad)).clicked() {
        a = Some(MenuAction::Remove);
    }
    a
}

fn setup_instructions(ui: &mut egui::Ui, p: &Palette) {
    #[cfg(windows)]
    {
        egui::Frame::new().fill(p.raised).corner_radius(10).inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Set up the virtual microphone (one time)").font(theme::font_bold(14.0)).color(p.text));
            for line in [
                "Windows only lets drivers create microphones, so FreeMic uses the free, signed VB-CABLE driver by VB-Audio.",
                "1. Download VB-CABLE from the official site.",
                "2. Extract the zip, right-click VBCABLE_Setup_x64.exe, choose Run as administrator, then Install Driver.",
                "3. Restart Windows if the installer asks, then click Re-check devices.",
            ] {
                ui.add(egui::Label::new(RichText::new(line).color(p.dim)).wrap());
            }
            widgets::hint(ui, "FreeMic never changes your default microphone or speakers.");
            ui.horizontal_wrapped(|ui| {
                let helper = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("install-vbcable.ps1")));
                if let Some(script) = helper.filter(|p| p.exists()) {
                    if widgets::button(ui, Kind::Primary, Some(Icon::Download), "Download and install VB-CABLE…")
                        .on_hover_text("Downloads the official package from vb-audio.com, checks its signature and starts VB-Audio's installer (Windows will ask for administrator approval).")
                        .clicked()
                    {
                        let _ = std::process::Command::new("powershell.exe")
                            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-NoExit", "-File"])
                            .arg(script)
                            .spawn();
                    }
                }
                if widgets::button(ui, Kind::Secondary, None, "Open the VB-CABLE web page").clicked() {
                    platform::open_url("https://vb-audio.com/Cable/");
                }
            });
        });
    }
    #[cfg(target_os = "linux")]
    {
        ui.add(
            egui::Label::new(
                RichText::new("FreeMic creates its virtual microphone through PulseAudio or PipeWire (pipewire-pulse). Make sure one of them is running for your user session, then click Re-check devices.")
                    .color(p.dim),
            )
            .wrap(),
        );
    }
}

fn fmt_time(s: f64) -> String {
    let s = s.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Paint one sound pad: color tile, loop/duration/hotkey details on top,
/// the name at the bottom (up to the rows that fit, elided with "…").
#[allow(clippy::too_many_arguments)]
fn paint_pad(ui: &egui::Ui, p: &Palette, rect: Rect, sound: &config::Sound, playing: bool, progress: Option<f64>, hovered: bool, dragging: bool) {
    let painter = ui.painter();
    let base = match sound.color {
        Some(c) => Color32::from_rgb(c[0], c[1], c[2]),
        None => p.pad,
    };
    let fg = if sound.color.is_some() { theme::text_on(base) } else { p.text };
    let mut fill = base;
    if hovered {
        fill = theme::mix(base, fg, 0.08);
    }
    if dragging {
        fill = fill.gamma_multiply(0.5);
    }
    let radius = 12.0;
    if hovered && !dragging {
        painter.rect_filled(rect.translate(vec2(0.0, 2.0)), radius, Color32::from_black_alpha(if ui.visuals().dark_mode { 70 } else { 22 }));
    }
    painter.rect_filled(rect, radius, fill);
    if playing {
        painter.rect_stroke(rect.expand(3.0), radius + 3.0, Stroke::new(2.0, p.accent), StrokeKind::Outside);
    }
    let pad = 10.0;
    let inner = rect.shrink(pad);
    let detail = fg.gamma_multiply(0.78);

    // Top row: [playing][loop][duration] ... [hotkey]
    let top_h = 18.0;
    let mut x = inner.left();
    let cy = inner.top() + top_h / 2.0;
    if playing {
        let r = Rect::from_center_size(pos2(x + 7.0, cy), Vec2::splat(14.0));
        icons::paint(painter, r, Icon::Speaker, fg);
        x += 14.0 + 6.0;
    }
    if sound.mode == PlayMode::Loop {
        let r = Rect::from_center_size(pos2(x + 7.0, cy), Vec2::splat(14.0));
        icons::paint(painter, r, Icon::Loop, detail);
        x += 14.0 + 6.0;
    }
    let mut left_end = x;
    if let Some(d) = sound.duration.filter(|d| *d > 0.0) {
        let g = widgets::one_line(ui, &fmt_time(d.max(1.0)), theme::font(11.5), detail, inner.right() - x);
        if x + g.size().x <= inner.right() {
            left_end = x + g.size().x;
            painter.galley(pos2(x, cy - g.size().y / 2.0), g, detail);
        }
    }
    if let Some(h) = &sound.hotkey {
        let max_w = inner.right() - left_end - 8.0 - 12.0;
        if max_w >= 28.0 {
            let g = widgets::one_line(ui, &hotkeys::pretty(h), theme::font_bold(11.0), fg, max_w);
            let chip = Rect::from_min_max(pos2(inner.right() - g.size().x - 12.0, cy - 9.0), pos2(inner.right(), cy + 9.0));
            painter.rect_filled(chip, 6.0, Color32::from_black_alpha(if fg == Color32::WHITE { 60 } else { 26 }));
            painter.galley(pos2(chip.left() + 6.0, cy - g.size().y / 2.0), g, fg);
        }
    }

    // Name, anchored to the bottom left.
    let font = theme::font_bold(14.5);
    let line_h = ui.fonts_mut(|f| f.row_height(&font));
    let name_h = inner.height() - top_h - 4.0;
    let max_rows = ((name_h / line_h).floor() as usize).clamp(1, 3);
    let g = widgets::rows(ui, &sound.name, font, fg, inner.width(), max_rows);
    let pos = pos2(inner.left(), inner.bottom() - g.size().y);
    painter.galley(pos, g, fg);

    // Progress through the sound (loops: through the current pass), along the bottom edge.
    if let Some(pos) = progress.filter(|_| playing) {
        let frac = match sound.duration {
            Some(d) if d > 0.0 => ((pos % d) / d) as f32,
            _ => 1.0,
        };
        let track = Rect::from_min_max(pos2(rect.left() + 10.0, rect.bottom() - 6.0), pos2(rect.right() - 10.0, rect.bottom() - 3.0));
        painter.rect_filled(track, 1.5, fg.gamma_multiply(0.22));
        painter.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * frac.clamp(0.0, 1.0), track.height())), 1.5, fg);
    }
}

impl eframe::App for FreeMicApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, position: usize) -> PlayTarget {
        PlayTarget { name: name.into(), path: PathBuf::new(), looped: false, position }
    }

    #[test]
    fn play_command_finds_sounds_by_name_or_button_position() {
        // ids in creation order, buttons reordered: id 7 is now the first button.
        let map: HashMap<u64, PlayTarget> = [(3, t("Airhorn", 2)), (5, t("Drum roll", 3)), (7, t("42", 1))].into_iter().collect();
        assert_eq!(find_target(&map, "airhorn"), Some(3));
        assert_eq!(find_target(&map, " Drum Roll "), Some(5));
        assert_eq!(find_target(&map, "#1"), Some(7));
        assert_eq!(find_target(&map, "3"), Some(5));
        assert_eq!(find_target(&map, "42"), Some(7), "an exact name wins over a position");
        assert_eq!(find_target(&map, "#4"), None);
        assert_eq!(find_target(&map, "nope"), None);
    }

    #[test]
    fn pad_text_color_is_readable_on_every_pad_color() {
        for c in theme::PAD_COLORS {
            assert_eq!(theme::text_on(Color32::from_rgb(c[0], c[1], c[2])), Color32::WHITE, "{c:?}");
        }
        assert_ne!(theme::text_on(Color32::from_rgb(240, 230, 140)), Color32::WHITE);
    }
}
