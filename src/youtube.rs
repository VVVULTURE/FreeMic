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

use std::io::{BufRead, BufReader, Read, Write};
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

#[cfg(windows)]
const ASSET: &str = "yt-dlp.exe";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const ASSET: &str = "yt-dlp_linux";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const ASSET: &str = "yt-dlp_linux_aarch64";

const RELEASES: &str = "https://github.com/yt-dlp/yt-dlp/releases";
const RECHECK: Duration = Duration::from_secs(6 * 3600);
const CLIENT_FALLBACKS: &[&str] = &["default,-tv", "default,web_embedded", "web_safari,mweb", "ios", "android"];

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct ToolState {
    pub version: String,
    /// Unix time of the last successful check for a newer release.
    pub checked: u64,
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

pub struct Tool {
    dir: PathBuf,
}

impl Tool {
    pub fn new(tools_dir: PathBuf) -> Tool {
        Tool { dir: tools_dir }
    }

    pub fn exe(&self) -> PathBuf {
        self.dir.join(ASSET)
    }

    fn state_file(&self) -> PathBuf {
        self.dir.join("yt-dlp.json")
    }

    pub fn state(&self) -> Option<ToolState> {
        if !self.exe().is_file() {
            return None;
        }
        std::fs::read_to_string(self.state_file()).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    fn save_state(&self, st: &ToolState) {
        if let Ok(s) = serde_json::to_string_pretty(st) {
            let _ = crate::config::write_atomic(&self.state_file(), s.as_bytes());
        }
    }

    pub fn remove(&self) {
        let _ = std::fs::remove_file(self.exe());
        let _ = std::fs::remove_file(self.state_file());
    }

    /// Download `version` of yt-dlp and verify it before it replaces the current copy.
    fn install(&self, version: &str, progress: &dyn Fn(Option<f32>), cancel: &AtomicBool) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("could not create {}: {e}", self.dir.display()))?;
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
        let part = self.dir.join(format!("{ASSET}.part"));
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("could not make yt-dlp executable: {e}"))?;
        }
        std::fs::rename(&part, self.exe()).map_err(|e| format!("could not install yt-dlp: {e}"))?;
        self.save_state(&ToolState { version: version.to_string(), checked: now() });
        log::info!("yt-dlp {version} installed ({have})");
        Ok(())
    }

    /// Make sure yt-dlp is there and, at most every 6 hours (or when `force`),
    /// up to date. Being offline is fine when a copy is already installed.
    pub fn ensure(&self, force: bool, progress: &dyn Fn(Option<f32>), cancel: &AtomicBool) -> Result<ToolState, String> {
        let current = self.state();
        if let Some(st) = &current {
            if !force && now().saturating_sub(st.checked) < RECHECK.as_secs() {
                return Ok(st.clone());
            }
        }
        let latest = match latest_version() {
            Ok(v) => v,
            Err(e) => {
                return match current {
                    Some(st) => {
                        log::warn!("yt-dlp update check failed: {e}");
                        Ok(st)
                    }
                    None => Err(e),
                };
            }
        };
        match current {
            Some(mut st) if st.version == latest => {
                st.checked = now();
                self.save_state(&st);
                Ok(st)
            }
            _ => {
                self.install(&latest, progress, cancel)?;
                self.state().ok_or_else(|| "yt-dlp was installed but cannot be found".into())
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

/// Stop yt-dlp and the process it unpacks itself into.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill").args(["/T", "/F", "/PID", &child.id().to_string()]).creation_flags(0x0800_0000).output();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{}", child.id())]).output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Run yt-dlp, calling `line` for each stdout line. Returns (success, stdout, stderr).
fn run(mut cmd: Command, cancel: &Arc<AtomicBool>, mut line: impl FnMut(&str)) -> Result<(i32, String), String> {
    let mut child = cmd.spawn().map_err(|e| format!("could not start yt-dlp: {e}"))?;
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
        let (child, done, cancel) = (child.clone(), finished.clone(), cancel.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    kill_tree(&mut child.lock().unwrap());
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
    /// Install yt-dlp (or check for an update when `force`).
    Setup { force: bool },
    Search { query: String, music: Option<(String, String)> },
    Lookup { id: String },
    Download { video: Video },
    Remove,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Busy {
    Setup,
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
    pub tool_dir: PathBuf,
}

impl Service {
    /// `tools_dir`: where yt-dlp lives; `out_dir`: where downloads go.
    pub fn start(tools_dir: PathBuf, out_dir: PathBuf, notify: Arc<dyn Fn() + Send + Sync>) -> Service {
        let tool = Tool::new(tools_dir.clone());
        let state = Arc::new(Mutex::new(State { tool: tool.state(), ..Default::default() }));
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let (state, cancel) = (state.clone(), cancel.clone());
            std::thread::Builder::new()
                .name("youtube".into())
                .spawn(move || worker(rx, tool, out_dir, state, cancel, notify))
                .expect("spawn youtube worker");
        }
        Service { state, tx, cancel, tool_dir: tools_dir }
    }

    pub fn send(&self, job: Job) {
        self.cancel.store(false, Ordering::Relaxed);
        {
            let mut st = self.state.lock().unwrap();
            st.error = None;
            st.progress = None;
            st.busy = Some(match &job {
                Job::Setup { .. } | Job::Remove => Busy::Setup,
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
    while let Ok(job) = rx.recv() {
        let result: Result<(), String> = (|| {
            if let Job::Remove = job {
                tool.remove();
                set(&|s| s.tool = None);
                return Ok(());
            }
            let force = matches!(job, Job::Setup { force: true });
            let need_setup = !tool.exe().is_file();
            if need_setup {
                set(&|s| s.busy = Some(Busy::Setup));
            }
            let st = tool.ensure(force, &progress, &cancel)?;
            set(&|s| {
                s.tool = Some(st.clone());
                s.progress = None;
            });
            let exe = tool.exe();
            match &job {
                Job::Setup { .. } | Job::Remove => {}
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
            }
            Ok(())
        })();
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
