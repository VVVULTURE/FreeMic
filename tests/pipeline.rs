//! Offline tests of the full soundboard pipeline: decoder thread -> mixer,
//! with a simulated microphone. No audio devices are used.

mod common;
use common::*;

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use freemic::audio::drift::DriftResampler;
use freemic::audio::mixer::{self, MicFeed, Mixer, MixerMsg};
use freemic::audio::player::{Player, PlayerCmd};
use freemic::audio::Shared;

const RATE: u32 = 48000;
const PERIOD: usize = 480;

struct Rig {
    shared: Arc<Shared>,
    mixer: Mixer,
    player_tx: mpsc::Sender<PlayerCmd>,
    mixer_tx: mpsc::Sender<MixerMsg>,
    errors: Arc<Mutex<Vec<String>>>,
    mic: Option<(rtrb::Producer<f32>, f64, u64)>, // producer, drifted rate, samples produced
    mic_freq: f64,
    mic_amp: f32,
    periods: u64,
}

impl Rig {
    fn new() -> Self {
        let shared = Arc::new(Shared::default());
        shared.out_rate.store(RATE, Ordering::Relaxed);
        shared.sound_gain.store(1.0);
        let (stx, srx) = mixer::sound_queue();
        let (mtx, mrx) = mpsc::channel();
        let (ptx, prx) = mpsc::channel();
        let errors = Arc::new(Mutex::new(Vec::new()));
        let e2 = errors.clone();
        let player = Player::new(shared.clone(), prx, stx, Arc::new(move |m| e2.lock().unwrap().push(m)));
        std::thread::spawn(move || player.run());
        let mixer = Mixer::new(shared.clone(), srx, mrx, RATE);
        Rig { shared, mixer, player_tx: ptx, mixer_tx: mtx, errors, mic: None, mic_freq: 220.0, mic_amp: 0.3, periods: 0 }
    }

    fn attach_mic(&mut self, ppm: f64, freq: f64, amp: f32) {
        let (prod, cons) = rtrb::RingBuffer::new(RATE as usize);
        let rs = DriftResampler::new(1, RATE, RATE, PERIOD + RATE as usize / 100);
        self.mixer_tx.send(MixerMsg::Mic(Some(MicFeed { consumer: cons, resampler: rs }))).unwrap();
        self.mic = Some((prod, RATE as f64 * (1.0 + ppm * 1e-6), 0));
        self.mic_freq = freq;
        self.mic_amp = amp;
    }

    fn play(&self, name: &str, looped: bool) -> u64 {
        let g = self.shared.target_gen.fetch_add(1, Ordering::AcqRel) + 1;
        self.player_tx.send(PlayerCmd::Play { generation: g, path: fixture(name), looped }).unwrap();
        g
    }

    fn stop(&self) {
        let g = self.shared.target_gen.fetch_add(1, Ordering::AcqRel) + 1;
        self.shared.finished_gen.fetch_max(g, Ordering::AcqRel);
    }

    /// Run `secs` of output, returning interleaved stereo.
    fn run(&mut self, secs: f64) -> Vec<f32> {
        let n = (secs * RATE as f64 / PERIOD as f64).round() as usize;
        let mut out = Vec::with_capacity(n * PERIOD * 2);
        let mut buf = vec![0.0f32; PERIOD * 2];
        for _ in 0..n {
            if let Some((prod, rate, produced)) = self.mic.as_mut() {
                let due = ((self.periods + 1) as f64 * PERIOD as f64 / RATE as f64 * *rate) as u64;
                while *produced < due {
                    let s = self.mic_amp * (2.0 * std::f64::consts::PI * self.mic_freq * *produced as f64 / *rate).sin() as f32;
                    let _ = prod.push(s);
                    *produced += 1;
                }
            }
            self.mixer.process(&mut buf);
            out.extend_from_slice(&buf);
            self.periods += 1;
            assert_eq!(self.shared.sound_underruns.load(Ordering::Relaxed), 0, "soundboard underrun (gap in a sound)");
            // ~5x faster than real time: plenty for the decoder, still exercises scheduling.
            std::thread::sleep(Duration::from_micros(2000));
        }
        out
    }
}

fn window(out: &[f32], from: f64, to: f64) -> Vec<f32> {
    let a = (from * RATE as f64) as usize * 2;
    let b = ((to * RATE as f64) as usize * 2).min(out.len());
    out[a..b].to_vec()
}

#[test]
fn play_once_plays_whole_file_then_stops() {
    let mut rig = Rig::new();
    let g = rig.play("stereo_s16_44k.wav", false);
    let out = rig.run(1.5);
    let l = channel(&window(&out, 0.1, 0.9), 0);
    let r = channel(&window(&out, 0.1, 0.9), 1);
    // Output passes through the limiter (threshold -1 dBFS); 0.5 amplitude is below it.
    assert!((tone_amplitude(&l, 440.0, RATE as f64) - 0.5).abs() < 0.02);
    assert!((tone_amplitude(&r, 660.0, RATE as f64) - 0.5).abs() < 0.02);
    let tail = window(&out, 1.1, 1.5);
    assert!(peak(&tail) < 1e-4, "silence after the end, got {}", peak(&tail));
    assert!(rig.shared.finished_gen.load(Ordering::Acquire) >= g, "finished flag set");
    // Total sounding length ~= 1.0 s.
    let sounding = out.chunks_exact(2 * 480).filter(|c| rms(c) > 0.05).count() as f64 * 0.01;
    assert!((sounding - 1.0).abs() < 0.03, "sounding {sounding}");
    assert_eq!(rig.shared.sound_underruns.load(Ordering::Relaxed), 0);
}

#[test]
fn loop_repeats_seamlessly_until_stopped() {
    let mut rig = Rig::new();
    let g = rig.play("stereo_f32_48k.wav", true);
    let out = rig.run(3.5);
    assert!(rig.shared.finished_gen.load(Ordering::Acquire) < g, "loop must not finish");
    // Every 10 ms window across loop boundaries carries the tone (no gaps).
    let body = window(&out, 0.05, 3.45);
    for (i, c) in body.chunks_exact(2 * 480).enumerate() {
        assert!(rms(c) > 0.3, "gap in window {i} (t={:.2}s)", 0.05 + i as f64 * 0.01);
    }
    // Boundary at 1.0/2.0/3.0 s: the fixture is an integer number of cycles of 440 Hz,
    // so the waveform is continuous; check there is no click (sample-to-sample jump).
    let l = channel(&body, 0);
    let max_step = l.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let expected = 0.5 * 2.0 * std::f32::consts::PI * 440.0 / RATE as f32;
    assert!(max_step < expected * 1.3, "click at loop boundary: step {max_step} vs {expected}");
    rig.stop();
    let after = rig.run(0.3);
    assert!(peak(&window(&after, 0.02, 0.3)) < 1e-4, "silent after stop");
    assert!(peak(&window(&after, 0.0, 0.01)) <= 0.5 + 1e-3, "fade only");
}

#[test]
fn new_sound_replaces_old_one_immediately() {
    let mut rig = Rig::new();
    rig.play("stereo_f32_48k.wav", true); // 440/660 loop
    let first = rig.run(0.5);
    assert!(tone_amplitude(&channel(&window(&first, 0.1, 0.5), 0), 440.0, RATE as f64) > 0.4);
    rig.play("long_20s.mp3", false); // 1 kHz
    let second = rig.run(1.0);
    // From one period after the switch, only the new sound may be audible.
    let l = channel(&window(&second, 0.02, 1.0), 0);
    let old = tone_amplitude(&l, 440.0, RATE as f64);
    let new = tone_amplitude(&l, 1000.0, RATE as f64);
    assert!(old < 0.002, "old sound still audible: {old}");
    assert!(new > 0.4, "new sound missing: {new}");
    // Switching must not glitch: no sample above the old/new peaks.
    assert!(peak(&window(&second, 0.0, 0.02)) <= 0.9);
}

#[test]
fn mic_and_sound_are_mixed_with_independent_gains() {
    let mut rig = Rig::new();
    rig.attach_mic(250.0, 220.0, 0.3);
    rig.shared.mic_gain.store(1.0);
    rig.shared.sound_gain.store(0.5);
    rig.play("long_20s.mp3", false);
    let a = rig.run(2.0);
    let wa = channel(&window(&a, 1.0, 2.0), 0);
    let mic_a = tone_amplitude(&wa, 220.0, RATE as f64);
    let snd_a = tone_amplitude(&wa, 1000.0, RATE as f64);
    assert!((mic_a - 0.3).abs() < 0.02, "mic {mic_a}");
    assert!((snd_a - 0.25).abs() < 0.02, "sound {snd_a}");

    // Change only the soundboard volume: the mic level must not move.
    rig.shared.sound_gain.store(0.1);
    let b = rig.run(1.0);
    let wb = channel(&window(&b, 0.1, 1.0), 0);
    let (mic_b, snd_b) = (tone_amplitude(&wb, 220.0, RATE as f64), tone_amplitude(&wb, 1000.0, RATE as f64));
    assert!((mic_b - mic_a).abs() < 0.01, "mic changed with sound gain: {mic_a} -> {mic_b}");
    assert!((snd_b - 0.05).abs() < 0.01, "sound {snd_b}");

    // Change only the mic volume: the sound level must not move.
    rig.shared.mic_gain.store(0.5);
    let c = rig.run(1.0);
    let wc = channel(&window(&c, 0.1, 1.0), 0);
    let (mic_c, snd_c) = (tone_amplitude(&wc, 220.0, RATE as f64), tone_amplitude(&wc, 1000.0, RATE as f64));
    assert!((mic_c - 0.15).abs() < 0.01, "mic {mic_c}");
    assert!((snd_c - snd_b).abs() < 0.01, "sound changed with mic gain: {snd_b} -> {snd_c}");

    // The microphone keeps flowing while nothing plays.
    rig.stop();
    let d = rig.run(1.0);
    let wd = channel(&window(&d, 0.1, 1.0), 0);
    assert!((tone_amplitude(&wd, 220.0, RATE as f64) - 0.15).abs() < 0.01);
    assert!(tone_amplitude(&wd, 1000.0, RATE as f64) < 0.002);
    assert_eq!(rig.shared.mic_underruns.load(Ordering::Relaxed), 0, "mic underruns");
}

#[test]
fn loud_mic_plus_loud_sound_never_clips() {
    let mut rig = Rig::new();
    rig.attach_mic(-400.0, 300.0, 0.95);
    rig.shared.mic_gain.store(2.0); // +6 dB boost on an already hot mic
    rig.shared.sound_gain.store(1.0);
    rig.play("stereo_f32_48k.wav", true);
    let out = rig.run(2.0);
    assert!(peak(&out) <= 1.0, "peak {}", peak(&out));
    assert!(rig.shared.limiter_gain.load() < 0.5, "limiter engaged");
    // Content is still there (limited, not muted).
    let l = channel(&window(&out, 1.0, 2.0), 0);
    assert!(tone_amplitude(&l, 300.0, RATE as f64) > 0.2);
    assert!(tone_amplitude(&l, 440.0, RATE as f64) > 0.05);
}

#[test]
fn unplayable_file_reports_error_and_finishes() {
    let mut rig = Rig::new();
    let g = rig.play("not_audio.mp3", false);
    rig.run(0.1);
    assert!(rig.shared.finished_gen.load(Ordering::Acquire) >= g);
    let errs = rig.errors.lock().unwrap().clone();
    assert_eq!(errs.len(), 1, "{errs:?}");
    // A good file still plays afterwards.
    rig.play("stereo_s16_44k.wav", false);
    let out = rig.run(0.5);
    assert!(tone_amplitude(&channel(&window(&out, 0.1, 0.5), 0), 440.0, RATE as f64) > 0.4);
}

/// Regression: replacing a loop whose look-ahead queue is full of old blocks
/// used to mark the new sound "ready" before any of it was queued, so it
/// started, ran dry after two blocks and left a gap.
#[test]
fn replacement_is_not_ready_until_its_own_blocks_are_queued() {
    let shared = Arc::new(Shared::default());
    shared.out_rate.store(RATE, Ordering::Relaxed);
    let (mut tx, mut rx) = mixer::sound_queue();
    while tx.push(freemic::audio::Block::empty(1)).is_ok() {} // full of the old sound
    shared.target_gen.store(2, Ordering::Release);
    let (ptx, prx) = mpsc::channel();
    let player = Player::new(shared.clone(), prx, tx, Arc::new(|_| {}));
    let handle = std::thread::spawn(move || player.run());
    ptx.send(PlayerCmd::Play { generation: 2, path: fixture("long_20s.mp3"), looped: false }).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(shared.ready_gen.load(Ordering::Acquire) < 2, "ready while the queue holds only old blocks");

    // The mixer drops the stale blocks; the new sound must then queue its start blocks.
    while rx.pop().is_ok() {}
    let t0 = std::time::Instant::now();
    while shared.ready_gen.load(Ordering::Acquire) < 2 {
        assert!(t0.elapsed() < Duration::from_secs(2), "never became ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    let queued = rx.slots();
    assert!(queued >= freemic::audio::player::START_BLOCKS as usize, "ready with only {queued} blocks");
    while let Ok(b) = rx.pop() {
        assert_eq!(b.generation, 2);
    }
    ptx.send(PlayerCmd::Shutdown).unwrap();
    // The player may be parked on a full queue; keep draining so it sees the command.
    while !handle.is_finished() {
        while rx.pop().is_ok() {}
        std::thread::sleep(Duration::from_millis(5));
    }
}
