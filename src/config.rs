//! Persistent settings and soundboard layout (JSON, written atomically).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlayMode {
    #[default]
    Once,
    Loop,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Sound {
    pub id: u64,
    pub name: String,
    /// Relative to the data directory when the file lives in the library, else absolute.
    pub file: PathBuf,
    #[serde(default)]
    pub mode: PlayMode,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub hotkey: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Board {
    pub version: u32,
    pub sounds: Vec<Sound>,
    pub next_id: u64,
}

impl Default for Board {
    fn default() -> Self {
        Self { version: 1, sounds: Vec::new(), next_id: 1 }
    }
}

impl Board {
    pub fn add(&mut self, name: String, file: PathBuf, mode: PlayMode) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.sounds.push(Sound { id, name, file, mode, color: None, hotkey: None, duration: None });
        id
    }
    pub fn get(&self, id: u64) -> Option<&Sound> {
        self.sounds.iter().find(|s| s.id == id)
    }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Sound> {
        self.sounds.iter_mut().find(|s| s.id == id)
    }
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.sounds.iter().position(|s| s.id == id)
    }
    pub fn move_to(&mut self, from: usize, to: usize) {
        if from < self.sounds.len() && to < self.sounds.len() && from != to {
            let s = self.sounds.remove(from);
            self.sounds.insert(to, s);
        }
    }
    pub fn remove(&mut self, id: u64) -> Option<Sound> {
        let i = self.index_of(id)?;
        Some(self.sounds.remove(i))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DeviceRef {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// Slider positions in percent (gain = (pct/100)^2).
    pub sound_volume: f32,
    pub mic_volume: f32,
    pub master_volume: f32,
    pub monitor_volume: f32,
    pub mic_enabled: bool,
    pub mic_muted: bool,
    pub mic_device: Option<DeviceRef>,
    pub output_device: Option<DeviceRef>,
    pub monitor_device: Option<DeviceRef>,
    pub theme: Theme,
    pub start_with_system: bool,
    pub start_minimized: bool,
    pub copy_imports: bool,
    pub default_mode: PlayMode,
    pub button_width: f32,
    pub stop_hotkey: Option<String>,
    pub window_size: Option<[f32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            sound_volume: 70.0,
            mic_volume: 100.0,
            master_volume: 100.0,
            monitor_volume: 60.0,
            mic_enabled: true,
            mic_muted: false,
            mic_device: None,
            output_device: None,
            monitor_device: None,
            theme: Theme::System,
            start_with_system: false,
            start_minimized: false,
            copy_imports: true,
            default_mode: PlayMode::Once,
            button_width: 150.0,
            stop_hotkey: None,
            window_size: None,
        }
    }
}

impl Settings {
    /// Clamp values loaded from disk into valid ranges.
    pub fn sanitize(&mut self) {
        let fix = |v: &mut f32, max: f32, def: f32| {
            if !v.is_finite() {
                *v = def;
            }
            *v = v.clamp(0.0, max);
        };
        fix(&mut self.sound_volume, 100.0, 70.0);
        fix(&mut self.mic_volume, 200.0, 100.0);
        fix(&mut self.master_volume, 100.0, 100.0);
        fix(&mut self.monitor_volume, 100.0, 60.0);
        if !self.button_width.is_finite() {
            self.button_width = 150.0;
        }
        self.button_width = self.button_width.clamp(90.0, 300.0);
    }
}

/// Write `data` to `path` atomically (temp file + rename) so a crash never leaves a half-written file.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    write_atomic(path, &data).map_err(|e| format!("could not save {}: {e}", path.display()))
}

/// Load JSON, falling back to defaults. A damaged file is kept aside (never silently lost).
pub fn load_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> (T, Option<String>) {
    match fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (T::default(), None),
        Err(e) => (T::default(), Some(format!("Could not read {}: {e}", path.display()))),
        // Tolerate a UTF-8 byte order mark (added by some editors and Windows PowerShell 5).
        Ok(bytes) => match serde_json::from_slice(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes)) {
            Ok(v) => (v, None),
            Err(e) => {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let aside = path.with_extension(format!("damaged-{stamp}.json"));
                let _ = fs::rename(path, &aside);
                (
                    T::default(),
                    Some(format!(
                        "{} was damaged ({e}) and has been reset. The old file was kept as {}.",
                        path.display(),
                        aside.display()
                    )),
                )
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_damaged_file_recovery() {
        let dir = std::env::temp_dir().join(format!("freemic-cfg-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        s.sound_volume = 42.0;
        s.mic_device = Some(DeviceRef { id: "{x}".into(), name: "Mic".into() });
        save_json(&path, &s).unwrap();
        let (loaded, warn): (Settings, _) = load_json(&path);
        assert!(warn.is_none());
        assert_eq!(loaded, s);

        fs::write(&path, b"{ not json").unwrap();
        let (loaded, warn): (Settings, _) = load_json(&path);
        assert_eq!(loaded, Settings::default());
        assert!(warn.unwrap().contains("damaged"));
        assert!(!path.exists());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "damaged copy kept");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn utf8_bom_is_accepted() {
        let dir = std::env::temp_dir().join(format!("freemic-bom-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("settings.json");
        fs::write(&path, [&[0xEFu8, 0xBB, 0xBF][..], br#"{"sound_volume": 33}"#].concat()).unwrap();
        let (s, warn): (Settings, _) = load_json(&path);
        assert!(warn.is_none(), "{warn:?}");
        assert_eq!(s.sound_volume, 33.0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let s: Settings = serde_json::from_str(r#"{"sound_volume": 500, "future_option": true}"#).unwrap();
        let mut s = s;
        s.sanitize();
        assert_eq!(s.sound_volume, 100.0);
        assert_eq!(s.mic_volume, 100.0);
    }

    #[test]
    fn board_reorder() {
        let mut b = Board::default();
        let a = b.add("a".into(), "a.wav".into(), PlayMode::Once);
        let _b2 = b.add("b".into(), "b.wav".into(), PlayMode::Loop);
        let c = b.add("c".into(), "c.wav".into(), PlayMode::Once);
        b.move_to(2, 0);
        assert_eq!(b.sounds[0].id, c);
        assert_eq!(b.sounds[1].id, a);
        b.remove(a);
        assert_eq!(b.sounds.len(), 2);
    }
}
