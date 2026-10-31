#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use eframe::egui;
use freemic::audio::backend::BackendKind;
use freemic::audio::engine::Engine;
use freemic::audio::Shared;
use freemic::config::{self, Board, Settings};
use freemic::paths::Paths;
use freemic::ui::{FreeMicApp, StartOptions, Targets};
use freemic::{icon, ipc, logging, platform};

const HELP: &str = "FreeMic - soundboard and virtual microphone

Usage: freemic [options]
  --minimized        start minimized
  --play <name|#n>   play (or stop, if playing) a sound in the running FreeMic,
                     by name or by button position (#1 is the first button)
  --stop             stop the soundboard in the running FreeMic
  --quit             close the running FreeMic
  --null-audio       run without touching any audio device (testing)
  --version          print the version
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let cli = ["--help", "-h", "--version", "--play", "--stop", "--quit"].iter().any(|f| has(f));
    if cli {
        attach_parent_console();
    }
    if has("--help") || has("-h") {
        out(HELP);
        return;
    }
    if has("--version") {
        out(&format!("FreeMic {}\n", env!("CARGO_PKG_VERSION")));
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--play") {
        let name = args.get(i + 1).cloned().unwrap_or_default();
        std::process::exit(remote(&format!("play {name}")));
    }
    if has("--stop") {
        std::process::exit(remote("stop"));
    }
    if has("--quit") {
        std::process::exit(remote("quit"));
    }

    let paths = Paths::detect();
    logging::init(&paths.log_file());
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("crash: {info}");
        log::logger().flush();
        // Do not leave the virtual microphone behind (Linux) if the UI crashes.
        freemic::audio::backend::emergency_cleanup();
        default_hook(info);
    }));
    log::info!("FreeMic {} starting; data dir {} (portable: {})", env!("CARGO_PKG_VERSION"), paths.data_dir.display(), paths.portable);

    let backend = if has("--null-audio") || std::env::var("FREEMIC_BACKEND").as_deref() == Ok("null") {
        BackendKind::Null
    } else {
        BackendKind::Native
    };
    let tag = if backend == BackendKind::Null { "null" } else { "main" };
    let Some(mut instance) = platform::single_instance(tag) else {
        log::info!("another instance is running");
        rfd::MessageDialog::new()
            .set_title("FreeMic")
            .set_description("FreeMic is already running. Look for its window in the taskbar.")
            .set_level(rfd::MessageLevel::Info)
            .show();
        return;
    };

    let mut warnings = Vec::new();
    let (mut settings, w): (Settings, _) = config::load_json(&paths.settings_file());
    warnings.extend(w);
    settings.sanitize();
    let (board, w): (Board, _) = config::load_json(&paths.board_file());
    warnings.extend(w);

    let shared = Arc::new(Shared::default());
    FreeMicApp::apply_gains(&settings, &shared);
    let ctx_slot: Arc<Mutex<Option<egui::Context>>> = Arc::default();
    let notify = {
        let slot = ctx_slot.clone();
        Arc::new(move || {
            if let Some(c) = slot.lock().unwrap().as_ref() {
                c.request_repaint();
            }
        })
    };
    let engine = Arc::new(Engine::start(backend, FreeMicApp::engine_config(&settings), shared, notify));
    let targets: Targets = Arc::default();

    let handler = {
        let (e, t, slot) = (engine.clone(), targets.clone(), ctx_slot.clone());
        Arc::new(move |cmd: &str| -> String {
                let ctx = slot.lock().unwrap().clone();
                if cmd == "quit" {
                    if let Some(c) = ctx {
                        c.send_viewport_cmd(egui::ViewportCommand::Close);
                        c.request_repaint();
                    }
                    return "ok
".into();
                }
                let reply = command(&e, &t, cmd);
                if let Some(c) = ctx {
                    c.request_repaint();
                }
                reply
        })
    };
    #[cfg(unix)]
    if let Some(listener) = instance.take_listener() {
        ipc::serve(listener, handler);
    }
    #[cfg(windows)]
    ipc::serve(handler);
    let _ = &mut instance;

    let size = settings.window_size.unwrap_or([820.0, 560.0]);
    let minimized = has("--minimized") || settings.start_minimized;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("FreeMic")
        .with_app_id("freemic")
        .with_inner_size(size)
        .with_min_inner_size([480.0, 400.0])
        .with_icon(Arc::new(icon::window_icon()))
        .with_drag_and_drop(true);
    if std::env::var_os("FREEMIC_TEST_WINDOW").is_some() {
        // Automated UI checks: keep the window off-screen and never steal focus.
        viewport = viewport.with_position([-4000.0, 100.0]).with_active(false);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let eng = engine.clone();
    let result = eframe::run_native(
        "FreeMic",
        options,
        Box::new(move |cc| {
            *ctx_slot.lock().unwrap() = Some(cc.egui_ctx.clone());
            let opts = StartOptions { start_minimized: minimized, load_warnings: warnings, global_hotkeys: true };
            Ok(Box::new(FreeMicApp::new(&cc.egui_ctx, paths, settings, board, eng, targets, opts)))
        }),
    );
    if let Err(e) = result {
        log::error!("window error: {e}");
        rfd::MessageDialog::new()
            .set_title("FreeMic")
            .set_description(format!("FreeMic could not open its window: {e}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
    engine.shutdown();
    ipc::cleanup();
    log::info!("FreeMic exited");
}

/// Handle a command from `freemic --play/--stop`.
fn command(engine: &Engine, targets: &Targets, cmd: &str) -> String {
    use freemic::hotkeys::Action;
    if cmd == "stop" {
        engine.stop_sound();
        return "ok\n".into();
    }
    if let Some(arg) = cmd.strip_prefix("play ") {
        let arg = arg.trim();
        let found = freemic::ui::find_target(&targets.lock().unwrap(), arg);
        return match found {
            Some(id) => {
                freemic::ui::trigger(engine, targets, Action::Play(id));
                "ok\n".into()
            }
            None => format!("no sound named \"{arg}\"\n"),
        };
    }
    "unknown command\n".into()
}

fn remote(cmd: &str) -> i32 {
    match ipc::send(cmd) {
        Ok(reply) if reply.starts_with("ok") => 0,
        Ok(reply) => {
            err(&reply);
            1
        }
        Err(e) => {
            err(&format!("{e}\n"));
            1
        }
    }
}

// Command-line output must never panic (e.g. when the reader closed the pipe).
fn out(s: &str) {
    use std::io::Write;
    let _ = std::io::stdout().write_all(s.as_bytes());
    let _ = std::io::stdout().flush();
}

fn err(s: &str) {
    use std::io::Write;
    let _ = std::io::stderr().write_all(s.as_bytes());
}

/// FreeMic.exe is a GUI program on Windows, so a console that starts it with
/// command-line options would show nothing. Attach to that console instead,
/// unless the output is already redirected to a file or pipe.
#[cfg(windows)]
fn attach_parent_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE};
    let redirected = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.is_ok_and(|h| !h.is_invalid() && !h.0.is_null());
    if !redirected {
        let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}
