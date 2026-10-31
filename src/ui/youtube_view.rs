//! The "YouTube" screen: search (or paste a link), then save the audio of one
//! video into the library and onto the soundboard.

use eframe::egui::{self, Align, Layout, Margin, Rect, RichText, Sense, Stroke, UiBuilder, Vec2, pos2, vec2};

use super::icons::{self, Icon};
use super::theme::{self, Palette};
use super::widgets::{self, Kind};
use super::{FreeMicApp, fmt_time};
use crate::config::PlayMode;
use crate::library;
use crate::youtube::{self, Busy, Job, Link, Video};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Mode {
    #[default]
    Search,
    Music,
}

#[derive(Default)]
pub(super) struct YtForm {
    pub mode: Mode,
    pub query: String,
    pub artist: String,
    pub song: String,
    /// Shown above the results (e.g. "only this video, not the playlist").
    pub note: Option<String>,
}

impl FreeMicApp {
    fn yt_out_dir(&self) -> std::path::PathBuf {
        self.paths.sounds_dir().join("youtube")
    }

    /// The board sound that already holds video `id`, if any.
    fn yt_on_board(&self, id: &str) -> Option<u64> {
        let dir = self.yt_out_dir();
        self.board
            .sounds
            .iter()
            .find(|s| {
                let p = self.paths.resolve(&s.file);
                p.parent() == Some(dir.as_path()) && p.file_stem().is_some_and(|st| st == id)
            })
            .map(|s| s.id)
    }

    /// Add finished downloads to the soundboard.
    pub(super) fn yt_poll(&mut self) {
        let finished: Vec<youtube::Finished> = std::mem::take(&mut self.yt.state.lock().unwrap().finished);
        for f in finished {
            self.yt_add(&f.video, &f.path);
        }
    }

    fn yt_add(&mut self, v: &Video, path: &std::path::Path) {
        if self.yt_on_board(&v.id).is_some() {
            self.toast(format!("\"{}\" is already on your soundboard", v.title), false);
            return;
        }
        match library::add_library_file(&self.paths, &mut self.board, path, &v.title, PlayMode::Once) {
            Ok(_) => {
                self.toast(format!("Added \"{}\" to your soundboard", v.title), false);
                self.board_changed();
                self.save_now();
            }
            Err(e) => self.toast(format!("The download of \"{}\" cannot be played: {e}", v.title), true),
        }
    }

    fn yt_download(&mut self, v: &Video) {
        // Already in the library (e.g. removed from the board earlier): no need to download again.
        let dir = self.yt_out_dir();
        let existing = std::fs::read_dir(&dir).ok().and_then(|rd| {
            rd.flatten().map(|e| e.path()).find(|p| p.is_file() && p.file_stem().is_some_and(|s| s == v.id.as_str()))
        });
        match existing {
            Some(p) => self.yt_add(v, &p),
            None => self.yt.send(Job::Download { video: v.clone() }),
        }
    }

    pub(super) fn yt_submit(&mut self) {
        self.yt_form.note = None;
        match self.yt_form.mode {
            Mode::Search => {
                let q = self.yt_form.query.trim().to_string();
                if q.is_empty() {
                    return;
                }
                match youtube::parse_link(&q) {
                    Some(Link::Video { id, in_list }) => {
                        if in_list {
                            self.yt_form.note = Some("This link is part of a playlist or mix. Only this one video will be downloaded.".into());
                        }
                        self.yt.send(Job::Lookup { id });
                    }
                    Some(Link::NotAVideo) => {
                        self.yt.state.lock().unwrap().error = Some(
                            "This link isn't a single video (it's a playlist, channel or page). FreeMic only downloads one video at a time, so paste the link of the video itself.".into(),
                        );
                    }
                    Some(Link::NotYouTube) => {
                        self.yt.state.lock().unwrap().error = Some("Only YouTube links are supported.".into());
                    }
                    None => self.yt.send(Job::Search { query: q, music: None }),
                }
            }
            Mode::Music => {
                let (a, s) = (self.yt_form.artist.trim().to_string(), self.yt_form.song.trim().to_string());
                if let Some(q) = youtube::music_query(&a, &s) {
                    self.yt.send(Job::Search { query: q, music: Some((a, s)) });
                }
            }
        }
    }

    pub(super) fn youtube_view(&mut self, ui: &mut egui::Ui) {
        let p = *theme::pal(ui);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let avail = ui.available_width();
            let w = (avail - 12.0).min(760.0);
            ui.horizontal(|ui| {
                ui.add_space(((avail - w) / 2.0).max(6.0));
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 10.0;
                    self.youtube_body(ui, &p);
                    ui.add_space(8.0);
                });
            });
        });
    }

    fn youtube_body(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let (tool, busy, progress, results, ranked, error, searched) = {
            let st = self.yt.state.lock().unwrap();
            (st.tool.clone(), st.busy.clone(), st.progress, st.results.clone(), st.ranked, st.error.clone(), st.searched.clone())
        };
        ui.label(RichText::new("Get sounds from YouTube").font(theme::font_bold(19.0)).color(p.text));
        ui.add_space(-4.0);
        ui.add(
            egui::Label::new(
                RichText::new("Search for a sound or a song, or paste the link of one video. FreeMic saves only its audio on this computer and adds it to your soundboard.")
                    .color(p.dim),
            )
            .wrap(),
        );

        if tool.is_none() && busy.is_none() {
            self.youtube_setup_card(ui, p, error.as_deref());
            return;
        }

        // ---- search form
        let mut submit = false;
        egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(12).inner_margin(Margin::symmetric(16, 14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (m, label) in [(Mode::Search, "Sounds & videos"), (Mode::Music, "Music")] {
                    let kind = if self.yt_form.mode == m { Kind::Primary } else { Kind::Ghost };
                    if widgets::button(ui, kind, None, label).clicked() {
                        self.yt_form.mode = m;
                    }
                }
            });
            let enabled = busy.is_none();
            ui.add_enabled_ui(enabled, |ui| match self.yt_form.mode {
                Mode::Search => {
                    let is_link = youtube::parse_link(&self.yt_form.query).is_some();
                    let label = if is_link { "Get video" } else { "Search" };
                    let btn_w = 13.0 * 2.0 + 16.0 + 7.0 + widgets::text_width(ui, label, theme::font_bold(13.5));
                    let field_w = (ui.available_width() - btn_w - 8.0).max(120.0);
                    ui.horizontal_wrapped(|ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.yt_form.query)
                                .hint_text("Search YouTube or paste a video link")
                                .desired_width(field_w - 20.0)
                                .margin(Margin::symmetric(10, 7)),
                        );
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            submit = true;
                        }
                        let icon = if is_link { Icon::Download } else { Icon::Search };
                        if widgets::button(ui, Kind::Primary, Some(icon), label).clicked() {
                            submit = true;
                        }
                    });
                }
                Mode::Music => {
                    let wide = ui.available_width() >= 520.0;
                    let field_w = if wide { (ui.available_width() - 16.0) / 2.0 } else { ui.available_width() };
                    let field = |ui: &mut egui::Ui, text: &mut String, hint: &str| {
                        let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(field_w - 20.0).margin(Margin::symmetric(10, 7)));
                        r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    };
                    if wide {
                        ui.horizontal(|ui| {
                            submit |= field(ui, &mut self.yt_form.artist, "Artist");
                            submit |= field(ui, &mut self.yt_form.song, "Song");
                        });
                    } else {
                        submit |= field(ui, &mut self.yt_form.artist, "Artist");
                        submit |= field(ui, &mut self.yt_form.song, "Song");
                    }
                    ui.horizontal_wrapped(|ui| {
                        if widgets::button(ui, Kind::Primary, Some(Icon::Music), "Find song").clicked() {
                            submit = true;
                        }
                    });
                    widgets::hint(ui, "Same search as YT-Downloader: fill in the artist, the song, or both. Results are ranked by how well they match.");
                }
            });
        });
        if submit && busy.is_none() {
            self.yt_submit();
        }

        // ---- status
        if let Some(b) = &busy {
            let text = match b {
                Busy::Setup => "Getting yt-dlp ready…".to_string(),
                Busy::Search => "Searching YouTube…".to_string(),
                Busy::Lookup => "Looking up the video…".to_string(),
                Busy::Download(id) => {
                    let t = results.iter().find(|v| &v.id == id).map(|v| v.title.clone()).unwrap_or_default();
                    format!("Downloading \"{t}\"…")
                }
            };
            egui::Frame::new().fill(p.raised).corner_radius(10).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(16.0).color(p.accent));
                    let cancel_w = 13.0 * 2.0 + widgets::text_width(ui, "Cancel", theme::font_bold(13.5));
                    let tw = (ui.available_width() - cancel_w - 12.0).max(40.0);
                    let g = widgets::one_line(ui, &text, theme::font(14.0), p.text, tw);
                    let (r, _) = ui.allocate_exact_size(vec2(tw, 20.0), Sense::hover());
                    ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, p.text);
                    if widgets::button(ui, Kind::Ghost, None, "Cancel").clicked() {
                        self.yt.cancel();
                    }
                });
                if let Some(f) = progress {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, p.surface);
                    ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * f.clamp(0.0, 1.0), r.height())), 3.0, p.accent);
                }
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(e) = &error {
            egui::Frame::new()
                .fill(theme::mix(p.bad, p.bg, 0.88))
                .stroke(Stroke::new(1.0, theme::mix(p.bad, p.bg, 0.6)))
                .corner_radius(10)
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::hover());
                        icons::paint(ui.painter(), Rect::from_min_size(r.min + vec2(0.0, 1.0), Vec2::splat(16.0)), Icon::Warning, p.bad);
                        ui.add(egui::Label::new(RichText::new(e).color(p.text)).wrap());
                    });
                });
        }
        if let Some(n) = &self.yt_form.note {
            widgets::status_line(ui, p.warn, n.clone());
        }

        // ---- results
        if results.is_empty() {
            if let (Some(q), None) = (&searched, &busy) {
                widgets::hint(ui, format!("No videos found for {q}."));
            }
        }
        let downloading = match &busy {
            Some(Busy::Download(id)) => Some(id.clone()),
            _ => None,
        };
        let mut start: Option<Video> = None;
        for (i, v) in results.iter().enumerate() {
            let best = ranked && i == 0;
            let on_board = self.yt_on_board(&v.id).is_some();
            // The main action is the best match, or the only result (a pasted link).
            let primary = best || results.len() == 1;
            if let Some(go) = result_row(ui, p, v, best, primary, on_board, downloading.as_deref(), progress, busy.is_some()) {
                if go {
                    start = Some(v.clone());
                }
            }
        }
        if let Some(v) = start {
            self.yt_download(&v);
        }

        ui.add_space(4.0);
        widgets::hint(
            ui,
            format!(
                "Only the single video you choose is ever downloaded, never a playlist. Audio is saved in {}. Only download audio you have the right to use.",
                self.yt_out_dir().display()
            ),
        );
    }

    fn youtube_setup_card(&mut self, ui: &mut egui::Ui, p: &Palette, error: Option<&str>) {
        widgets::card(ui, "Set up YouTube downloads", None, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new("FreeMic uses yt-dlp, the free and open-source downloader that YT-Downloader is built on. It is downloaded once from yt-dlp's official GitHub releases and checked against the release's published SHA-256 checksum before it is used.")
                        .color(p.dim),
                )
                .wrap(),
            );
            #[cfg(windows)]
            let size = "about 24 MB";
            #[cfg(not(windows))]
            let size = "about 41 MB";
            widgets::hint(ui, format!("Download size {size}. FreeMic looks for a newer yt-dlp at most every 6 hours, and only while you use YouTube features."));
            if let Some(e) = error {
                widgets::status_line(ui, p.bad, e.to_string());
            }
            if widgets::button(ui, Kind::Primary, Some(Icon::Download), "Set up YouTube downloads").clicked() {
                self.yt.send(Job::Setup { force: false });
            }
        });
    }

    /// Card in Settings.
    pub(super) fn youtube_settings_card(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let (tool, busy) = {
            let st = self.yt.state.lock().unwrap();
            (st.tool.clone(), st.busy.clone())
        };
        widgets::card(ui, "YouTube downloads", Some("yt-dlp downloads the audio of YouTube videos for FreeMic."), |ui| {
            match &tool {
                Some(t) => {
                    let ago = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs().saturating_sub(t.checked))
                        .unwrap_or(0);
                    let ago = match ago {
                        0..=119 => "just now".to_string(),
                        120..=7199 => format!("{} minutes ago", ago / 60),
                        7200..=172_799 => format!("{} hours ago", ago / 3600),
                        _ => format!("{} days ago", ago / 86400),
                    };
                    widgets::status_line(ui, p.good, format!("yt-dlp {} is installed (last checked for updates {ago}).", t.version));
                }
                None => widgets::status_line(ui, p.faint, "yt-dlp is not installed. It is downloaded the first time you use the YouTube page."),
            }
            match youtube::js_runtime() {
                Some((name, path)) => widgets::hint(ui, format!("JavaScript runtime for YouTube's player checks: {name} ({})", path.display())),
                None => widgets::hint(ui, "No JavaScript runtime (Deno, Node.js or Bun) found. It is not needed for the audio downloads FreeMic makes."),
            }
            ui.add_enabled_ui(busy.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if tool.is_some() {
                        if widgets::button(ui, Kind::Secondary, None, "Check for update").clicked() {
                            self.yt.send(Job::Setup { force: true });
                        }
                        if widgets::button(ui, Kind::Ghost, None, "Remove yt-dlp").clicked() {
                            self.yt.send(Job::Remove);
                        }
                    }
                    if widgets::button(ui, Kind::Secondary, Some(Icon::Folder), "Open downloads folder").clicked() {
                        crate::platform::open_folder(&self.yt_out_dir());
                    }
                });
            });
            if matches!(busy, Some(Busy::Setup)) {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0).color(p.accent));
                    ui.label(RichText::new("Working…").color(p.dim));
                });
            }
        });
    }
}

/// One search result. Returns `Some(true)` when "Add" was clicked.
#[allow(clippy::too_many_arguments)]
fn result_row(ui: &mut egui::Ui, p: &Palette, v: &Video, best: bool, primary: bool, on_board: bool, downloading: Option<&str>, progress: Option<f32>, busy: bool) -> Option<bool> {
    let mut clicked = None;
    let this_downloading = downloading == Some(v.id.as_str());
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, if best { theme::mix(p.accent, p.border, 0.5) } else { p.border }))
        .corner_radius(12)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let full = ui.available_width();
            let label = if on_board { "On your soundboard" } else { "Add to soundboard" };
            let btn_w = 13.0 * 2.0 + 16.0 + 7.0 + widgets::text_width(ui, label, theme::font_bold(13.5));
            let side = full >= 460.0;
            let text_w = if side { full - btn_w - 14.0 } else { full };

            let mut meta = Vec::new();
            if best {
                meta.push("Best match".to_string());
            }
            if !v.channel.is_empty() {
                meta.push(v.channel.clone());
            }
            if let Some(d) = v.duration {
                meta.push(fmt_time(d));
            }
            let title = widgets::rows(ui, &v.title, theme::font_bold(14.0), p.text, text_w, 2);
            let meta_g = widgets::one_line(ui, &meta.join(" · "), theme::font(12.5), if best { p.accent } else { p.dim }, text_w);
            let text_h = title.size().y + 4.0 + meta_g.size().y;
            let row_h = if side { text_h.max(32.0) } else { text_h + 10.0 + 32.0 };
            let (rect, _) = ui.allocate_exact_size(vec2(full, row_h), Sense::hover());
            let ty = if side { rect.center().y - text_h / 2.0 } else { rect.top() };
            ui.painter().galley(pos2(rect.left(), ty), title.clone(), p.text);
            ui.painter().galley(pos2(rect.left(), ty + title.size().y + 4.0), meta_g, p.dim);

            let btn_rect = if side {
                Rect::from_min_size(pos2(rect.right() - btn_w, rect.center().y - 16.0), vec2(btn_w, 32.0))
            } else {
                Rect::from_min_size(pos2(rect.left(), rect.bottom() - 32.0), vec2(btn_w.min(full), 32.0))
            };
            ui.scope_builder(UiBuilder::new().max_rect(btn_rect).layout(Layout::left_to_right(Align::Center)), |ui| {
                if this_downloading {
                    let (r, _) = ui.allocate_exact_size(btn_rect.size(), Sense::hover());
                    let bar = Rect::from_center_size(r.center(), vec2(r.width(), 6.0));
                    ui.painter().rect_filled(bar, 3.0, p.raised);
                    let f = progress.unwrap_or(0.0).clamp(0.0, 1.0);
                    ui.painter().rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * f, bar.height())), 3.0, p.accent);
                } else if on_board {
                    ui.add_enabled_ui(false, |ui| widgets::button_sized(ui, Kind::Secondary, Some(Icon::Check), label, Some(btn_w)));
                } else {
                    let kind = if primary { Kind::Primary } else { Kind::Secondary };
                    let r = ui.add_enabled_ui(!busy, |ui| widgets::button_sized(ui, kind, Some(Icon::Plus), label, Some(btn_w))).inner;
                    if r.clicked() {
                        clicked = Some(true);
                    }
                }
            });
        });
    clicked
}
