//! Live YouTube test (needs the internet, so it is not part of the normal run):
//!
//!     cargo test --release --test youtube_live -- --ignored --nocapture
//!
//! Installs yt-dlp into a temporary folder (download + SHA-256 check), searches,
//! looks up a video, downloads its audio, decodes it with FreeMic's decoder, and
//! checks that a playlist link is refused and a link inside a playlist only
//! fetches that one video.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use freemic::audio::decode;
use freemic::youtube::{self, Link, Tool};

/// "Me at the zoo", the first YouTube video (19 s).
const VIDEO: &str = "jNQXAC9IVRw";

#[test]
#[ignore]
fn youtube_end_to_end() {
    let dir = std::env::temp_dir().join(format!("freemic-yt-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tool = Tool::new(dir.join("tools"));
    let cancel = Arc::new(AtomicBool::new(false));

    // 1. Install yt-dlp from GitHub, verified against SHA2-256SUMS.
    let st = tool.ensure(false, &|_| {}, &cancel).expect("install yt-dlp");
    println!("installed yt-dlp {} at {}", st.version, tool.exe().display());
    assert!(tool.exe().is_file());
    // A second call within 6 hours does not touch the network or reinstall.
    let modified = std::fs::metadata(tool.exe()).unwrap().modified().unwrap();
    let st2 = tool.ensure(false, &|_| {}, &cancel).unwrap();
    assert_eq!(st2.version, st.version);
    assert_eq!(std::fs::metadata(tool.exe()).unwrap().modified().unwrap(), modified);
    // A forced check finds the same version and keeps the file.
    let st3 = tool.ensure(true, &|_| {}, &cancel).unwrap();
    assert_eq!(st3.version, st.version);
    assert_eq!(std::fs::metadata(tool.exe()).unwrap().modified().unwrap(), modified);

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

    // 7. Cancel stops a running yt-dlp quickly.
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

    tool.remove();
    assert!(!tool.exe().exists());
    let _ = std::fs::remove_dir_all(&dir);
}
