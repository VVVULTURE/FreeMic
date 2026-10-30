//! The user interface (egui). The soundboard is the main screen; audio and
//! application settings live on a separate screen so the board stays clean.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::audio::backend::DeviceInfo;
use crate::audio::engine::{DevStatus, Engine, EngineConfig};
use crate::audio::{percent_to_gain, Shared};
use crate::config::{self, Board, DeviceRef, PlayMode, Settings, Theme};
use crate::hotkeys::{self, Action, Hotkeys};
use crate::paths::Paths;
use crate::{library, platform};

const PALETTE: &[[u8; 3]] = &[
    [70, 110, 190],
    [60, 150, 110],
    [180, 120, 50],
    [170, 70, 70],
    [140, 90, 180],
    [60, 140, 160],
    [190, 90, 140],
    [110, 110, 120],
];

#[derive(PartialEq, Eq, Clone, Copy)]
enum View {
    Board,
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
}

impl FreeMicApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        paths: Paths,
        settings: Settings,
        board: Board,
        engine: Arc<Engine>,
        targets: Targets,
        start_minimized: bool,
        load_warnings: Vec<String>,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        let (e2, t2, c2) = (engine.clone(), targets.clone(), ctx.clone());
        let hotkeys = Hotkeys::new(Arc::new(move |a| {
            trigger(&e2, &t2, a);
            c2.request_repaint();
        }));
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
            start_minimized,
            first_frame: true,
        };
        app.apply_theme(&ctx);
        app.sync_targets();
        // Hooks for automated UI checks (screenshots); harmless when unset.
        if std::env::var("FREEMIC_TEST_VIEW").as_deref() == Ok("settings") {
            app.view = View::Settings;
            app.refresh_devices();
        }
        if let Some(id) = std::env::var("FREEMIC_TEST_PLAY").ok().and_then(|v| v.parse().ok()) {
            app.play(id);
        }
        for w in load_warnings {
            app.toast(w, true);
        }
        app
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
        if self.toasts.len() > 5 {
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

    // ---------------------------------------------------------------- panels

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.add(egui::Button::new(RichText::new("+ Add sounds").strong())).on_hover_text("Import MP3, WAV, OGG, FLAC or M4A files (you can also drop files on the window)").clicked() {
                self.import_dialog();
            }
            let playing = self.engine.now_playing();
            let stop = ui.add_enabled(playing.is_some(), egui::Button::new(RichText::new("Stop").strong()));
            let stop = match &self.settings.stop_hotkey {
                Some(h) => stop.on_hover_text(format!("Stop the soundboard ({})", hotkeys::pretty(h))),
                None => stop.on_hover_text("Stop the soundboard"),
            };
            if stop.clicked() {
                self.engine.stop_sound();
            }
            ui.separator();
            match &playing {
                Some((np, pos)) => {
                    let name = self.board.get(np.sound_id).map(|s| s.name.clone()).unwrap_or_default();
                    let icon = if np.looped { "Looping:" } else { "Playing:" };
                    ui.label(RichText::new(format!("{icon} {name}")).strong());
                    ui.label(RichText::new(fmt_time(*pos)).weak().monospace());
                }
                None => {
                    ui.label(RichText::new("Nothing playing").weak());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.view == View::Settings { "Back to soundboard" } else { "Settings" };
                if ui.button(label).clicked() {
                    self.view = if self.view == View::Settings { View::Board } else { View::Settings };
                    if self.view == View::Settings {
                        self.refresh_devices();
                    }
                }
                ui.separator();
                let mut changed = false;
                let mic_icon = if self.settings.mic_muted { "Mic muted" } else { "Mute mic" };
                if ui
                    .selectable_label(self.settings.mic_muted, mic_icon)
                    .on_hover_text(if self.settings.mic_muted { "Microphone muted (click to unmute)" } else { "Mute your microphone (soundboard keeps playing)" })
                    .clicked()
                {
                    self.settings.mic_muted = !self.settings.mic_muted;
                    changed = true;
                }
                changed |= ui
                    .add(egui::Slider::new(&mut self.settings.mic_volume, 0.0..=200.0).show_value(false).trailing_fill(true))
                    .on_hover_text(format!("Microphone volume {:.0}%", self.settings.mic_volume))
                    .changed();
                ui.label("Mic");
                changed |= ui
                    .add(egui::Slider::new(&mut self.settings.sound_volume, 0.0..=100.0).show_value(false).trailing_fill(true))
                    .on_hover_text(format!("Soundboard volume {:.0}%", self.settings.sound_volume))
                    .changed();
                ui.label("Sounds");
                if changed {
                    self.settings_changed();
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let st = self.engine.status();
        ui.horizontal(|ui| {
            let (col, text) = match &st.output {
                DevStatus::Running { name, .. } => {
                    let mic = st.virtual_mic.mic_name.clone().unwrap_or_else(|| name.clone());
                    (Color32::from_rgb(70, 170, 90), format!("Virtual mic active — in Discord/games select \"{mic}\""))
                }
                DevStatus::Starting => (Color32::from_rgb(200, 170, 60), "Virtual mic starting…".to_string()),
                DevStatus::Error(e) => (Color32::from_rgb(210, 80, 70), format!("Virtual mic unavailable: {e}")),
                DevStatus::Off => (Color32::GRAY, "Virtual mic off".to_string()),
            };
            dot(ui, col);
            let r = ui.label(RichText::new(text).small());
            if !matches!(st.output, DevStatus::Running { .. }) && r.clicked() {
                self.view = View::Settings;
            }
            ui.separator();
            let (col, text) = match &st.mic {
                DevStatus::Running { name, .. } if self.settings.mic_muted => (Color32::from_rgb(200, 170, 60), format!("{name} (muted)")),
                DevStatus::Running { name, .. } => (Color32::from_rgb(70, 170, 90), name.clone()),
                DevStatus::Starting => (Color32::from_rgb(200, 170, 60), "Microphone starting…".into()),
                DevStatus::Error(e) => (Color32::from_rgb(210, 80, 70), format!("Microphone: {e}")),
                DevStatus::Off => (Color32::GRAY, "Microphone passthrough off".into()),
            };
            dot(ui, col);
            ui.label(RichText::new(text).small());
        });
    }

    fn setup_banner(&mut self, ui: &mut egui::Ui) {
        let st = self.engine.status();
        if st.virtual_mic.feed.is_some() || matches!(st.output, DevStatus::Starting) {
            return;
        }
        let Some(problem) = st.virtual_mic.problem.clone() else { return };
        egui::Frame::new()
            .fill(ui.visuals().warn_fg_color.gamma_multiply(0.15))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(10)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Virtual microphone not set up.").strong());
                    ui.label(problem);
                    if ui.button("Set up…").clicked() {
                        self.view = View::Settings;
                        self.refresh_devices();
                    }
                });
            });
        ui.add_space(6.0);
    }

    fn board_view(&mut self, ui: &mut egui::Ui) {
        self.setup_banner(ui);
        if self.board.sounds.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.3);
                ui.label(RichText::new("Your soundboard is empty").size(20.0));
                ui.label("Click “+ Add sounds” or drop audio files onto this window.");
                ui.add_space(8.0);
                if ui.button("+ Add sounds").clicked() {
                    self.import_dialog();
                }
            });
            return;
        }
        if self.board.sounds.len() > 12 {
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter sounds").desired_width(220.0));
                if !self.filter.is_empty() && ui.small_button("Clear").clicked() {
                    self.filter.clear();
                }
            });
            ui.add_space(4.0);
        }

        let playing = self.engine.now_playing();
        let filter = self.filter.to_lowercase();
        let visible: Vec<usize> = (0..self.board.sounds.len())
            .filter(|&i| filter.is_empty() || self.board.sounds[i].name.to_lowercase().contains(&filter))
            .collect();

        let bw = self.settings.button_width;
        let bh = (bw * 0.45).max(52.0);
        let gap = 8.0;
        let mut action_play: Option<u64> = None;
        let mut menu_action: Option<(u64, MenuAction)> = None;
        let mut drop_target: Option<usize> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let width = ui.available_width();
            let cols = (((width + gap) / (bw + gap)).floor() as usize).max(1);
            let rows = visible.len().div_ceil(cols);
            let (area, _) = ui.allocate_exact_size(Vec2::new(width, rows as f32 * (bh + gap)), Sense::hover());
            let pointer = ui.ctx().pointer_interact_pos();
            for (n, &idx) in visible.iter().enumerate() {
                let (r, c) = (n / cols, n % cols);
                let rect = Rect::from_min_size(
                    area.min + Vec2::new(c as f32 * (bw + gap), r as f32 * (bh + gap)),
                    Vec2::new(bw, bh),
                );
                let sound = &self.board.sounds[idx];
                let id = ui.make_persistent_id(("sound", sound.id));
                let resp = ui.interact(rect, id, Sense::click_and_drag());
                let is_playing = playing.as_ref().map(|(np, _)| np.sound_id) == Some(sound.id);
                let progress = playing.as_ref().filter(|_| is_playing).map(|(_, pos)| *pos);
                paint_button(ui, rect, sound, is_playing, progress, resp.hovered(), self.drag_from == Some(idx));

                if resp.clicked() {
                    action_play = Some(sound.id);
                }
                if resp.drag_started() && filter.is_empty() {
                    self.drag_from = Some(idx);
                }
                if self.drag_from.is_some() && pointer.is_some_and(|p| rect.contains(p)) {
                    drop_target = Some(idx);
                    if self.drag_from != Some(idx) {
                        ui.painter().rect_stroke(rect.expand(3.0), 8.0, Stroke::new(2.0, ui.visuals().selection.stroke.color), egui::StrokeKind::Outside);
                    }
                }
                let hover = match &sound.hotkey {
                    Some(h) => format!("Click to play · right-click for options · drag to move\nHotkey: {}", hotkeys::pretty(h)),
                    None => "Click to play · right-click for options · drag to move".to_string(),
                };
                let resp = resp.on_hover_text(hover);
                let sid = sound.id;
                let mode = sound.mode;
                let has_hotkey = sound.hotkey.is_some();
                resp.context_menu(|ui| {
                    if let Some(a) = sound_menu(ui, mode, has_hotkey, idx, self.board.sounds.len()) {
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

    fn dialogs(&mut self, ctx: &egui::Context) {
        // Rename.
        if let Some((id, mut text, first)) = self.rename.take() {
            let mut keep = true;
            let mut commit = false;
            egui::Modal::new(egui::Id::unique("rename")).show(ctx, |ui| {
                ui.heading("Rename sound");
                let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(280.0).char_limit(60));
                if first {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    commit = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        commit = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
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
            egui::Modal::new(egui::Id::unique("remove")).show(ctx, |ui| {
                ui.heading("Remove sound?");
                ui.label(format!("Remove \"{name}\" from the soundboard?"));
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        if self.engine.now_playing().map(|(n, _)| n.sound_id) == Some(id) {
                            self.engine.stop_sound();
                        }
                        library::remove(&self.paths, &mut self.board, id);
                        self.board_changed();
                        close = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
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
            egui::Modal::new(egui::Id::unique("hotkey")).show(ctx, |ui| {
                ui.heading("Set hotkey");
                ui.label("Press the key combination now (e.g. Ctrl+Shift+1 or F9).");
                ui.label(RichText::new("Letters and digits need Ctrl, Alt or Shift so typing in chat does not trigger sounds.").small().weak());
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
                if ui.button("Cancel").clicked() {
                    result = Some(None);
                }
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

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        let st = self.engine.status();
        let mut changed = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(640.0);

            // ---------------- Virtual microphone
            section(ui, "Virtual microphone");
            match &st.output {
                DevStatus::Running { name, rate, latency_ms } => {
                    ui.horizontal(|ui| {
                        dot(ui, Color32::from_rgb(70, 170, 90));
                        ui.label(format!("Active: FreeMic is sending audio to \"{name}\" ({rate} Hz, {latency_ms:.0} ms buffer)."));
                    });
                    if let Some(mic) = &st.virtual_mic.mic_name {
                        ui.label(RichText::new(format!("In Discord, games and other apps choose the microphone: {mic}")).strong());
                    }
                }
                DevStatus::Starting => {
                    ui.label("Starting…");
                }
                DevStatus::Error(e) => {
                    ui.horizontal_wrapped(|ui| {
                        dot(ui, Color32::from_rgb(210, 80, 70));
                        ui.label(e.clone());
                    });
                }
                DevStatus::Off => {}
            }
            if st.virtual_mic.feed.is_none() {
                setup_instructions(ui);
            }
            ui.horizontal(|ui| {
                ui.label("Send the mix to:");
                let current = self.settings.output_device.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "Automatic (detect virtual cable)".into());
                egui::ComboBox::from_id_salt("outdev").selected_text(current).width(300.0).show_ui(ui, |ui| {
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
                if ui.button("Re-check devices").clicked() {
                    self.engine.rescan();
                    self.refresh_devices();
                }
            });
            if self.settings.output_device.is_some() {
                ui.label(RichText::new("Advanced: only pick a device that is the input side of a virtual cable. Picking your speakers would play everything out loud.").small().weak());
            }

            // ---------------- Microphone
            section(ui, "Your microphone");
            changed |= ui.checkbox(&mut self.settings.mic_enabled, "Pass my microphone through to the virtual microphone").changed();
            ui.add_enabled_ui(self.settings.mic_enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Microphone:");
                    let current = self.settings.mic_device.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "System default".into());
                    let avoid = st.virtual_mic.mic_name.clone();
                    egui::ComboBox::from_id_salt("micdev").selected_text(current).width(300.0).show_ui(ui, |ui| {
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
                        ui.label(RichText::new(format!("Capturing \"{name}\" at {rate} Hz (~{latency_ms:.0} ms buffer)")).small().weak());
                    }
                    DevStatus::Error(e) => {
                        ui.colored_label(Color32::from_rgb(210, 80, 70), e);
                    }
                    _ => {}
                }
                changed |= slider_row(ui, "Microphone volume", &mut self.settings.mic_volume, 200.0);
                changed |= ui.checkbox(&mut self.settings.mic_muted, "Mute microphone").changed();
            });

            // ---------------- Levels
            section(ui, "Volume");
            changed |= slider_row(ui, "Soundboard volume", &mut self.settings.sound_volume, 100.0);
            changed |= slider_row(ui, "Virtual microphone output", &mut self.settings.master_volume, 100.0);
            let s = self.engine.shared.clone();
            let lv = [s.mic_peak.take(), s.sound_peak.take(), s.out_peak.take()];
            for (m, v) in self.meter.iter_mut().zip(lv) {
                *m = if v > *m { v } else { *m * 0.85 };
            }
            meter(ui, "Microphone", self.meter[0]);
            meter(ui, "Soundboard", self.meter[1]);
            meter(ui, "Output", self.meter[2]);
            let lg = s.limiter_gain.load();
            s.limiter_gain.store(1.0);
            if lg < 0.9 {
                ui.label(RichText::new("Peak limiter is working to prevent clipping — consider lowering a volume.").small().color(ui.visuals().warn_fg_color));
            }

            // ---------------- Monitoring
            section(ui, "Hear the soundboard yourself (optional)");
            ui.label(RichText::new("Plays only the soundboard (never your microphone) to a device of your choice, e.g. your headphones.").small().weak());
            ui.horizontal(|ui| {
                ui.label("Monitor on:");
                let current = self.settings.monitor_device.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "Off".into());
                let feed = st.virtual_mic.feed.as_ref().map(|f| f.id.clone());
                egui::ComboBox::from_id_salt("mondev").selected_text(current).width(300.0).show_ui(ui, |ui| {
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
                changed |= slider_row(ui, "Monitor volume", &mut self.settings.monitor_volume, 100.0);
                if let DevStatus::Error(e) = &st.monitor {
                    ui.colored_label(Color32::from_rgb(210, 80, 70), e);
                }
            }

            // ---------------- Hotkeys
            section(ui, "Hotkeys");
            if let Some(u) = &self.hotkeys.unavailable {
                ui.label(RichText::new(u).color(ui.visuals().warn_fg_color));
                #[cfg(target_os = "linux")]
                ui.label(RichText::new("On Wayland, bind keys in your compositor to commands like  freemic --play \"Name\"  or  freemic --stop.").small());
            }
            ui.horizontal(|ui| {
                ui.label("Stop all sounds:");
                let t = self.settings.stop_hotkey.as_deref().map(hotkeys::pretty).unwrap_or_else(|| "none".into());
                ui.label(RichText::new(t).monospace());
                if ui.button("Set…").clicked() {
                    self.capturing = Some(HotkeyTarget::Stop);
                }
                if self.settings.stop_hotkey.is_some() && ui.button("Clear").clicked() {
                    self.settings.stop_hotkey = None;
                    self.dirty_settings = true;
                    self.sync_targets();
                }
            });
            ui.label(RichText::new("Per-sound hotkeys: right-click a sound > Set hotkey. Hotkeys work while other apps and games have focus.").small().weak());
            for p in &self.hotkey_problems {
                ui.colored_label(ui.visuals().warn_fg_color, p);
            }

            // ---------------- General
            section(ui, "General");
            ui.horizontal(|ui| {
                ui.label("Theme:");
                for (t, n) in [(Theme::System, "System"), (Theme::Dark, "Dark"), (Theme::Light, "Light")] {
                    if ui.selectable_label(self.settings.theme == t, n).clicked() {
                        self.settings.theme = t;
                        self.apply_theme(ui.ctx());
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("New sounds play:");
                changed |= ui.radio_value(&mut self.settings.default_mode, PlayMode::Once, "Once").changed();
                changed |= ui.radio_value(&mut self.settings.default_mode, PlayMode::Loop, "Looped").changed();
            });
            ui.horizontal(|ui| {
                ui.label("Button size:");
                changed |= ui.add(egui::Slider::new(&mut self.settings.button_width, 100.0..=260.0).show_value(false)).changed();
            });
            changed |= ui
                .checkbox(&mut self.settings.copy_imports, "Copy added files into FreeMic's library (recommended)")
                .on_hover_text("Keeps the soundboard working if the original files are moved or deleted.")
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
                ui.label(RichText::new("Portable mode: if you move the FreeMic folder, turn this off and on again.").small().weak());
            }

            section(ui, "About");
            ui.label(format!("FreeMic {} — audio backend: {}", env!("CARGO_PKG_VERSION"), self.engine.backend_name()));
            ui.label(format!(
                "{} data folder: {}",
                if self.paths.portable { "Portable" } else { "Installed" },
                self.paths.data_dir.display()
            ));
            ui.horizontal(|ui| {
                if ui.button("Open data folder").clicked() {
                    platform::open_folder(&self.paths.data_dir);
                }
                if ui.button("Open log file").clicked() {
                    platform::open_url(&self.paths.log_file().to_string_lossy());
                }
            });
            ui.add_space(20.0);
        });
        if changed {
            self.settings.sanitize();
            self.settings_changed();
        }
    }

    fn show_toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-12.0, -36.0))
            .show(ctx, |ui| {
                for t in &self.toasts {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(420.0);
                        let col = if t.error { Color32::from_rgb(220, 90, 80) } else { ui.visuals().text_color() };
                        ui.label(RichText::new(&t.text).color(col));
                    });
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
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

fn sound_menu(ui: &mut egui::Ui, mode: PlayMode, has_hotkey: bool, idx: usize, len: usize) -> Option<MenuAction> {
    let mut a = None;
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
        for c in PALETTE {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(c[0], c[1], c[2]));
            if resp.clicked() {
                a = Some(MenuAction::Color(Some(*c)));
            }
        }
        if ui.small_button("Default").on_hover_text("Default color").clicked() {
            a = Some(MenuAction::Color(None));
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
    ui.horizontal(|ui| {
        if ui.add_enabled(idx > 0, egui::Button::new("Move left")).clicked() {
            a = Some(MenuAction::MoveBy(-1));
        }
        if ui.add_enabled(idx + 1 < len, egui::Button::new("Move right")).clicked() {
            a = Some(MenuAction::MoveBy(1));
        }
    });
    ui.separator();
    if ui.button("Remove…").clicked() {
        a = Some(MenuAction::Remove);
    }
    a
}

fn setup_instructions(ui: &mut egui::Ui) {
    #[cfg(windows)]
    {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Set up the virtual microphone (one time)").strong());
            ui.label("Windows only lets drivers create microphones, so FreeMic uses the free, signed VB-CABLE driver by VB-Audio.");
            ui.label("1. Download VB-CABLE from the official site.");
            ui.label("2. Extract the zip, right-click VBCABLE_Setup_x64.exe > Run as administrator > Install Driver.");
            ui.label("3. Restart Windows if the installer asks, then click Re-check devices.");
            ui.label(RichText::new("FreeMic never changes your default microphone or speakers.").small().weak());
            ui.horizontal(|ui| {
                let helper = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("install-vbcable.ps1")));
                if let Some(script) = helper.filter(|p| p.exists()) {
                    if ui
                        .button("Download and install VB-CABLE…")
                        .on_hover_text("Downloads the official package from vb-audio.com, checks its signature and starts VB-Audio's installer (Windows will ask for administrator approval).")
                        .clicked()
                    {
                        let _ = std::process::Command::new("powershell.exe")
                            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-NoExit", "-File"])
                            .arg(script)
                            .spawn();
                    }
                }
                if ui.button("Open the VB-CABLE web page").clicked() {
                    platform::open_url("https://vb-audio.com/Cable/");
                }
            });
        });
    }
    #[cfg(target_os = "linux")]
    {
        ui.label("FreeMic creates its virtual microphone through PulseAudio or PipeWire (pipewire-pulse). Make sure one of them is running for your user session, then click Re-check devices.");
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).size(16.0).strong());
    ui.separator();
}

fn slider_row(ui: &mut egui::Ui, label: &str, v: &mut f32, max: f32) -> bool {
    ui.horizontal(|ui| {
        fixed_label(ui, 190.0, RichText::new(label));
        ui.add(egui::Slider::new(v, 0.0..=max).suffix("%").fixed_decimals(0).trailing_fill(true)).changed()
    })
    .inner
}

fn meter(ui: &mut egui::Ui, label: &str, peak: f32) {
    ui.horizontal(|ui| {
        fixed_label(ui, 190.0, RichText::new(label).small());
        let (rect, _) = ui.allocate_exact_size(Vec2::new(260.0, 10.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
        let db = 20.0 * peak.max(1e-5).log10();
        let frac = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let col = if db > -1.5 { Color32::from_rgb(220, 80, 60) } else if db > -9.0 { Color32::from_rgb(220, 180, 60) } else { Color32::from_rgb(80, 180, 100) };
        let mut fill = rect;
        fill.set_width(rect.width() * frac);
        p.rect_filled(fill, 3.0, col);
    });
}

/// Left-aligned label in a fixed-width column (keeps sliders and meters lined up).
fn fixed_label(ui: &mut egui::Ui, width: f32, text: RichText) {
    ui.allocate_ui_with_layout(Vec2::new(width, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        ui.label(text);
    });
}

fn dot(ui: &mut egui::Ui, c: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, c);
}

fn fmt_time(s: f64) -> String {
    let s = s.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn paint_button(ui: &egui::Ui, rect: Rect, sound: &config::Sound, playing: bool, progress: Option<f64>, hovered: bool, dragging: bool) {
    let p = ui.painter();
    let dark = ui.visuals().dark_mode;
    let base = match sound.color {
        Some(c) => Color32::from_rgb(c[0], c[1], c[2]),
        None if dark => Color32::from_rgb(52, 56, 66),
        None => Color32::from_rgb(222, 226, 234),
    };
    let fill = if hovered { base.gamma_multiply(1.15) } else { base };
    let text_col = if sound.color.is_some() || dark { Color32::WHITE } else { Color32::from_rgb(30, 30, 36) };
    p.rect_filled(rect, 8.0, if dragging { fill.gamma_multiply(0.6) } else { fill });
    if playing {
        // Progress shading for Play-Once sounds; loops show a solid highlight.
        let accent = Color32::from_rgb(90, 200, 120);
        p.rect_stroke(rect, 8.0, Stroke::new(3.0, accent), egui::StrokeKind::Inside);
        if let Some(pos) = progress {
            // Play-once: bar shows progress through the file. Loop: progress through the current pass.
            let frac = match sound.duration {
                Some(d) if d > 0.0 => ((pos % d) / d) as f32,
                _ => 1.0,
            };
            let bar = Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y - 5.0), Vec2::new(rect.width() * frac.clamp(0.0, 1.0), 5.0));
            p.rect_filled(bar, CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }, accent.gamma_multiply(0.8));
        }
    }
    let has_badges = sound.mode == PlayMode::Loop || sound.hotkey.is_some();
    let text_rect = if has_badges { Rect::from_min_max(rect.min + Vec2::new(0.0, 14.0), rect.max) } else { rect };
    let mut galley = p.layout(sound.name.clone(), FontId::proportional(15.0), text_col, rect.width() - 16.0);
    if galley.size().y > text_rect.height() - 6.0 {
        galley = p.layout(sound.name.clone(), FontId::proportional(12.0), text_col, rect.width() - 12.0);
    }
    let pos = text_rect.center() - galley.size() / 2.0;
    p.with_clip_rect(text_rect.shrink(4.0)).galley(pos, galley, text_col);
    let mut badges = String::new();
    if sound.mode == PlayMode::Loop {
        badges.push_str("LOOP");
    }
    if let Some(h) = &sound.hotkey {
        if !badges.is_empty() {
            badges.push(' ');
        }
        badges.push_str(&hotkeys::pretty(h));
    }
    if !badges.is_empty() {
        p.text(rect.right_top() + Vec2::new(-6.0, 4.0), Align2::RIGHT_TOP, badges, FontId::proportional(11.0), text_col.gamma_multiply(0.8));
    }
}

impl eframe::App for FreeMicApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
        // Files dropped onto the window.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !dropped.is_empty() {
            self.import(dropped);
        }
        // Keyboard: Escape stops the soundboard when the window is focused.
        if self.capturing.is_none() && self.rename.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.engine.stop_sound();
        }

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| match self.view {
            View::Board => self.board_view(ui),
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

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_now();
        self.engine.shutdown();
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
}
