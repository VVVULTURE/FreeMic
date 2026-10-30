//! Command channel to a running FreeMic: `freemic --play "Name"` / `--stop` / `--quit`.
//! Linux uses a Unix socket in $XDG_RUNTIME_DIR, Windows a per-user named pipe.
//! Lets Wayland users bind soundboard keys in their compositor (where apps
//! cannot grab global hotkeys) and lets Stream Deck/AutoHotkey users script it.

#[cfg(unix)]
mod imp {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn socket_path() -> PathBuf {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir());
        let user = std::env::var("USER").unwrap_or_default();
        dir.join(format!("freemic-{user}.sock"))
    }

    /// Bind the socket; `None` if another instance is listening.
    pub fn bind() -> Option<UnixListener> {
        let p = socket_path();
        if UnixStream::connect(&p).is_ok() {
            return None;
        }
        let _ = std::fs::remove_file(&p); // stale socket from a crash
        match UnixListener::bind(&p) {
            Ok(l) => Some(l),
            Err(e) => {
                log::warn!("could not create {}: {e}", p.display());
                // Fall back to "no single-instance protection" rather than refusing to start.
                UnixListener::bind(std::env::temp_dir().join(format!("freemic-{}.sock", std::process::id()))).ok()
            }
        }
    }

    pub fn serve(listener: UnixListener, handler: Arc<dyn Fn(&str) -> String + Send + Sync>) {
        std::thread::Builder::new()
            .name("freemic-ipc".into())
            .spawn(move || {
                for conn in listener.incoming() {
                    let Ok(mut conn) = conn else { continue };
                    let mut line = String::new();
                    if BufReader::new(&conn).read_line(&mut line).is_ok() {
                        let reply = handler(line.trim());
                        let _ = conn.write_all(reply.as_bytes());
                    }
                }
            })
            .ok();
    }

    pub fn send(cmd: &str) -> Result<String, String> {
        let mut s = UnixStream::connect(socket_path()).map_err(|_| "FreeMic is not running".to_string())?;
        s.write_all(format!("{cmd}\n").as_bytes()).map_err(|e| e.to_string())?;
        let _ = s.shutdown(std::net::Shutdown::Write);
        let mut reply = String::new();
        let _ = BufReader::new(s).read_line(&mut reply);
        Ok(reply)
    }

    pub fn cleanup() {
        let _ = std::fs::remove_file(socket_path());
    }
}

#[cfg(unix)]
pub use imp::*;

#[cfg(windows)]
mod imp {
    use std::io::{Read, Write};
    use std::sync::Arc;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile, FlushFileBuffers, PIPE_ACCESS_DUPLEX};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    fn pipe_name() -> String {
        let user = std::env::var("USERNAME").unwrap_or_default();
        format!(r"\\.\pipe\FreeMic-{user}")
    }

    /// Serve commands on a per-user named pipe (local clients only).
    pub fn serve(handler: Arc<dyn Fn(&str) -> String + Send + Sync>) {
        std::thread::Builder::new()
            .name("freemic-ipc".into())
            .spawn(move || {
                let name: Vec<u16> = pipe_name().encode_utf16().chain(std::iter::once(0)).collect();
                loop {
                    let h: HANDLE = unsafe {
                        CreateNamedPipeW(
                            PCWSTR(name.as_ptr()),
                            PIPE_ACCESS_DUPLEX,
                            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                            PIPE_UNLIMITED_INSTANCES,
                            512,
                            512,
                            0,
                            None,
                        )
                    };
                    if h == INVALID_HANDLE_VALUE {
                        log::warn!("could not create the command pipe");
                        return;
                    }
                    // Blocks until a client connects (no polling).
                    let _ = unsafe { ConnectNamedPipe(h, None) };
                    let mut buf = [0u8; 512];
                    let mut n = 0u32;
                    if unsafe { ReadFile(h, Some(&mut buf), Some(&mut n), None) }.is_ok() {
                        let cmd = String::from_utf8_lossy(&buf[..n as usize]).trim().to_string();
                        let reply = handler(&cmd);
                        let mut w = 0u32;
                        let _ = unsafe { WriteFile(h, Some(reply.as_bytes()), Some(&mut w), None) };
                        let _ = unsafe { FlushFileBuffers(h) };
                    }
                    let _ = unsafe { DisconnectNamedPipe(h) };
                    let _ = unsafe { CloseHandle(h) };
                }
            })
            .ok();
    }

    pub fn send(cmd: &str) -> Result<String, String> {
        // The server re-creates the pipe after each client; retry briefly if we hit that gap.
        let mut tries = 0;
        let mut f = loop {
            match std::fs::OpenOptions::new().read(true).write(true).open(pipe_name()) {
                Ok(f) => break f,
                Err(_) if tries < 50 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(_) => return Err("FreeMic is not running".into()),
            }
        };
        f.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;
        let mut reply = String::new();
        let _ = f.read_to_string(&mut reply);
        Ok(reply)
    }

    pub fn cleanup() {}
}

#[cfg(windows)]
pub use imp::*;
