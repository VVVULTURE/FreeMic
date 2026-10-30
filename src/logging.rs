//! Tiny file logger: `freemic.log` in the data directory (previous run kept as `freemic.old.log`).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct FileLogger {
    file: Mutex<Option<File>>,
    level: log::LevelFilter,
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= self.level && (m.target().starts_with("freemic") || m.level() <= log::Level::Warn)
    }
    fn log(&self, r: &log::Record<'_>) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let line = format!("{}.{:03} {:5} {}: {}\n", t.as_secs(), t.subsec_millis(), r.level(), r.target(), r.args());
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        if let Ok(mut g) = self.file.lock() {
            if let Some(f) = g.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }
    fn flush(&self) {
        if let Ok(mut g) = self.file.lock() {
            if let Some(f) = g.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

pub fn init(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::rename(path, path.with_file_name("freemic.old.log"));
    let file = OpenOptions::new().create(true).write(true).truncate(true).open(path).ok();
    let level = if std::env::var_os("FREEMIC_DEBUG").is_some() { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    let logger = Box::leak(Box::new(FileLogger { file: Mutex::new(file), level }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}
