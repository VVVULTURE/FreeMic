//! Developer test tool (not shipped). Read-only device inspection, recording
//! from a device the way a receiving app (Discord, a game) would, and offline
//! analysis of recordings.
//!
//!   fmtest list                         endpoints + processes with sessions on them
//!   fmtest record <device-substring> <secs> <out.wav>
//!   fmtest analyze <file.wav> <freq>... amplitude of each tone, peak, clipping
//!   fmtest tone <out.wav> <secs> <freq> <amp> [rate]
//!   fmtest play <device-substring> <file.wav>   render a file to an output device
//!                                              (tests only ever use the virtual cable)

use std::path::PathBuf;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let r = match a.first().map(String::as_str) {
        Some("list") => list(),
        Some("record") if a.len() >= 4 => record(&a[1], a[2].parse().unwrap_or(3.0), &PathBuf::from(&a[3])),
        Some("analyze") if a.len() >= 2 => analyze(&PathBuf::from(&a[1]), &a[2..]),
        Some("tone") if a.len() >= 5 => tone(&a[1..]),
        Some("icons") if a.len() >= 2 => icons(&PathBuf::from(&a[1])),
        Some("play") if a.len() >= 3 => play(&a[1], &PathBuf::from(&a[2])),
        _ => Err("usage: fmtest list | record <dev> <secs> <out.wav> | analyze <wav> <freq>... | tone <out> <secs> <freq> <amp> [rate] | play <dev> <wav> | icons <dir>".into()),
    };
    if let Err(e) = r {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Minimal PNG encoder (stored deflate blocks) so icons need no image crates.
fn png(size: u32, rgba: &[u8]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
    }
    let mut raw = Vec::new();
    for row in rgba.chunks_exact(size as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65535).enumerate() {
        let last = (i + 1) * 65535 >= raw.len();
        z.push(last as u8);
        z.extend_from_slice(&(block.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn icons(dir: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let pngs: Vec<(u32, Vec<u8>)> = sizes.iter().map(|&s| (s, png(s, &freemic::icon::rgba(s)))).collect();
    for (s, p) in &pngs {
        std::fs::write(dir.join(format!("freemic-{s}.png")), p).map_err(|e| e.to_string())?;
    }
    // ICO with PNG-compressed images (supported since Windows Vista).
    let mut ico = vec![0, 0, 1, 0];
    ico.extend_from_slice(&(pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len() as u32;
    for (s, p) in &pngs {
        let d = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[d, d, 0, 0, 1, 0, 32, 0]);
        ico.extend_from_slice(&(p.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += p.len() as u32;
    }
    for (_, p) in &pngs {
        ico.extend_from_slice(p);
    }
    std::fs::write(dir.join("freemic.ico"), ico).map_err(|e| e.to_string())
}

fn tone(a: &[String]) -> Result<(), String> {
    let (out, secs, f, amp) = (PathBuf::from(&a[0]), a[1].parse::<f64>().unwrap(), a[2].parse::<f64>().unwrap(), a[3].parse::<f32>().unwrap());
    let rate: u32 = a.get(4).and_then(|r| r.parse().ok()).unwrap_or(48000);
    let n = (secs * rate as f64) as usize;
    let mono: Vec<f32> = (0..n).map(|i| amp * (2.0 * std::f64::consts::PI * f * i as f64 / rate as f64).sin() as f32).collect();
    freemic::audio::wav::write_pcm16(&out, rate, 1, &mono).map_err(|e| e.to_string())
}

fn goertzel(x: &[f32], freq: f64, rate: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * freq / rate;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &v in x {
        let s0 = v as f64 + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    2.0 * (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0).sqrt() / x.len().max(1) as f64
}

fn analyze(path: &PathBuf, freqs: &[String]) -> Result<(), String> {
    let st = freemic::audio::decode::decode_all(path, 48000)?;
    let l: Vec<f32> = st.chunks_exact(2).map(|f| f[0]).collect();
    let secs = l.len() as f64 / 48000.0;
    let peak = st.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let clipped = st.iter().filter(|s| s.abs() >= 0.999).count();
    println!("duration {secs:.2}s peak {peak:.3} clipped_samples {clipped}");
    // Per-second breakdown so timelines (mic only / mic+sound / ...) are visible.
    for (i, sec) in l.chunks(48000).enumerate() {
        let mut line = format!("  t={i:>3}s rms {:.3}", (sec.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / sec.len() as f64).sqrt());
        for f in freqs {
            let f: f64 = f.parse().map_err(|_| format!("bad freq {f}"))?;
            line += &format!("  {f}Hz {:.3}", goertzel(sec, f, 48000.0));
        }
        println!("{line}");
    }
    Ok(())
}

#[cfg(windows)]
fn list() -> Result<(), String> {
    use wasapi::*;
    let _ = initialize_mta();
    let en = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    for dir in [Direction::Render, Direction::Capture] {
        println!("== {dir}");
        let coll = en.get_device_collection(&dir).map_err(|e| e.to_string())?;
        for d in &coll {
            let d = d.map_err(|e| e.to_string())?;
            let name = d.get_friendlyname().unwrap_or_default();
            let fmt = d.get_iaudioclient().ok().and_then(|c| c.get_mixformat().ok());
            let fmt = fmt.map(|f| format!("{} Hz {} ch", f.get_samplespersec(), f.get_nchannels())).unwrap_or_default();
            println!("  {name}  [{fmt}]  id={}", d.get_id().unwrap_or_default());
            let mgr = d.get_iaudiosessionmanager().map_err(|e| e.to_string())?;
            let en2 = mgr.get_audiosessionenumerator().map_err(|e| e.to_string())?;
            for i in 0..en2.get_count().unwrap_or(0) {
                if let Ok(s) = en2.get_session(i) {
                    let pid = s.get_process_id().unwrap_or(0);
                    let state = s.get_state().map(|s| format!("{s:?}")).unwrap_or_default();
                    let pname = process_name(pid);
                    println!("      session pid {pid} ({pname}) state {state}");
                }
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn process_name(pid: u32) -> String {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output();
    out.ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).split(',').next().unwrap_or("").trim_matches('"').to_string())
        .unwrap_or_default()
}

/// Record like a receiving application: shared-mode capture at the device format.
#[cfg(windows)]
fn record(dev: &str, secs: f64, out: &PathBuf) -> Result<(), String> {
    use wasapi::*;
    let _ = initialize_mta();
    let en = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let coll = en.get_device_collection(&Direction::Capture).map_err(|e| e.to_string())?;
    let mut found = None;
    for d in &coll {
        let d = d.map_err(|e| e.to_string())?;
        if d.get_friendlyname().unwrap_or_default().to_lowercase().contains(&dev.to_lowercase()) {
            found = Some(d);
            break;
        }
    }
    let d = found.ok_or(format!("no capture device matching {dev}"))?;
    println!("recording from {}", d.get_friendlyname().unwrap_or_default());
    let mut client = d.get_iaudioclient().map_err(|e| e.to_string())?;
    let mix = client.get_mixformat().map_err(|e| e.to_string())?;
    let rate = mix.get_samplespersec();
    let fmt = WaveFormat::new(32, 32, &SampleType::Float, rate as usize, 2, None);
    let (def, _) = client.get_device_period().map_err(|e| e.to_string())?;
    client
        .initialize_client(&fmt, &Direction::Capture, &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: def })
        .map_err(|e| e.to_string())?;
    let h = client.set_get_eventhandle().map_err(|e| e.to_string())?;
    let cc = client.get_audiocaptureclient().map_err(|e| e.to_string())?;
    let mut wav = freemic::audio::wav::WavWriter::create(out, rate, 2).map_err(|e| e.to_string())?;
    let mut q = std::collections::VecDeque::new();
    client.start_stream().map_err(|e| e.to_string())?;
    let total = (secs * rate as f64) as usize * 8;
    let mut got = 0usize;
    while got < total {
        h.wait_for_event(1000).map_err(|e| e.to_string())?;
        cc.read_from_device_to_deque(&mut q).map_err(|e| e.to_string())?;
        let bytes: Vec<u8> = q.drain(..).collect();
        got += bytes.len();
        let samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        wav.write(&samples).map_err(|e| e.to_string())?;
    }
    client.stop_stream().ok();
    wav.finish().map_err(|e| e.to_string())?;
    println!("wrote {} ({} Hz)", out.display(), rate);
    Ok(())
}

/// Render a WAV file to an output device (WASAPI shared mode), e.g. into
/// "CABLE Input" to feed a known signal into the cable's microphone side.
#[cfg(windows)]
fn play(dev: &str, file: &PathBuf) -> Result<(), String> {
    use wasapi::*;
    let _ = initialize_mta();
    let en = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let coll = en.get_device_collection(&Direction::Render).map_err(|e| e.to_string())?;
    let mut found = None;
    for d in &coll {
        let d = d.map_err(|e| e.to_string())?;
        if d.get_friendlyname().unwrap_or_default().to_lowercase().contains(&dev.to_lowercase()) {
            found = Some(d);
            break;
        }
    }
    let d = found.ok_or(format!("no output device matching {dev}"))?;
    let name = d.get_friendlyname().unwrap_or_default();
    let mut client = d.get_iaudioclient().map_err(|e| e.to_string())?;
    let rate = client.get_mixformat().map_err(|e| e.to_string())?.get_samplespersec();
    let pcm = freemic::audio::decode::decode_all(file, rate)?; // interleaved stereo f32
    let fmt = WaveFormat::new(32, 32, &SampleType::Float, rate as usize, 2, None);
    let (def, _) = client.get_device_period().map_err(|e| e.to_string())?;
    client
        .initialize_client(&fmt, &Direction::Render, &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: def })
        .map_err(|e| e.to_string())?;
    let h = client.set_get_eventhandle().map_err(|e| e.to_string())?;
    let rc = client.get_audiorenderclient().map_err(|e| e.to_string())?;
    println!("playing {} to {name} ({rate} Hz)", file.display());
    let mut pos = 0usize; // in samples
    client.start_stream().map_err(|e| e.to_string())?;
    while pos < pcm.len() {
        h.wait_for_event(1000).map_err(|e| e.to_string())?;
        let avail = client.get_available_space_in_frames().map_err(|e| e.to_string())? as usize;
        let n = avail.min((pcm.len() - pos) / 2);
        if n == 0 {
            continue;
        }
        let bytes: Vec<u8> = pcm[pos..pos + n * 2].iter().flat_map(|s| s.to_le_bytes()).collect();
        rc.write_to_device(n, &bytes, None).map_err(|e| e.to_string())?;
        pos += n * 2;
    }
    std::thread::sleep(std::time::Duration::from_millis(100)); // let the buffer drain
    client.stop_stream().ok();
    Ok(())
}

#[cfg(not(windows))]
fn list() -> Result<(), String> {
    Err("use pactl/pw-cli on Linux".into())
}
#[cfg(not(windows))]
fn record(_: &str, _: f64, _: &PathBuf) -> Result<(), String> {
    Err("use parecord on Linux".into())
}
#[cfg(not(windows))]
fn play(_: &str, _: &PathBuf) -> Result<(), String> {
    Err("use paplay on Linux".into())
}
