//! Where FreeMic keeps its data.
//!
//! * Portable mode (a `portable.txt` file or `data` folder next to the
//!   executable): everything lives in `<exe dir>/data`.
//! * Installed: `%APPDATA%\FreeMic` on Windows,
//!   `$XDG_CONFIG_HOME/freemic` (settings) and `$XDG_DATA_HOME/freemic` (sounds) on Linux.
//! * `FREEMIC_DATA_DIR` overrides both (used by tests).

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub portable: bool,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn detect() -> Paths {
        if let Some(dir) = std::env::var_os("FREEMIC_DATA_DIR") {
            let d = PathBuf::from(dir);
            return Paths { portable: true, config_dir: d.clone(), data_dir: d };
        }
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            if exe_dir.join("portable.txt").exists() || exe_dir.join("data").is_dir() {
                let d = exe_dir.join("data");
                return Paths { portable: true, config_dir: d.clone(), data_dir: d };
            }
        }
        #[cfg(windows)]
        {
            let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
            let d = base.join("FreeMic");
            Paths { portable: false, config_dir: d.clone(), data_dir: d }
        }
        #[cfg(not(windows))]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
            let xdg = |var: &str, fallback: &str| {
                std::env::var_os(var)
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .unwrap_or_else(|| home.join(fallback))
            };
            Paths {
                portable: false,
                config_dir: xdg("XDG_CONFIG_HOME", ".config").join("freemic"),
                data_dir: xdg("XDG_DATA_HOME", ".local/share").join("freemic"),
            }
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }
    pub fn board_file(&self) -> PathBuf {
        self.config_dir.join("soundboard.json")
    }
    pub fn sounds_dir(&self) -> PathBuf {
        self.data_dir.join("sounds")
    }
    pub fn log_file(&self) -> PathBuf {
        self.data_dir.join("freemic.log")
    }

    /// Resolve a stored sound path (relative paths are inside the data directory).
    pub fn resolve(&self, p: &Path) -> PathBuf {
        if p.is_absolute() { p.to_path_buf() } else { self.data_dir.join(p) }
    }

    /// Store paths inside the data directory relative to it, so portable folders can move.
    pub fn store(&self, p: &Path) -> PathBuf {
        p.strip_prefix(&self.data_dir).map(Path::to_path_buf).unwrap_or_else(|_| p.to_path_buf())
    }
}
