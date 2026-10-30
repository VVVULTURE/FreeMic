//! Small OS integrations: start with the system, single instance, opening links/folders.

use std::path::Path;

pub fn open_url(url: &str) {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = r {
        log::warn!("could not open {url}: {e}");
    }
}

pub fn open_folder(p: &Path) {
    let _ = std::fs::create_dir_all(p);
    open_url(&p.to_string_lossy());
}

/// Held for the lifetime of the process; a second instance cannot acquire it.
pub struct InstanceGuard {
    #[cfg(windows)]
    _handle: windows::Win32::Foundation::HANDLE,
    #[cfg(not(windows))]
    listener: Option<std::os::unix::net::UnixListener>,
}

impl InstanceGuard {
    /// The command socket (Linux), to be served by `ipc::serve`.
    #[cfg(not(windows))]
    pub fn take_listener(&mut self) -> Option<std::os::unix::net::UnixListener> {
        self.listener.take()
    }
}

/// Returns `None` if another FreeMic instance is already running.
pub fn single_instance(tag: &str) -> Option<InstanceGuard> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        let name: Vec<u16> = format!("Local\\FreeMic-{tag}\0").encode_utf16().collect();
        let h = unsafe { CreateMutexW(None, true, windows::core::PCWSTR(name.as_ptr())) }.ok()?;
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return None;
        }
        Some(InstanceGuard { _handle: h })
    }
    #[cfg(not(windows))]
    {
        let _ = tag;
        let listener = crate::ipc::bind()?;
        Some(InstanceGuard { listener: Some(listener) })
    }
}

#[cfg(windows)]
mod autostart_impl {
    use windows::Win32::System::Registry::*;
    use windows::core::PCWSTR;

    const RUN: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const NAME: &str = "FreeMic";

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn set(enabled: bool, command: &str) -> Result<(), String> {
        unsafe {
            let mut key = HKEY::default();
            let r = RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(w(RUN).as_ptr()), Some(0), KEY_SET_VALUE | KEY_QUERY_VALUE, &mut key);
            if r.is_err() {
                return Err(format!("could not open the Run key ({r:?})"));
            }
            let name = w(NAME);
            let r = if enabled {
                let data = w(command);
                let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
                RegSetValueExW(key, PCWSTR(name.as_ptr()), Some(0), REG_SZ, Some(bytes))
            } else {
                let r = RegDeleteValueW(key, PCWSTR(name.as_ptr()));
                if r == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND { windows::Win32::Foundation::ERROR_SUCCESS } else { r }
            };
            let _ = RegCloseKey(key);
            if r.is_err() { Err(format!("registry update failed ({r:?})")) } else { Ok(()) }
        }
    }
}

/// Enable or disable launching FreeMic (minimized) when the user signs in.
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        autostart_impl::set(enabled, &format!("\"{}\" --minimized", exe.display()))
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME").ok_or("HOME not set")?;
        let dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| Path::new(&home).join(".config"))
            .join("autostart");
        let file = dir.join("freemic.desktop");
        if enabled {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let body = format!(
                "[Desktop Entry]\nType=Application\nName=FreeMic\nComment=Soundboard and virtual microphone\nExec=\"{}\" --minimized\nIcon=freemic\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
                exe.display()
            );
            std::fs::write(&file, body).map_err(|e| e.to_string())
        } else {
            match std::fs::remove_file(&file) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            }
        }
    }
}
