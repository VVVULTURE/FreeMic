//! Live YouTube tests (need the internet, so they are not part of the normal run):
//!
//!     cargo test --release --test youtube_live -- --ignored --nocapture --test-threads 1
//!
//! Windows: yt-dlp ships with FreeMic, so point `FREEMIC_YTDLP` at a copy of
//! the official yt-dlp.exe (it is copied into a temporary folder first). For
//! `yt_dlp_self_update`, point `FREEMIC_YTDLP_OLD` at an older official
//! yt-dlp.exe.
//! Linux: yt-dlp is downloaded into a temporary folder (SHA-256 checked).
//!
//! Then: searches, looks up a video, downloads its audio, decodes it with
//! FreeMic's decoder, checks that a playlist link is refused and a link inside
//! a playlist only fetches that one video, and that Cancel stops yt-dlp.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use freemic::audio::decode;
use freemic::youtube::{self, Link, Tool};

/// "Me at the zoo", the first YouTube video (19 s).
const VIDEO: &str = "jNQXAC9IVRw";

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("freemic-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// yt-dlp.exe processes started from `dir` that are still running.
#[cfg(windows)]
fn running_from(dir: &std::path::Path) -> usize {
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "Get-Process yt-dlp -ErrorAction SilentlyContinue | ForEach-Object { $_.Path }"])
        .output()
        .unwrap();
    let dir = dir.to_string_lossy().to_lowercase();
    String::from_utf8_lossy(&out.stdout).lines().filter(|l| l.to_lowercase().starts_with(&dir)).count()
}

#[cfg(windows)]
fn tool(dir: &std::path::Path, var: &str) -> Tool {
    let src = std::env::var_os(var).unwrap_or_else(|| panic!("set {var} to an official yt-dlp.exe"));
    let exe = dir.join("app").join("yt-dlp.exe");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::copy(src, &exe).unwrap();
    Tool::bundled(exe, dir.join("tools"))
}

#[test]
#[ignore]
fn youtube_end_to_end() {
    let dir = temp_dir("yt-live");
    let cancel = Arc::new(AtomicBool::new(false));

    // 1. yt-dlp ready: Windows reads the version of the bundled copy; Linux
    //    installs it from GitHub, verified against SHA2-256SUMS.
    #[cfg(windows)]
    let tool = tool(&dir, "FREEMIC_YTDLP");
    #[cfg(not(windows))]
    let tool = Tool::new(dir.join("tools"));
    let st = tool.setup(&|_| {}, &cancel).expect("yt-dlp ready");
    println!("yt-dlp {} at {} (bundled: {})", st.version, tool.exe().display(), tool.is_bundled());
    assert!(tool.exe().is_file());
    assert!(!st.version.is_empty() && st.version.starts_with("20"), "{st:?}");
    let modified = std::fs::metadata(tool.exe()).unwrap().modified().unwrap();
    // A forced check asks GitHub for the newest version and never touches yt-dlp itself.
    let checked = tool.check(true).expect("update check");
    println!("update check: {checked:?}");
    assert!(checked.checked > 0 && checked.latest.is_some());
    assert_eq!(std::fs::metadata(tool.exe()).unwrap().modified().unwrap(), modified);
    // An unforced check within 6 hours is answered from the saved state.
    assert_eq!(tool.check(false).unwrap().checked, checked.checked);

    // 2. Search.
    let results = youtube::search(&tool.exe(), "me at the zoo", &cancel).expect("search");
    println!("search: {} results, first: {:?}", results.len(), results.first().map(|v| (&v.id, &v.title)));
    assert!(!results.is_empty() && results.len() <= 10);
    assert!(results.iter().any(|v| v.id == VIDEO), "{results:?}");

    // 3. Music search ranked like YT-Downloader.
    let q = youtube::music_query("Rick Astley", "Never Gonna Give You Up").unwrap();
    let mut music = youtube::search(&tool.exe(), &q, &cancel).expect("music search");
    youtube::rank_music(&mut music, "Rick Astley", "Never Gonna Give You Up");
    println!("music best match: {:?} score {:?}", music[0].title, music[0].score);
    assert!(music.windows(2).all(|w| w[0].score >= w[1].score));

    // 4. A link inside a mix: only the one video.
    let link = format!("https://www.youtube.com/watch?v={VIDEO}&list=RD{VIDEO}&start_radio=1");
    let Some(Link::Video { id, in_list: true }) = youtube::parse_link(&link) else { panic!("link not parsed") };
    let v = youtube::lookup(&tool.exe(), &id, &cancel).expect("lookup");
    println!("lookup: {:?} by {:?}, {:?} s", v.title, v.channel, v.duration);
    assert_eq!(v.id, VIDEO);
    assert!(v.duration.is_some_and(|d| (18.0..=20.0).contains(&d)));

    // 5. Download the audio and decode it.
    let out = dir.join("sounds").join("youtube");
    std::fs::create_dir_all(&out).unwrap();
    let last = std::sync::Mutex::new(None);
    let path = youtube::download(&tool.exe(), VIDEO, &out, &|p| *last.lock().unwrap() = p, &cancel).expect("download");
    println!("downloaded {} ({} bytes), last progress {:?}", path.display(), std::fs::metadata(&path).unwrap().len(), last.lock().unwrap());
    assert_eq!(path, out.join(format!("{VIDEO}.m4a")));
    let files: Vec<_> = std::fs::read_dir(&out).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(files.len(), 1, "exactly one file, no leftovers: {files:?}");
    let info = decode::probe(&path).expect("FreeMic can decode the download");
    println!("decoded: {} Hz, {} ch, {:?} s", info.sample_rate, info.channels, info.duration_secs);
    assert!(info.duration_secs.is_some_and(|d| (18.0..=20.0).contains(&d)));

    // 6. Playlist links are refused before yt-dlp is ever called.
    assert_eq!(youtube::parse_link("https://www.youtube.com/playlist?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI"), Some(Link::NotAVideo));

    // 7. Cancel stops a running yt-dlp (and the process it unpacks into) quickly.
    let c2 = Arc::new(AtomicBool::new(false));
    let c3 = c2.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        c3.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let started = std::time::Instant::now();
    let r = youtube::search(&tool.exe(), "cancel test", &c2);
    t.join().unwrap();
    println!("cancelled search returned {:?} after {:?}", r.as_ref().err(), started.elapsed());
    assert_eq!(r.err().as_deref(), Some("cancelled"));
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    #[cfg(windows)]
    {
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_eq!(running_from(&dir), 0, "no yt-dlp process left");
    }

    #[cfg(not(windows))]
    {
        tool.remove();
        assert!(!tool.exe().exists());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Windows: "Update yt-dlp" runs yt-dlp's own updater on an older official
/// build; afterwards the newest version is in place, verified by yt-dlp
/// against SHA2-256SUMS, and nothing is left behind.
#[cfg(windows)]
#[test]
#[ignore]
fn yt_dlp_self_update() {
    let dir = temp_dir("yt-update");
    let cancel = Arc::new(AtomicBool::new(false));
    let tool = tool(&dir, "FREEMIC_YTDLP_OLD");
    let before = tool.check(true).expect("check");
    println!("before: {before:?}");
    let latest = before.update().expect("FREEMIC_YTDLP_OLD must be older than the newest release").to_string();
    let started = std::time::Instant::now();
    let after = tool.update(&|_| {}, &cancel).expect("update");
    println!("after {:?}: {after:?}", started.elapsed());
    assert_eq!(after.version, latest);
    assert_eq!(after.update(), None);
    assert_eq!(youtube::file_version(&tool.exe()).as_deref(), Some(latest.as_str()));
    let left: Vec<_> = std::fs::read_dir(tool.exe().parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(left, vec![std::ffi::OsString::from("yt-dlp.exe")], "no .old / .new copies left");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(running_from(&dir), 0, "no yt-dlp process left");
    // The updated copy works.
    let v = youtube::lookup(&tool.exe(), VIDEO, &cancel).expect("lookup with the updated yt-dlp");
    assert_eq!(v.id, VIDEO);
    let _ = std::fs::remove_dir_all(&dir);
}
