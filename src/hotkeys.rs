//! System-wide hotkeys (work while a game has focus).
//!
//! Uses RegisterHotKey on Windows and XGrabKey on X11. Wayland compositors do
//! not let applications grab keys globally; there the `freemic --play` command
//! can be bound in the compositor instead (see docs).

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Play(u64),
    Stop,
}

pub struct Hotkeys {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    map: Arc<Mutex<HashMap<u32, Action>>>,
    pub unavailable: Option<String>,
}

impl Hotkeys {
    pub fn new(on_action: Arc<dyn Fn(Action) + Send + Sync>) -> Self {
        let map: Arc<Mutex<HashMap<u32, Action>>> = Arc::default();
        let disabled = std::env::var_os("FREEMIC_NO_HOTKEYS").is_some();
        let (manager, unavailable) = if disabled {
            (None, Some("Global hotkeys are disabled (FREEMIC_NO_HOTKEYS).".to_string()))
        } else if wayland_session() {
            // Through XWayland the X11 grab would only fire while an X11 window has focus.
            (None, Some("Global hotkeys are not available in a Wayland session.".to_string()))
        } else {
            match GlobalHotKeyManager::new() {
                Ok(m) => (Some(m), None),
                Err(e) => (None, Some(format!("Global hotkeys are not available on this desktop ({e})."))),
            }
        };
        if let Some(u) = &unavailable {
            log::info!("{u}");
        }
        let m2 = map.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |ev: GlobalHotKeyEvent| {
            if ev.state() == HotKeyState::Pressed {
                let action = m2.lock().ok().and_then(|m| m.get(&ev.id()).copied());
                if let Some(a) = action {
                    on_action(a);
                }
            }
        }));
        Hotkeys { manager, registered: Vec::new(), map, unavailable }
    }

    /// Register exactly the given bindings. Returns messages for keys that could not be registered.
    pub fn sync(&mut self, bindings: &[(String, Action)]) -> Vec<String> {
        let Some(mgr) = &self.manager else { return Vec::new() };
        let _ = mgr.unregister_all(&self.registered);
        self.registered.clear();
        let mut map = HashMap::new();
        let mut problems = Vec::new();
        for (text, action) in bindings {
            let Ok(hk) = HotKey::from_str(text) else {
                problems.push(format!("Invalid hotkey \"{text}\""));
                continue;
            };
            if map.contains_key(&hk.id()) {
                problems.push(format!("{} is assigned twice", pretty(text)));
                continue;
            }
            match mgr.register(hk) {
                Ok(()) => {
                    self.registered.push(hk);
                    map.insert(hk.id(), *action);
                }
                Err(e) => problems.push(format!("{} could not be registered (probably used by another program): {e}", pretty(text))),
            }
        }
        *self.map.lock().unwrap() = map;
        problems
    }
}

/// Convert a key press captured by egui into a hotkey string.
pub fn from_egui(key: egui::Key, m: egui::Modifiers) -> Option<String> {
    use egui::Key as K;
    let code = match key {
        K::A => Code::KeyA, K::B => Code::KeyB, K::C => Code::KeyC, K::D => Code::KeyD, K::E => Code::KeyE,
        K::F => Code::KeyF, K::G => Code::KeyG, K::H => Code::KeyH, K::I => Code::KeyI, K::J => Code::KeyJ,
        K::K => Code::KeyK, K::L => Code::KeyL, K::M => Code::KeyM, K::N => Code::KeyN, K::O => Code::KeyO,
        K::P => Code::KeyP, K::Q => Code::KeyQ, K::R => Code::KeyR, K::S => Code::KeyS, K::T => Code::KeyT,
        K::U => Code::KeyU, K::V => Code::KeyV, K::W => Code::KeyW, K::X => Code::KeyX, K::Y => Code::KeyY,
        K::Z => Code::KeyZ,
        K::Num0 => Code::Digit0, K::Num1 => Code::Digit1, K::Num2 => Code::Digit2, K::Num3 => Code::Digit3,
        K::Num4 => Code::Digit4, K::Num5 => Code::Digit5, K::Num6 => Code::Digit6, K::Num7 => Code::Digit7,
        K::Num8 => Code::Digit8, K::Num9 => Code::Digit9,
        K::F1 => Code::F1, K::F2 => Code::F2, K::F3 => Code::F3, K::F4 => Code::F4, K::F5 => Code::F5,
        K::F6 => Code::F6, K::F7 => Code::F7, K::F8 => Code::F8, K::F9 => Code::F9, K::F10 => Code::F10,
        K::F11 => Code::F11, K::F12 => Code::F12, K::F13 => Code::F13, K::F14 => Code::F14, K::F15 => Code::F15,
        K::F16 => Code::F16, K::F17 => Code::F17, K::F18 => Code::F18, K::F19 => Code::F19, K::F20 => Code::F20,
        K::Space => Code::Space, K::Insert => Code::Insert, K::Delete => Code::Delete, K::Home => Code::Home,
        K::End => Code::End, K::PageUp => Code::PageUp, K::PageDown => Code::PageDown,
        K::ArrowUp => Code::ArrowUp, K::ArrowDown => Code::ArrowDown, K::ArrowLeft => Code::ArrowLeft,
        K::ArrowRight => Code::ArrowRight, K::Minus => Code::Minus, K::Equals => Code::Equal,
        K::Comma => Code::Comma, K::Period => Code::Period, K::Slash => Code::Slash, K::Semicolon => Code::Semicolon,
        K::Backtick => Code::Backquote, K::OpenBracket => Code::BracketLeft, K::CloseBracket => Code::BracketRight,
        K::Backslash => Code::Backslash, K::Quote => Code::Quote,
        _ => return None,
    };
    let mut mods = Modifiers::empty();
    if m.ctrl { mods |= Modifiers::CONTROL; }
    if m.shift { mods |= Modifiers::SHIFT; }
    if m.alt { mods |= Modifiers::ALT; }
    // A bare letter/digit would fire while typing in chat; require a modifier for those.
    let is_function = matches!(code, Code::F1 | Code::F2 | Code::F3 | Code::F4 | Code::F5 | Code::F6 | Code::F7 | Code::F8 | Code::F9 | Code::F10 | Code::F11 | Code::F12 | Code::F13 | Code::F14 | Code::F15 | Code::F16 | Code::F17 | Code::F18 | Code::F19 | Code::F20 | Code::Insert | Code::Home | Code::End | Code::PageUp | Code::PageDown);
    if mods.is_empty() && !is_function {
        return None;
    }
    Some(HotKey::new(if mods.is_empty() { None } else { Some(mods) }, code).into_string())
}

/// True in a Linux Wayland session, where applications cannot grab keys globally.
fn wayland_session() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland"))
}

/// Human friendly form: "control+shift+KeyA" -> "Ctrl+Shift+A".
pub fn pretty(s: &str) -> String {
    s.split('+')
        .map(|p| match p.to_ascii_lowercase().as_str() {
            "control" | "ctrl" => "Ctrl".to_string(),
            "shift" => "Shift".into(),
            "alt" => "Alt".into(),
            "meta" | "super" => "Super".into(),
            _ => {
                let p = p.strip_prefix("Key").or_else(|| p.strip_prefix("Digit")).unwrap_or(p);
                p.replace("Arrow", "")
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn egui_to_hotkey_roundtrip() {
        let s = from_egui(egui::Key::A, egui::Modifiers { ctrl: true, shift: true, ..Default::default() }).unwrap();
        assert!(HotKey::from_str(&s).is_ok(), "{s}");
        assert_eq!(pretty(&s), "Shift+Ctrl+A");
        assert!(from_egui(egui::Key::A, egui::Modifiers::NONE).is_none(), "bare letters rejected");
        let f = from_egui(egui::Key::F9, egui::Modifiers::NONE).unwrap();
        assert!(HotKey::from_str(&f).is_ok());
        assert_eq!(pretty(&f), "F9");
    }
}
