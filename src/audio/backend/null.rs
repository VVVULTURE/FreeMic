//! Device-free backend. Streams are paced by the system clock; the output can
//! be recorded to a WAV file (`FREEMIC_NULL_OUT`) and the "microphone" is a
//! synthetic tone or a WAV file (`FREEMIC_NULL_MIC`). Used for isolated tests:
//! nothing here can ever reach real speakers or another application.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rtrb::{Consumer, Producer};

use super::{DeviceInfo, RunCtl, Started, VirtualMicInfo};
use crate::audio::mixer::Mixer;
use crate::audio::wav::WavWriter;

const PERIOD_MS: u64 = 10;

fn rate() -> u32 {
    std::env::var("FREEMIC_NULL_RATE").ok().and_then(|v| v.parse().ok()).unwrap_or(48000)
}

pub fn list_inputs() -> Vec<DeviceInfo> {
    vec![DeviceInfo { id: "null-mic".into(), name: "Test tone (null backend)".into() }]
}

pub fn list_outputs() -> Vec<DeviceInfo> {
    vec![DeviceInfo { id: "null-out".into(), name: "Null output".into() }]
}

pub fn virtual_mic() -> VirtualMicInfo {
    VirtualMicInfo {
        feed: Some(list_outputs().remove(0)),
        mic_name: Some("FreeMic null sink (testing)".into()),
        problem: None,
    }
}

/// Sleep until `deadline`, returning false if asked to stop.
fn wait_until(deadline: Instant, ctl: &RunCtl) -> bool {
    loop {
        if ctl.should_stop() {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(20)));
    }
}

pub fn run_output(mixer: &mut Mixer, ctl: &RunCtl, started: &mut dyn FnMut(Started, &mut Mixer)) -> Result<(), String> {
    let rate = rate();
    let period = (rate as u64 * PERIOD_MS / 1000) as usize;
    started(Started { name: "Null output".into(), rate, period_frames: period }, mixer);
    let mut wav = match std::env::var_os("FREEMIC_NULL_OUT") {
        Some(p) => Some(WavWriter::create(&PathBuf::from(p), rate, 2).map_err(|e| e.to_string())?),
        None => None,
    };
    let mut buf = vec![0.0f32; period * 2];
    let t0 = Instant::now();
    let mut n: u64 = 0;
    while wait_until(t0 + Duration::from_millis(n * PERIOD_MS), ctl) {
        mixer.process(&mut buf);
        if let Some(w) = wav.as_mut() {
            w.write(&buf).map_err(|e| e.to_string())?;
        }
        n += 1;
    }
    if let Some(w) = wav {
        w.finish().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn run_capture(rate: u32, prod: &mut Producer<f32>, ctl: &RunCtl, started: &mut dyn FnMut(Started)) -> Result<(), String> {
    // Optional WAV "microphone"; otherwise a 220 Hz tone at -12 dBFS.
    let file: Option<Vec<f32>> = match std::env::var_os("FREEMIC_NULL_MIC") {
        Some(p) => {
            let st = crate::audio::decode::decode_all(&PathBuf::from(p), rate)?;
            Some(st.chunks_exact(2).map(|f| 0.5 * (f[0] + f[1])).collect())
        }
        None => None,
    };
    // Simulate a capture clock that runs 150 ppm fast, like real hardware would.
    let true_rate = rate as f64 * (1.0 + 150e-6);
    started(Started { name: "Test tone (null backend)".into(), rate, period_frames: (rate / 100) as usize });
    let t0 = Instant::now();
    let mut n: u64 = 0;
    let mut produced: u64 = 0;
    while wait_until(t0 + Duration::from_millis(n * PERIOD_MS), ctl) {
        n += 1;
        let due = (n as f64 * PERIOD_MS as f64 / 1000.0 * true_rate) as u64;
        while produced < due {
            let s = match &file {
                Some(v) if !v.is_empty() => v[(produced as usize) % v.len()],
                _ => 0.25 * (2.0 * std::f64::consts::PI * 220.0 * produced as f64 / true_rate).sin() as f32,
            };
            let _ = prod.push(s);
            produced += 1;
        }
    }
    Ok(())
}

pub fn run_monitor(rate: u32, cons: &mut Consumer<f32>, ctl: &RunCtl, started: &mut dyn FnMut(Started)) -> Result<(), String> {
    started(Started { name: "Null monitor".into(), rate, period_frames: (rate / 100) as usize });
    let t0 = Instant::now();
    let mut n: u64 = 0;
    while wait_until(t0 + Duration::from_millis(n * PERIOD_MS), ctl) {
        n += 1;
        let k = cons.slots();
        if let Ok(c) = cons.read_chunk(k) {
            c.commit_all();
        }
    }
    Ok(())
}
