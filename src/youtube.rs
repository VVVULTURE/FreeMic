//! Getting sounds from YouTube, ported from VVVULTURE/YT-Downloader.
//!
//! * yt-dlp (public domain, <https://github.com/yt-dlp/yt-dlp>) does the
//!   downloading. FreeMic fetches the official standalone build from yt-dlp's
//!   GitHub releases on first use, checks it against the release's published
//!   SHA-256 sums, and looks for a newer release at most every 6 hours
//!   (only when YouTube features are used).
//! * Only the audio of **one** video is ever downloaded. Links are reduced to
//!   the video id and passed to yt-dlp as `https://www.youtube.com/watch?v=<id>`
//!   with `--no-playlist --max-downloads 1`; playlist-only links are refused.
//!   (`--no-playlist` alone does not stop a `/playlist?list=` link.)
//! * The audio is saved as AAC in an .m4a file, which FreeMic plays directly,
//!   so no ffmpeg is needed.
//! * Music search, ranking and the player-client fallbacks follow YT-Downloader.

use std::io::{BufRead, BufReader, Read};
#[cfg(not(windows))]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------ links

/// What a pasted link points at.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Link {
    /// A single video. `in_list` is true when the link also named a playlist
    /// or mix (only the video will be downloaded).
    Video { id: String, in_list: bool },
    /// A playlist, mix, channel or other page that is not one video.
    NotAVideo,
    /// Not a YouTube link.
    NotYouTube,
}

fn valid_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Parse a pasted YouTube link. Returns `None` when `text` is not a link at all
/// (then it is treated as a search).
pub fn parse_link(text: &str) -> Option<Link> {
    let t = text.trim();
    if t.contains(char::is_whitespace) {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    let has_scheme = lower.starts_with("https://") || lower.starts_with("http://");
    let rest = if has_scheme { &t[t.find("://").unwrap_or(0) + 3..] } else { t };
    let (host, path_query) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = host.to_ascii_lowercase();
    let host = host.split('@').next_back().unwrap_or("").split(':').next().unwrap_or("");
    let looks_like_url = has_scheme || host.contains('.');
    if !looks_like_url {
        return None;
    }
    let yt = matches!(
        host,
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be" | "www.youtu.be" | "youtube-nocookie.com" | "www.youtube-nocookie.com"
    );
    if !yt {
        return Some(Link::NotYouTube);
    }
    let path_query = path_query.split('#').next().unwrap_or("");
    let (path, query) = match path_query.find('?') {
        Some(i) => (&path_query[..i], &path_query[i + 1..]),
        None => (path_query, ""),
    };
    let param = |name: &str| query.split('&').find_map(|kv| kv.strip_prefix(name).and_then(|v| v.strip_prefix('=')));
    let in_list = param("list").is_some_and(|v| !v.is_empty());
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let id = if host.ends_with("youtu.be") {
        segs.first().copied()
    } else {
        match segs.as_slice() {
            ["watch"] => param("v"),
            ["shorts" | "embed" | "live" | "v" | "e", id, ..] => Some(*id),
            _ => None,
        }
    };
    Some(match id {
        Some(id) if valid_id(id) => Link::Video { id: id.to_string(), in_list },
        _ => Link::NotAVideo,
    })
}

pub fn watch_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

// ------------------------------------------------------------------ search + ranking

/// The search YT-Downloader runs for its Artist / Song fields.
pub fn music_query(artist: &str, song: &str) -> Option<String> {
    let (a, s) = (artist.trim(), song.trim());
    match (a.is_empty(), s.is_empty()) {
        (false, false) => Some(format!("{a} {s}")),
        (false, true) => Some(format!("'{a}' official music video")),
        (true, false) => Some(format!("'{s}' song official")),
        (true, true) => None,
    }
}

const MUSIC_BLACKLIST: &[&str] = &["live", "concert", "performance", "stage", "acoustic", "cover", "karaoke"];

/// YT-Downloader's `score_music`: title similarity to the song, artist in the
/// channel name, VEVO, and penalties for live/cover/karaoke versions.
pub fn score_music(title: &str, channel: &str, artist: &str, song: &str) -> i32 {
    let title = title.to_lowercase();
    let channel = channel.to_lowercase();
    let mut score = 0;
    let sim = ratio(song, &title);
    if sim > 0.0 {
        score += (sim * 40.0) as i32;
    }
    if channel.contains(&artist.to_lowercase()) {
        score += 25;
    }
    if channel.contains("vevo") {
        score += 10;
    }
    let words = title.split_whitespace().count();
    for b in MUSIC_BLACKLIST {
        if title.contains(b) && words > 3 {
            score -= 20;
        }
    }
    score
}

/// Python's `difflib.SequenceMatcher(None, a.lower(), b.lower()).ratio()`.
pub fn ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    2.0 * matching_chars(&a, &b) as f64 / total as f64
}

/// Total size of the matching blocks found by difflib's algorithm (including
/// its "autojunk" rule for sequences of 200+ items).
fn matching_chars(a: &[char], b: &[char]) -> usize {
    use std::collections::HashMap;
    let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
    for (j, c) in b.iter().enumerate() {
        b2j.entry(*c).or_default().push(j);
    }
    let n = b.len();
    if n >= 200 {
        let ntest = n / 100 + 1;
        b2j.retain(|_, v| v.len() <= ntest);
    }
    let longest = |alo: usize, ahi: usize, blo: usize, bhi: usize| -> (usize, usize, usize) {
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0usize);
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for (i, c) in a.iter().enumerate().take(ahi).skip(alo) {
            let mut new: HashMap<usize, usize> = HashMap::new();
            if let Some(js) = b2j.get(c) {
                for &j in js {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    let k = if j > 0 { j2len.get(&(j - 1)).copied().unwrap_or(0) } else { 0 } + 1;
                    new.insert(j, k);
                    if k > bestsize {
                        besti = i + 1 - k;
                        bestj = j + 1 - k;
                        bestsize = k;
                    }
                }
            }
            j2len = new;
        }
        // difflib then extends the match with equal neighbours that were not
        // indexed (the "popular" items dropped by autojunk).
        while besti > alo && bestj > blo && a[besti - 1] == b[bestj - 1] {
            besti -= 1;
            bestj -= 1;
            bestsize += 1;
        }
        while besti + bestsize < ahi && bestj + bestsize < bhi && a[besti + bestsize] == b[bestj + bestsize] {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    };
    let mut total = 0;
    let mut queue = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let (i, j, k) = longest(alo, ahi, blo, bhi);
        if k > 0 {
            total += k;
            if alo < i && blo < j {
                queue.push((alo, i, blo, j));
            }
            if i + k < ahi && j + k < bhi {
                queue.push((i + k, ahi, j + k, bhi));
            }
        }
    }
    total
}

#[derive(Clone, Debug, PartialEq)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub duration: Option<f64>,
    /// YT-Downloader's music score (music searches only).
    pub score: Option<i32>,
}

#[derive(Deserialize)]
struct FlatEntry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    ie_key: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    uploader: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    live_status: Option<String>,
}

#[derive(Deserialize)]
struct FlatResult {
    #[serde(default)]
    entries: Vec<FlatEntry>,
}

/// Videos from yt-dlp's `--flat-playlist --dump-single-json` output of a
/// search. Live streams and premieres are left out (they cannot be saved).
pub fn parse_search(json: &str) -> Result<Vec<Video>, String> {
    let r: FlatResult = serde_json::from_str(json).map_err(|e| format!("unexpected answer from yt-dlp: {e}"))?;
    Ok(r.entries
        .into_iter()
        .filter(|e| e.ie_key.as_deref().is_none_or(|k| k == "Youtube"))
        .filter(|e| !matches!(e.live_status.as_deref(), Some("is_live" | "is_upcoming")))
        .filter_map(|e| {
            let id = e.id.filter(|i| valid_id(i))?;
            Some(Video {
                id,
                title: e.title.unwrap_or_else(|| "(untitled)".into()),
                channel: e.channel.or(e.uploader).unwrap_or_default(),
                duration: e.duration,
                score: None,
            })
        })
        .collect())
}

/// Sort a music search like YT-Downloader: highest score first; equal scores
/// keep YouTube's order (Python's `max` also returns the first of equals).
pub fn rank_music(videos: &mut [Video], artist: &str, song: &str) {
    for v in videos.iter_mut() {
        v.score = Some(score_music(&v.title, &v.channel, artist, song));
    }
    videos.sort_by_key(|v| std::cmp::Reverse(v.score.unwrap_or(i32::MIN)));
}

// ------------------------------------------------------------------ errors

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// Same video fails the same way on every client: stop trying others.
fn is_terminal(err: &str) -> bool {
    let e = err.to_lowercase();
    contains_any(
        &e,
        &[
            "this video is private",
            "private video",
            "members-only",
            "members only",
            "join this channel",
            "has been removed by",
            "account associated with this video has been terminated",
            "video has been removed for violating",
        ],
    )
}

fn is_format_error(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("requested format") || e.contains("format is not available")
}

/// A short, plain message for the last yt-dlp error.
pub fn explain_error(err: &str) -> String {
    let e = err.to_lowercase();
    if contains_any(&e, &["page needs to be reloaded", "sign in to confirm", "not a bot"]) {
        "YouTube blocked this attempt (bot check or session error). Try again in a moment. If it keeps happening, check for a yt-dlp update in Settings.".into()
    } else if is_terminal(err) {
        "This video is private, members-only or has been removed.".into()
    } else if is_format_error(err) {
        "YouTube offers no AAC audio for this video, which FreeMic needs. Try another upload of the same sound.".into()
    } else if contains_any(&e, &["unable to download", "getaddrinfo", "connection", "timed out", "network is unreachable"]) {
        "Could not reach YouTube. Check your internet connection.".into()
    } else {
        "The video appears to be unavailable or does not exist.".into()
    }
}

/// The last `ERROR:` line of yt-dlp's output (or the last line).
fn last_error_line(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines.iter().rev().find(|l| l.starts_with("ERROR:")).or(lines.last()).map(|l| l.to_string()).unwrap_or_default()
}

// ------------------------------------------------------------------ yt-dlp itself
//
// Windows: yt-dlp.exe ships with FreeMic (installer and portable zip put it
// next to FreeMic.exe). FreeMic never downloads or writes program files on
// Windows: a program that downloads an executable and saves it to disk is what
// malware "droppers" do, and Microsoft Defender flagged FreeMic 0.2.0
// (Trojan:Win32/Bearfoos.B!ml) the moment it saved a downloaded yt-dlp.exe.
// Updates use yt-dlp's own updater (`yt-dlp -U`, which checks the new file
// against the release's SHA2-256SUMS), only when the user asks for one.
//
// Linux: downloaded on first use from yt-dlp's GitHub releases into FreeMic's
// data folder and checked against the release's SHA2-256SUMS.

#[cfg(windows)]
const ASSET: &str = "yt-dlp.exe";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ASSET: &str = "yt-dlp_linux";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const ASSET: &str = "yt-dlp_linux_aarch64";

const RELEASES: &str = "https://github.com/yt-dlp/yt-dlp/releases";
const RECHECK: Duration = Duration::from_secs(6 * 3600);
const CLIENT_FALLBACKS: &[&str] = &["default,-tv", "default,web_embedded", "web_safari,mweb", "ios", "android"];

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct ToolState {
    pub version: String,
    /// Unix time of the last successful check for a newer release.
    pub checked: u64,
    /// A newer release found by that check (cleared once installed).
    #[serde(default)]
    pub latest: Option<String>,
}

impl ToolState {
    /// The newer release to offer, if any.
    pub fn update(&self) -> Option<&str> {
        self.latest.as_deref().filter(|l| newer(l, &self.version))
    }
}

/// Whether yt-dlp version `a` is newer than `b` (both `YYYY.MM.DD[.N]`).
pub fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(a) > parse(b)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(format!("FreeMic/{} (+https://github.com/VVVULTURE/FreeMic)", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(30)))
        .build()
        .into()
}

/// The newest yt-dlp version, from where GitHub's "latest release" link redirects.
fn latest_version() -> Result<String, String> {
    use ureq::ResponseExt;
    let resp = agent().head(format!("{RELEASES}/latest")).call().map_err(|e| format!("could not reach GitHub: {e}"))?;
    let uri = resp.get_uri().to_string();
    let tag = uri.rsplit("/tag/").next().filter(|_| uri.contains("/tag/")).unwrap_or("");
    if tag.is_empty() || tag.len() > 32 || !tag.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Err(format!("unexpected latest-release address: {uri}"));
    }
    Ok(tag.to_string())
}

#[cfg(not(windows))]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Expected SHA-256 of `asset` from a release's `SHA2-256SUMS` file.
pub fn sum_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == asset && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
    })
}

/// The `FileVersion` string of a Windows program (yt-dlp.exe has its version there).
#[cfg(windows)]
pub fn file_version(exe: &Path) -> Option<String> {
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    use windows::core::HSTRING;
    let path = HSTRING::from(exe.as_os_str());
    unsafe {
        let size = GetFileVersionInfoSizeW(&path, None);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        GetFileVersionInfoW(&path, None, size, buf.as_mut_ptr().cast()).ok()?;
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        // The string table named in the translation list, then the usual
        // English ones (yt-dlp.exe lists language 0000 but stores 040904B0;
        // .NET's FileVersionInfo falls back the same way).
        let mut tables = Vec::new();
        if VerQueryValueW(buf.as_ptr().cast(), &HSTRING::from("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool() && len >= 4 {
            let tr = std::slice::from_raw_parts(ptr as *const u16, 2);
            tables.push(format!("{:04x}{:04x}", tr[0], tr[1]));
        }
        tables.extend(["040904b0", "040904e4", "04090000"].map(String::from));
        for t in tables {
            let key = HSTRING::from(format!("\\StringFileInfo\\{t}\\FileVersion"));
            if VerQueryValueW(buf.as_ptr().cast(), &key, &mut ptr, &mut len).as_bool() && len > 0 {
                let v = String::from_utf16_lossy(std::slice::from_raw_parts(ptr as *const u16, len as usize));
                let v = v.trim_end_matches('\0').trim().to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        None
    }
}

pub struct Tool {
    exe: PathBuf,
    /// Remembers update checks (and, on Linux, the installed version).
    state_file: PathBuf,
    /// Shipped with FreeMic (Windows) rather than downloaded by it (Linux).
    bundled: bool,
}

impl Tool {
    /// yt-dlp as FreeMic uses it on this system; `tools_dir` is the data folder's `tools`.
    pub fn for_app(tools_dir: PathBuf) -> Tool {
        #[cfg(windows)]
        {
            // FREEMIC_YTDLP points development builds and tests at a copy.
            let exe = std::env::var_os("FREEMIC_YTDLP").map(PathBuf::from).unwrap_or_else(|| {
                std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(ASSET))).unwrap_or_else(|| PathBuf::from(ASSET))
            });
            Tool::bundled(exe, tools_dir)
        }
        #[cfg(not(windows))]
        Tool::new(tools_dir)
    }

    /// A yt-dlp.exe that came with FreeMic.
    #[cfg(windows)]
    pub fn bundled(exe: PathBuf, tools_dir: PathBuf) -> Tool {
        Tool { exe, state_file: tools_dir.join("yt-dlp.json"), bundled: true }
    }

    /// A yt-dlp that FreeMic downloads into `tools_dir`.
    #[cfg(not(windows))]
    pub fn new(tools_dir: PathBuf) -> Tool {
        Tool { exe: tools_dir.join(ASSET), state_file: tools_dir.join("yt-dlp.json"), bundled: false }
    }

    pub fn exe(&self) -> PathBuf {
        self.exe.clone()
    }

    pub fn is_bundled(&self) -> bool {
        self.bundled
    }

    fn saved(&self) -> Option<ToolState> {
        std::fs::read_to_string(&self.state_file).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    /// `None` when yt-dlp isn't there (Linux: not set up yet; Windows: missing from FreeMic's folder).
    pub fn state(&self) -> Option<ToolState> {
        if !self.exe.is_file() {
            return None;
        }
        #[cfg(windows)]
        if self.bundled {
            let mut st = self.saved().unwrap_or_default();
            st.version = file_version(&self.exe).unwrap_or_default();
            return Some(st);
        }
        self.saved()
    }

    fn save_state(&self, st: &ToolState) {
        if let Some(dir) = self.state_file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(st) {
            let _ = crate::config::write_atomic(&self.state_file, s.as_bytes());
        }
    }

    /// The message shown when yt-dlp can't be used.
    pub fn missing_message(&self) -> String {
        if self.bundled {
            format!("yt-dlp.exe is missing from FreeMic's folder ({}). Reinstall FreeMic, or download the portable version again.", self.exe.display())
        } else {
            "yt-dlp is not set up yet.".into()
        }
    }

    /// Make yt-dlp usable: on Linux, download it if it isn't there yet.
    pub fn setup(&self, progress: &dyn Fn(Option<f32>), cancel: &Arc<AtomicBool>) -> Result<ToolState, String> {
        if let Some(st) = self.state() {
            return Ok(st);
        }
        #[cfg(not(windows))]
        {
            self.install(&latest_version()?, progress, cancel)?;
            self.state().ok_or_else(|| "yt-dlp was installed but cannot be found".into())
        }
        #[cfg(windows)]
        {
            let _ = (progress, cancel);
            Err(self.missing_message())
        }
    }

    /// Look for a newer yt-dlp (when `force`, or at most every 6 hours). Never
    /// installs anything. Being offline is fine unless `force`.
    pub fn check(&self, force: bool) -> Result<ToolState, String> {
        let mut st = self.state().ok_or_else(|| self.missing_message())?;
        if !force && now().saturating_sub(st.checked) < RECHECK.as_secs() {
            return Ok(st);
        }
        match latest_version() {
            Ok(latest) => {
                st.checked = now();
                st.latest = Some(latest);
                self.save_state(&st);
                Ok(st)
            }
            Err(e) if force => Err(e),
            Err(e) => {
                log::warn!("yt-dlp update check failed: {e}");
                Ok(st)
            }
        }
    }

    /// Install the newest yt-dlp. Only ever called when the user asks for it.
    pub fn update(&self, progress: &dyn Fn(Option<f32>), cancel: &Arc<AtomicBool>) -> Result<ToolState, String> {
        let before = self.state().ok_or_else(|| self.missing_message())?;
        #[cfg(windows)]
        let result = {
            let _ = progress;
            self.self_update(cancel)
        };
        #[cfg(not(windows))]
        let result = latest_version().and_then(|v| self.install(&v, progress, cancel));
        result?;
        let mut st = self.state().ok_or_else(|| self.missing_message())?;
        st.checked = now();
        if st.latest.as_deref().is_some_and(|l| !newer(l, &st.version)) {
            st.latest = None;
        }
        self.save_state(&st);
        if st.version != before.version {
            log::info!("yt-dlp updated from {} to {}", before.version, st.version);
        }
        Ok(st)
    }

    /// `yt-dlp -U`: yt-dlp downloads its newest release, checks it against
    /// SHA2-256SUMS and replaces itself (keeping `yt-dlp.exe.old` meanwhile).
    #[cfg(windows)]
    fn self_update(&self, cancel: &Arc<AtomicBool>) -> Result<(), String> {
        let mut cmd = command(&self.exe);
        cmd.arg("-U");
        let mut out = Vec::new();
        let (code, stderr) = run(cmd, cancel, |l| out.push(l.to_string()))?;
        // yt-dlp leaves deleting the old copy to a delayed `cmd` that run()
        // already ended; do it here (the file is free once yt-dlp has exited).
        let old = PathBuf::from(format!("{}.old", self.exe.display()));
        for _ in 0..20 {
            if !old.exists() || std::fs::remove_file(&old).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if code != 0 {
            let e = last_error_line(&stderr);
            log::warn!("yt-dlp -U failed ({code}): {e} / {}", out.join(" | "));
            return Err(if e.contains("Insufficient permissions") {
                format!("yt-dlp can't update itself because FreeMic's folder is read-only for your account ({e}). Install the newest FreeMic instead.")
            } else {
                format!("The yt-dlp update failed ({e}).")
            });
        }
        log::info!("yt-dlp -U: {}", out.join(" | "));
        Ok(())
    }

    /// Remove a downloaded yt-dlp (Linux).
    #[cfg(not(windows))]
    pub fn remove(&self) {
        let _ = std::fs::remove_file(&self.exe);
        let _ = std::fs::remove_file(&self.state_file);
    }

    /// Download `version` of yt-dlp and verify it before it replaces the current copy.
    #[cfg(not(windows))]
    fn install(&self, version: &str, progress: &dyn Fn(Option<f32>), cancel: &AtomicBool) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        let dir = self.exe.parent().ok_or("bad tools folder")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        let base = format!("{RELEASES}/download/{version}");
        let sums = agent()
            .get(format!("{base}/SHA2-256SUMS"))
            .call()
            .map_err(|e| format!("could not download the checksum list: {e}"))?
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("could not read the checksum list: {e}"))?;
        let want = sum_for(&sums, ASSET).ok_or_else(|| format!("{ASSET} is not listed in the release's checksums"))?;

        let mut resp = agent().get(format!("{base}/{ASSET}")).call().map_err(|e| format!("could not download yt-dlp: {e}"))?;
        let total = resp.body().content_length();
        let mut reader = resp.body_mut().as_reader();
        let part = dir.join(format!("{ASSET}.part"));
        let mut out = std::fs::File::create(&part).map_err(|e| format!("could not write {}: {e}", part.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut got = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(out);
                let _ = std::fs::remove_file(&part);
                return Err("cancelled".into());
            }
            let n = reader.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n]).map_err(|e| format!("could not write {}: {e}", part.display()))?;
            got += n as u64;
            progress(total.map(|t| (got as f64 / t.max(1) as f64) as f32));
        }
        out.sync_all().ok();
        drop(out);
        let have = hex(&hasher.finalize());
        if have != want {
            let _ = std::fs::remove_file(&part);
            return Err(format!("the downloaded yt-dlp failed its checksum (expected {want}, got {have}); nothing was installed"));
        }
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("could not make yt-dlp executable: {e}"))?;
        }
        std::fs::rename(&part, &self.exe).map_err(|e| format!("could not install yt-dlp: {e}"))?;
        self.save_state(&ToolState { version: version.to_string(), checked: now(), latest: None });
        log::info!("yt-dlp {version} installed ({have})");
        Ok(())
    }
}

/// FreeMic 0.2.0 downloaded yt-dlp.exe into the data folder on Windows; that
/// copy is no longer used (yt-dlp now comes with FreeMic), so remove it.
#[cfg(windows)]
pub fn remove_downloaded_copy(tools_dir: &Path) {
    for name in ["yt-dlp.exe", "yt-dlp.exe.part"] {
        let p = tools_dir.join(name);
        if p.is_file() {
            match std::fs::remove_file(&p) {
                Ok(()) => log::info!("removed {} (left by FreeMic 0.2.0)", p.display()),
                Err(e) => log::warn!("could not remove {}: {e}", p.display()),
            }
        }
    }
}

/// A JavaScript runtime yt-dlp can use for YouTube's player checks, if one is
/// installed (`--js-runtimes`). Without one, the default clients still work.
/// Looked up once per run (searching `PATH` is slow on some systems).
pub fn js_runtime() -> Option<(&'static str, PathBuf)> {
    static FOUND: std::sync::OnceLock<Option<(&'static str, PathBuf)>> = std::sync::OnceLock::new();
    FOUND.get_or_init(find_js_runtime).clone()
}

fn find_js_runtime() -> Option<(&'static str, PathBuf)> {
    let path = std::env::var_os("PATH")?;
    for name in ["deno", "node", "bun"] {
        for dir in std::env::split_paths(&path) {
            #[cfg(windows)]
            let p = dir.join(format!("{name}.exe"));
            #[cfg(not(windows))]
            let p = dir.join(name);
            if p.is_file() {
                return Some((name, p));
            }
        }
    }
    None
}

fn command(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    // Ignore any yt-dlp config files on this computer (they could, e.g., turn playlists back on).
    c.args(["--ignore-config", "--no-warnings", "--encoding", "utf-8"]);
    if let Some((name, path)) = js_runtime() {
        c.arg("--js-runtimes").arg(format!("{name}:{}", path.display()));
    }
    c.env("PYTHONIOENCODING", "utf-8").env("PYTHONUTF8", "1").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // yt-dlp is a console program; without this a console window would pop up.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    c
}

/// yt-dlp and every process it starts (the standalone build unpacks itself
/// into a second process). Windows: a job object, which also ends them if
/// FreeMic exits; Linux: the process group.
struct ProcessTree {
    #[cfg(windows)]
    job: isize,
    #[cfg(unix)]
    pgid: u32,
}

impl ProcessTree {
    fn new(child: &Child) -> ProcessTree {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows::Win32::Foundation::{CloseHandle, HANDLE};
            use windows::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JobObjectExtendedLimitInformation, SetInformationJobObject,
            };
            let job = unsafe {
                let Ok(job) = CreateJobObjectW(None, windows::core::PCWSTR::null()) else {
                    log::warn!("could not create a job object for yt-dlp");
                    return ProcessTree { job: 0 };
                };
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(job, JobObjectExtendedLimitInformation, (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(), size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32);
                // yt-dlp starts its second process only after unpacking itself,
                // so it is in the job before it can start anything.
                let assign = AssignProcessToJobObject(job, HANDLE(child.as_raw_handle()));
                if set.is_err() || assign.is_err() {
                    log::warn!("could not put yt-dlp in a job object: {set:?} {assign:?}");
                    let _ = CloseHandle(job);
                    return ProcessTree { job: 0 };
                }
                job
            };
            ProcessTree { job: job.0 as isize }
        }
        #[cfg(unix)]
        ProcessTree { pgid: child.id() }
    }

    /// End every process in the tree.
    fn kill(&self) {
        #[cfg(windows)]
        if self.job != 0 {
            use windows::Win32::Foundation::HANDLE;
            use windows::Win32::System::JobObjects::TerminateJobObject;
            let _ = unsafe { TerminateJobObject(HANDLE(self.job as *mut _), 1) };
        }
        #[cfg(unix)]
        unsafe {
            unsafe extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            // SIGTERM to the whole group (negative pid).
            kill(-(self.pgid as i32), 15);
        }
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        // Windows: closing the job ends whatever yt-dlp left running.
        #[cfg(windows)]
        if self.job != 0 {
            use windows::Win32::Foundation::{CloseHandle, HANDLE};
            let _ = unsafe { CloseHandle(HANDLE(self.job as *mut _)) };
        }
    }
}

/// Run yt-dlp, calling `line` for each stdout line. Returns (exit code, stderr).
fn run(mut cmd: Command, cancel: &Arc<AtomicBool>, mut line: impl FnMut(&str)) -> Result<(i32, String), String> {
    let mut child = cmd.spawn().map_err(|e| format!("could not start yt-dlp: {e}"))?;
    let tree = Arc::new(ProcessTree::new(&child));
    let stderr = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let mut b = Vec::new();
            let _ = e.read_to_end(&mut b);
            s = String::from_utf8_lossy(&b).into_owned();
        }
        s
    });
    let stdout = child.stdout.take().ok_or("no output from yt-dlp")?;
    let finished = Arc::new(AtomicBool::new(false));
    // Watch for cancel while blocked reading output.
    let child = Arc::new(Mutex::new(child));
    let watcher = {
        let (tree, child, done, cancel) = (tree.clone(), child.clone(), finished.clone(), cancel.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    tree.kill();
                    let _ = child.lock().unwrap().kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
    };
    for l in BufReader::new(stdout).lines() {
        match l {
            Ok(l) => line(l.trim_end()),
            Err(_) => break,
        }
    }
    let status = child.lock().unwrap().wait();
    finished.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    let stderr = err_thread.join().unwrap_or_default();
    drop(tree);
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let code = status.map_err(|e| format!("yt-dlp: {e}"))?.code().unwrap_or(-1);
    Ok((code, stderr))
}

/// Search YouTube (at most 10 results, like YT-Downloader).
pub fn search(exe: &Path, query: &str, cancel: &Arc<AtomicBool>) -> Result<Vec<Video>, String> {
    let mut cmd = command(exe);
    cmd.args(["--flat-playlist", "--dump-single-json"]).arg(format!("ytsearch10:{query}"));
    let mut out = String::new();
    let (code, stderr) = run(cmd, cancel, |l| {
        out.push_str(l);
        out.push('\n');
    })?;
    if code != 0 {
        let e = last_error_line(&stderr);
        log::warn!("yt-dlp search failed ({code}): {e}");
        return Err(format!("{} ({e})", explain_error(&e)));
    }
    parse_search(&out)
}

/// Title, channel and length of one video.
pub fn lookup(exe: &Path, id: &str, cancel: &Arc<AtomicBool>) -> Result<Video, String> {
    let mut last = String::new();
    for client in CLIENT_FALLBACKS {
        let mut cmd = command(exe);
        cmd.args(["--no-playlist", "--skip-download", "--ignore-no-formats-error"])
            .args(["--print", "%(id)s\t%(duration)s\t%(channel,uploader)s\t%(title)s"])
            .args(["--extractor-args", &format!("youtube:player_client={client}")])
            .arg(watch_url(id));
        let mut lines = Vec::new();
        let (code, stderr) = run(cmd, cancel, |l| lines.push(l.to_string()))?;
        if code == 0 {
            let l = lines.iter().find(|l| l.starts_with(id)).ok_or("unexpected answer from yt-dlp")?;
            let mut f = l.splitn(4, '\t');
            let (vid, dur, chan, title) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
            return Ok(Video {
                id: vid.to_string(),
                title: title.to_string(),
                channel: if chan == "NA" { String::new() } else { chan.to_string() },
                duration: dur.parse().ok(),
                score: None,
            });
        }
        last = last_error_line(&stderr);
        log::warn!("yt-dlp lookup with client {client} failed: {last}");
        if is_terminal(&last) {
            break;
        }
    }
    Err(format!("{} ({last})", explain_error(&last)))
}

/// Download the audio of video `id` into `dir` as `<id>.m4a`. Tries the
/// player clients in YT-Downloader's order.
pub fn download(exe: &Path, id: &str, dir: &Path, progress: &dyn Fn(Option<f32>), cancel: &Arc<AtomicBool>) -> Result<PathBuf, String> {
    let tmp = dir.join(".downloading");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("could not create {}: {e}", tmp.display()))?;
    let mut last = String::new();
    let result = (|| {
        for client in CLIENT_FALLBACKS {
            let mut cmd = command(exe);
            cmd.args(["--no-playlist", "--max-downloads", "1", "--no-mtime"])
                .args(["-f", "bestaudio[ext=m4a]/bestaudio[acodec^=mp4a]"])
                .args(["--retries", "5", "--fragment-retries", "10"])
                .args(["--extractor-args", &format!("youtube:player_client={client}")])
                .args(["--progress", "--newline"])
                .args(["--progress-template", "download:FMPROG %(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s"])
                .args(["--print", "after_move:FMFILE %(filepath)s"])
                .arg("-o")
                .arg(tmp.join("%(id)s.%(ext)s"))
                .arg(watch_url(id));
            let mut files = Vec::new();
            let (code, stderr) = run(cmd, cancel, |l| {
                if let Some(p) = l.strip_prefix("FMPROG ") {
                    let n: Vec<f64> = p.split(' ').map(|x| x.parse().unwrap_or(f64::NAN)).collect();
                    let total = if n.get(1).is_some_and(|t| t.is_finite()) { n[1] } else { n.get(2).copied().unwrap_or(f64::NAN) };
                    let got = n.first().copied().unwrap_or(f64::NAN);
                    progress((got.is_finite() && total.is_finite() && total > 0.0).then(|| (got / total).clamp(0.0, 1.0) as f32));
                } else if let Some(p) = l.strip_prefix("FMFILE ") {
                    files.push(PathBuf::from(p));
                }
            })?;
            // 101 = stopped by --max-downloads after the one video, which is a success.
            if (code == 0 || code == 101) && files.len() == 1 && files[0].is_file() {
                return Ok(files.remove(0));
            }
            if files.len() > 1 {
                return Err("yt-dlp produced more than one file; nothing was added".to_string());
            }
            last = last_error_line(&stderr);
            log::warn!("yt-dlp download with client {client} failed ({code}): {last}");
            if is_terminal(&last) || is_format_error(&last) {
                break;
            }
        }
        Err(format!("{} ({last})", explain_error(&last)))
    })();
    let out = match result {
        Ok(f) => {
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let dest = dir.join(format!("{id}.{}", if ext.is_empty() { "m4a".into() } else { ext }));
            std::fs::rename(&f, &dest).map_err(|e| format!("could not move the download into the library: {e}")).map(|_| dest)
        }
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_dir_all(&tmp);
    out
}

// ------------------------------------------------------------------ background service

#[derive(Clone, Debug)]
pub enum Job {
    /// Make yt-dlp usable (Linux: download it on first use).
    Setup,
    /// Look for a newer yt-dlp. `force`: the user asked (shown, errors
    /// reported); otherwise a quiet check at most every 6 hours.
    Check { force: bool },
    /// Install the newer yt-dlp (only when the user asks).
    Update,
    Search { query: String, music: Option<(String, String)> },
    Lookup { id: String },
    Download { video: Video },
    /// Remove the downloaded yt-dlp (Linux).
    Remove,
}

impl Job {
    /// Runs without showing anything unless it changes the yt-dlp state.
    fn quiet(&self) -> bool {
        matches!(self, Job::Check { force: false })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Busy {
    Setup,
    Check,
    Update,
    Search,
    Lookup,
    Download(String),
}

/// A finished download, to be added to the soundboard by the UI.
#[derive(Clone, Debug)]
pub struct Finished {
    pub video: Video,
    pub path: PathBuf,
}

#[derive(Default)]
pub struct State {
    pub tool: Option<ToolState>,
    pub busy: Option<Busy>,
    pub progress: Option<f32>,
    pub results: Vec<Video>,
    /// The search the results belong to (for messages like "no results").
    pub searched: Option<String>,
    /// Results came from a music search (ranked, best first).
    pub ranked: bool,
    pub error: Option<String>,
    pub finished: Vec<Finished>,
}

pub struct Service {
    pub state: Arc<Mutex<State>>,
    tx: Sender<Job>,
    cancel: Arc<AtomicBool>,
    /// yt-dlp came with FreeMic (Windows) instead of being downloaded by it.
    pub bundled: bool,
    /// Why yt-dlp can't be used when it's missing.
    pub missing: String,
}

impl Service {
    /// `tools_dir`: FreeMic's tools folder; `out_dir`: where downloads go.
    pub fn start(tools_dir: PathBuf, out_dir: PathBuf, notify: Arc<dyn Fn() + Send + Sync>) -> Service {
        let tool = Tool::for_app(tools_dir.clone());
        #[cfg(windows)]
        if tool.exe().parent() != Some(tools_dir.as_path()) {
            remove_downloaded_copy(&tools_dir);
        }
        let (bundled, missing) = (tool.is_bundled(), tool.missing_message());
        let st = tool.state();
        match &st {
            Some(s) => log::info!("yt-dlp: {} ({})", tool.exe().display(), if s.version.is_empty() { "version unknown" } else { &s.version }),
            None if bundled => log::warn!("yt-dlp: {} is missing", tool.exe().display()),
            None => log::info!("yt-dlp: not set up yet"),
        }
        let state = Arc::new(Mutex::new(State { tool: st, ..Default::default() }));
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let (state, cancel) = (state.clone(), cancel.clone());
            std::thread::Builder::new()
                .name("youtube".into())
                .spawn(move || worker(rx, tool, out_dir, state, cancel, notify))
                .expect("spawn youtube worker");
        }
        Service { state, tx, cancel, bundled, missing }
    }

    pub fn send(&self, job: Job) {
        if !job.quiet() {
            self.cancel.store(false, Ordering::Relaxed);
            let mut st = self.state.lock().unwrap();
            st.error = None;
            st.progress = None;
            st.busy = Some(match &job {
                Job::Setup | Job::Remove => Busy::Setup,
                Job::Check { .. } => Busy::Check,
                Job::Update => Busy::Update,
                Job::Search { .. } => Busy::Search,
                Job::Lookup { .. } => Busy::Lookup,
                Job::Download { video } => Busy::Download(video.id.clone()),
            });
        }
        let _ = self.tx.send(job);
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

fn worker(rx: Receiver<Job>, tool: Tool, out_dir: PathBuf, state: Arc<Mutex<State>>, cancel: Arc<AtomicBool>, notify: Arc<dyn Fn() + Send + Sync>) {
    let set = |f: &dyn Fn(&mut State)| {
        f(&mut state.lock().unwrap());
        notify();
    };
    let progress = |p: Option<f32>| {
        state.lock().unwrap().progress = p;
        notify();
    };
    let set_tool = |st: &ToolState| {
        set(&|s| {
            s.tool = Some(st.clone());
            s.progress = None;
        })
    };
    while let Ok(job) = rx.recv() {
        let result: Result<(), String> = (|| {
            match &job {
                Job::Remove => {
                    #[cfg(not(windows))]
                    tool.remove();
                    set(&|s| s.tool = tool.state());
                }
                Job::Setup => set_tool(&tool.setup(&progress, &cancel)?),
                Job::Check { force } => set_tool(&tool.check(*force)?),
                Job::Update => set_tool(&tool.update(&progress, &cancel)?),
                Job::Search { .. } | Job::Lookup { .. } | Job::Download { .. } => {
                    if tool.state().is_none() {
                        set(&|s| s.busy = Some(Busy::Setup));
                    }
                    let st = tool.setup(&progress, &cancel)?;
                    // Only looks for a newer yt-dlp; never installs one by itself.
                    set_tool(&tool.check(false).unwrap_or(st));
                }
            }
            let exe = tool.exe();
            match &job {
                Job::Search { query, music } => {
                    set(&|s| s.busy = Some(Busy::Search));
                    let mut v = search(&exe, query, &cancel)?;
                    if let Some((a, s)) = music {
                        rank_music(&mut v, a, s);
                    }
                    let ranked = music.is_some();
                    set(&|s| {
                        s.results = v.clone();
                        s.searched = Some(query.clone());
                        s.ranked = ranked;
                    });
                }
                Job::Lookup { id } => {
                    set(&|s| s.busy = Some(Busy::Lookup));
                    let v = lookup(&exe, id, &cancel)?;
                    set(&|s| {
                        s.results = vec![v.clone()];
                        s.searched = None;
                        s.ranked = false;
                    });
                }
                Job::Download { video } => {
                    std::fs::create_dir_all(&out_dir).map_err(|e| format!("could not create {}: {e}", out_dir.display()))?;
                    let path = download(&exe, &video.id, &out_dir, &progress, &cancel)?;
                    log::info!("downloaded YouTube video {} to {}", video.id, path.display());
                    set(&|s| s.finished.push(Finished { video: video.clone(), path: path.clone() }));
                }
                _ => {}
            }
            Ok(())
        })();
        if job.quiet() {
            if let Err(e) = result {
                log::warn!("YouTube: {e}");
            }
            continue;
        }
        let cancelled = cancel.load(Ordering::Relaxed);
        set(&|s| {
            s.busy = None;
            s.progress = None;
            if let Err(e) = &result {
                if !cancelled {
                    s.error = Some(e.clone());
                }
            }
        });
        if let Err(e) = result {
            log::warn!("YouTube: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(id: &str, list: bool) -> Option<Link> {
        Some(Link::Video { id: id.into(), in_list: list })
    }

    #[test]
    fn single_video_links_are_recognized() {
        let id = "dQw4w9WgXcQ";
        assert_eq!(parse_link("https://www.youtube.com/watch?v=dQw4w9WgXcQ"), video(id, false));
        assert_eq!(parse_link("youtube.com/watch?v=dQw4w9WgXcQ&t=42s"), video(id, false));
        assert_eq!(parse_link("https://m.youtube.com/watch?feature=share&v=dQw4w9WgXcQ"), video(id, false));
        assert_eq!(parse_link("https://music.youtube.com/watch?v=dQw4w9WgXcQ&si=abc"), video(id, false));
        assert_eq!(parse_link("https://youtu.be/dQw4w9WgXcQ?si=xyz"), video(id, false));
        assert_eq!(parse_link("https://www.youtube.com/shorts/dQw4w9WgXcQ"), video(id, false));
        assert_eq!(parse_link("https://www.youtube.com/embed/dQw4w9WgXcQ"), video(id, false));
        assert_eq!(parse_link("https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ"), video(id, false));
        assert_eq!(parse_link("https://www.youtube.com/live/dQw4w9WgXcQ?feature=share"), video(id, false));
        assert_eq!(parse_link("HTTPS://WWW.YOUTUBE.COM/watch?v=dQw4w9WgXcQ#t=1"), video(id, false));
    }

    #[test]
    fn playlists_are_never_treated_as_videos() {
        // A video inside a playlist or mix: only that video.
        assert_eq!(parse_link("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=RDdQw4w9WgXcQ&start_radio=1"), video("dQw4w9WgXcQ", true));
        assert_eq!(parse_link("https://youtu.be/dQw4w9WgXcQ?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI"), video("dQw4w9WgXcQ", true));
        // Links that are not one video.
        for l in [
            "https://www.youtube.com/playlist?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI",
            "https://www.youtube.com/watch?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI",
            "https://music.youtube.com/playlist?list=OLAK5uy_abc",
            "https://www.youtube.com/@SomeChannel",
            "https://www.youtube.com/channel/UC38IQsAvIsxxjztdMZQtwHA",
            "https://www.youtube.com/results?search_query=airhorn",
            "https://www.youtube.com/",
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ%26",
        ] {
            assert_eq!(parse_link(l), Some(Link::NotAVideo), "{l}");
        }
        assert_eq!(parse_link("https://vimeo.com/123"), Some(Link::NotYouTube));
        assert_eq!(parse_link("https://evil.example/youtube.com/watch?v=dQw4w9WgXcQ"), Some(Link::NotYouTube));
        // Searches are not links.
        assert_eq!(parse_link("air horn"), None);
        assert_eq!(parse_link("vine-boom"), None);
        assert_eq!(parse_link("dQw4w9WgXcQ"), None);
    }

    #[test]
    fn music_query_matches_yt_downloader() {
        assert_eq!(music_query("Rick Astley", "Never Gonna Give You Up").as_deref(), Some("Rick Astley Never Gonna Give You Up"));
        assert_eq!(music_query(" Rick Astley ", "").as_deref(), Some("'Rick Astley' official music video"));
        assert_eq!(music_query("", "Never Gonna Give You Up").as_deref(), Some("'Never Gonna Give You Up' song official"));
        assert_eq!(music_query(" ", ""), None);
    }

    #[test]
    fn ratio_matches_python_difflib() {
        // Expected values computed with Python 3 difflib.SequenceMatcher(None, a.lower(), b.lower()).ratio().
        let cases: &[(&str, &str, f64)] = &[
            ("Never Gonna Give You Up", "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)", 0.5054945054945055),
            ("never gonna give you up", "never gonna give you up (2022 remaster)", 0.7419354838709677),
            ("abcd", "bcda", 0.75),
            ("", "anything", 0.0),
            ("", "", 1.0),
            ("hello", "HeLLo", 1.0),
        ];
        // b has 253 items, so difflib's "autojunk" drops its popular characters.
        let long = format!("{}{}xyz", "a".repeat(150), "b".repeat(100));
        assert!((ratio(&format!("{}xyz", "ab".repeat(10)), &long) - 0.036231884057971016).abs() < 1e-12);
        for (a, b, want) in cases {
            let got = ratio(a, b);
            assert!((got - want).abs() < 1e-12, "{a:?} vs {b:?}: {got} != {want}");
        }
    }

    #[test]
    fn music_ranking_matches_yt_downloader() {
        let mut v: Vec<Video> = [
            ("a", "Never Gonna Give You Up (Live at the BBC concert)", "BBC"),
            ("b", "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)", "Rick Astley"),
            ("c", "Never Gonna Give You Up karaoke version with lyrics", "Sing King"),
            ("d", "Rick Astley - Never Gonna Give You Up", "RickAstleyVEVO"),
        ]
        .iter()
        .map(|(id, t, c)| Video { id: id.to_string(), title: t.to_string(), channel: c.to_string(), duration: None, score: None })
        .collect();
        rank_music(&mut v, "Rick Astley", "Never Gonna Give You Up");
        let order: Vec<&str> = v.iter().map(|x| x.id.as_str()).collect();
        // Scores from YT-Downloader's score_music run in Python on the same input: b 45, d 40, c 4, a -15.
        assert_eq!(order, ["b", "d", "c", "a"], "{v:?}");
        let scores: Vec<i32> = v.iter().map(|x| x.score.unwrap()).collect();
        assert_eq!(scores, [45, 40, 4, -15]);
    }

    #[test]
    fn search_output_is_parsed_and_live_streams_dropped() {
        let json = r#"{"_type":"playlist","entries":[
            {"_type":"url","ie_key":"Youtube","id":"dQw4w9WgXcQ","title":"Rick","channel":"Rick Astley","duration":213.0,"live_status":null},
            {"_type":"url","ie_key":"Youtube","id":"aaaaaaaaaaa","title":"Live now","channel":"X","duration":null,"live_status":"is_live"},
            {"_type":"url","ie_key":"YoutubeTab","id":"UCxxxxxxxxxxxxxxxx","title":"A channel"},
            {"_type":"url","ie_key":"Youtube","id":"bbbbbbbbbbb","title":"No channel","uploader":"Up","duration":5}
        ]}"#;
        let v = parse_search(json).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].channel, "Rick Astley");
        assert_eq!(v[0].duration, Some(213.0));
        assert_eq!(v[1].channel, "Up");
        assert!(parse_search("not json").is_err());
    }

    #[test]
    fn checksums_file_is_read_exactly() {
        let sums = "e51e69e89489f199530f13d32a4342da3ddb2cf34842f364603a066db05afbf3  yt-dlp\n\
                    f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d  yt-dlp.exe\n\
                    767ba9dce7052aa196f8b12652f433d76de128ba168966fa84b6394d5e6c6795  yt-dlp_linux\n\
                    e87e9533e89fe5889abaa21f5114dfe7756d6aa6219b83551add04d48b18e483  yt-dlp_linux_aarch64\n";
        assert_eq!(sum_for(sums, "yt-dlp.exe").as_deref(), Some("f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d"));
        assert_eq!(sum_for(sums, "yt-dlp_linux").as_deref(), Some("767ba9dce7052aa196f8b12652f433d76de128ba168966fa84b6394d5e6c6795"));
        assert_eq!(sum_for(sums, "yt-dlp_macos"), None);
    }

    #[test]
    fn errors_are_explained() {
        assert!(explain_error("ERROR: [youtube] x: Sign in to confirm you're not a bot").contains("blocked"));
        assert!(explain_error("ERROR: [youtube] x: Private video. Sign in if you've been granted access").contains("private"));
        assert!(explain_error("ERROR: [youtube] x: Requested format is not available").contains("AAC"));
        assert!(is_terminal("ERROR: This video is private"));
        assert!(!is_terminal("ERROR: HTTP Error 403: Forbidden"));
        assert_eq!(last_error_line("WARNING: a\nERROR: first\nERROR: second\n"), "ERROR: second");
    }

    #[test]
    fn updates_are_offered_only_for_newer_versions() {
        assert!(newer("2026.11.02", "2026.10.26"));
        assert!(newer("2026.10.26.1", "2026.10.26"));
        assert!(newer("2027.01.01", "2026.12.31"));
        assert!(!newer("2026.10.26", "2026.10.26"));
        assert!(!newer("2026.10.21", "2026.10.26"));
        let st = |v: &str, l: Option<&str>| ToolState { version: v.into(), checked: 1, latest: l.map(String::from) };
        assert_eq!(st("2026.10.21", Some("2026.10.26")).update(), Some("2026.10.26"));
        assert_eq!(st("2026.10.26", Some("2026.10.26")).update(), None);
        assert_eq!(st("2026.10.26", None).update(), None);
        // A state file written by FreeMic 0.2.0 (no "latest") still loads.
        let old: ToolState = serde_json::from_str(r#"{"version":"2026.10.26","checked":1793470464}"#).unwrap();
        assert_eq!(old, ToolState { version: "2026.10.26".into(), checked: 1793470464, latest: None });
    }

    #[cfg(windows)]
    #[test]
    fn windows_version_resource_is_read() {
        let notepad = std::path::Path::new(r"C:\Windows\System32\notepad.exe");
        assert!(file_version(notepad).is_some_and(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit())));
        assert_eq!(file_version(std::path::Path::new(r"C:\does-not-exist.exe")), None);
    }
}

#[cfg(test)]
mod difflib_crosscheck {
    /// Compares `ratio` with Python's difflib on generated cases:
    /// FREEMIC_DIFFLIB_CASES=cases.json cargo test -- --ignored difflib
    #[test]
    #[ignore]
    fn difflib_cases_from_python() {
        let path = std::env::var("FREEMIC_DIFFLIB_CASES").unwrap();
        let cases: Vec<(String, String, f64)> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let bad: Vec<_> = cases.iter().filter(|(a, b, r)| (super::ratio(a, b) - r).abs() > 1e-12).collect();
        assert!(bad.is_empty(), "{} of {} differ, first: {:?}", bad.len(), cases.len(), bad.first());
        println!("{} cases match", cases.len());
    }
}
