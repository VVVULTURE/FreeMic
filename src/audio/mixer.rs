//! The real-time mix: physical microphone + soundboard -> virtual microphone.
//!
//! `Mixer::process` runs on the output device's audio thread. It never
//! allocates or locks: it reads the microphone from a ring buffer through the
//! drift-compensating resampler, reads soundboard blocks from a second ring,
//! applies the independent gains, and limits the sum so it cannot clip.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;

use rtrb::{Consumer, Producer};

use super::drift::DriftResampler;
use super::limiter::Limiter;
use super::{Block, Shared, BLOCK_FRAMES};

/// Length of the fade applied when a sound is stopped or replaced (~2.7 ms at 48 kHz).
const FADE_FRAMES: usize = 128;
const MAX_PERIOD: usize = 16384;

pub struct MicFeed {
    pub consumer: Consumer<f32>,
    pub resampler: DriftResampler,
}

pub enum MixerMsg {
    /// Attach a new microphone feed (or detach with `None`).
    Mic(Option<MicFeed>),
    /// Attach a monitor output (soundboard copy for the user's headphones).
    Monitor(Option<Producer<f32>>),
}

pub struct Mixer {
    shared: Arc<Shared>,
    sounds: Consumer<Block>,
    ctl: Receiver<MixerMsg>,
    mic: Option<MicFeed>,
    monitor: Option<Producer<f32>>,
    rate: u32,
    limiter: Limiter,
    cur: Option<(Block, usize)>,
    last_target: u64,
    fade: Vec<f32>,
    fade_len: usize,
    fade_pos: usize,
    mic_buf: Vec<f32>,
    sound_buf: Vec<f32>,
    g_sound: f32,
    g_mic: f32,
    g_master: f32,
}

impl Mixer {
    pub fn new(shared: Arc<Shared>, sounds: Consumer<Block>, ctl: Receiver<MixerMsg>, rate: u32) -> Self {
        let last_target = shared.target_gen.load(Ordering::Acquire);
        Self {
            g_sound: shared.sound_gain.load(),
            g_mic: shared.mic_gain.load(),
            g_master: shared.master_gain.load(),
            shared,
            sounds,
            ctl,
            mic: None,
            monitor: None,
            rate,
            limiter: Limiter::new(rate),
            cur: None,
            last_target,
            fade: vec![0.0; FADE_FRAMES * 2],
            fade_len: 0,
            fade_pos: 0,
            mic_buf: vec![0.0; MAX_PERIOD],
            sound_buf: vec![0.0; MAX_PERIOD * 2],
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Called (off the real-time path) when the output device is reopened.
    pub fn set_rate(&mut self, rate: u32) {
        if rate != self.rate {
            self.rate = rate;
            self.limiter = Limiter::new(rate);
            // The mic feed was built for the old rate; the capture thread will send a new one.
            self.mic = None;
        }
    }

    /// Detach the current mic feed (used when the output is reopened).
    pub fn drop_mic(&mut self) {
        self.mic = None;
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.ctl.try_recv() {
            match msg {
                MixerMsg::Mic(m) => self.mic = m,
                MixerMsg::Monitor(p) => self.monitor = p,
            }
        }
    }

    /// Collect a short fade-out tail from the sound that is being replaced or stopped.
    fn begin_fade(&mut self, target: u64) {
        let mut n = 0;
        if let Some((block, pos)) = self.cur.take() {
            if block.generation < target {
                let avail = (block.frames - pos).min(FADE_FRAMES);
                self.fade[..avail * 2].copy_from_slice(&block.data[pos * 2..(pos + avail) * 2]);
                n = avail;
            } else {
                self.cur = Some((block, pos));
            }
        }
        // Continue the tail from queued blocks of the old sound, discarding the rest.
        while let Ok(peek) = self.sounds.peek() {
            if peek.generation >= target {
                break;
            }
            let block = self.sounds.pop().unwrap();
            if n < FADE_FRAMES {
                let take = block.frames.min(FADE_FRAMES - n);
                self.fade[n * 2..(n + take) * 2].copy_from_slice(&block.data[..take * 2]);
                n += take;
            }
        }
        self.fade_len = n;
        self.fade_pos = 0;
    }

    fn fill_sound(&mut self, frames: usize) {
        let target = self.shared.target_gen.load(Ordering::Acquire);
        if target != self.last_target {
            self.begin_fade(target);
            self.last_target = target;
        }
        let buf = &mut self.sound_buf[..frames * 2];
        buf.fill(0.0);

        let mut i = 0;
        let mut starved = false;
        while i < frames {
            if self.cur.is_none() {
                // A new sound starts only once enough of it is queued to play without a gap.
                if let Ok(head) = self.sounds.peek() {
                    if head.generation >= target && self.shared.ready_gen.load(Ordering::Acquire) < head.generation {
                        break;
                    }
                }
                match self.sounds.pop() {
                    Ok(b) if b.generation < target => continue, // stale block from a replaced sound
                    Ok(b) => self.cur = Some((b, 0)),
                    Err(_) => {
                        starved = self.shared.finished_gen.load(Ordering::Acquire) < target
                            && self.shared.audible_gen.load(Ordering::Relaxed) == target;
                        break;
                    }
                }
            }
            let (block, pos) = self.cur.as_mut().unwrap();
            let n = (block.frames - *pos).min(frames - i);
            buf[i * 2..(i + n) * 2].copy_from_slice(&block.data[*pos * 2..(*pos + n) * 2]);
            *pos += n;
            i += n;
            self.shared.audible_gen.store(block.generation, Ordering::Relaxed);
            self.shared.position_frames.store(block.position + *pos as u64, Ordering::Relaxed);
            if *pos >= block.frames {
                if block.end {
                    self.shared.finished_gen.fetch_max(block.generation, Ordering::AcqRel);
                }
                self.cur = None;
            }
        }
        if starved {
            self.shared.sound_underruns.fetch_add(1, Ordering::Relaxed);
        }

        // Overlay the fade-out tail of a replaced sound.
        while self.fade_pos < self.fade_len && self.fade_pos < frames {
            let g = 1.0 - (self.fade_pos + 1) as f32 / (self.fade_len + 1) as f32;
            let k = self.fade_pos;
            buf[k * 2] += self.fade[k * 2] * g;
            buf[k * 2 + 1] += self.fade[k * 2 + 1] * g;
            self.fade_pos += 1;
        }
        self.fade_len = 0; // a fade only spans the first period after a switch
    }

    /// Produce `out.len() / 2` stereo frames of mixed audio.
    pub fn process(&mut self, out: &mut [f32]) {
        let frames = (out.len() / 2).min(MAX_PERIOD);
        let out = &mut out[..frames * 2];
        self.handle_messages();
        self.shared.output_callbacks.fetch_add(1, Ordering::Relaxed);

        // Microphone (mono).
        let mic = &mut self.mic_buf[..frames];
        match self.mic.as_mut() {
            Some(feed) => {
                let before = feed.resampler.underruns;
                let fill = (feed.consumer.slots()) as u32;
                feed.resampler.process(&mut feed.consumer, mic);
                if feed.resampler.underruns != before {
                    self.shared.mic_underruns.fetch_add(1, Ordering::Relaxed);
                }
                self.shared.mic_fill_frames.store(fill, Ordering::Relaxed);
                self.shared.mic_goal_frames.store(feed.resampler.goal_frames() as u32, Ordering::Relaxed);
                self.shared.mic_ratio_ppm.store(((feed.resampler.current_ratio() - 1.0) * 1e6) as i32, Ordering::Relaxed);
                self.shared.mic_skips.store(feed.resampler.skips, Ordering::Relaxed);
            }
            None => mic.fill(0.0),
        }
        self.fill_sound(frames);

        // Gains, ramped across the period to avoid zipper noise.
        let muted = self.shared.mic_muted.load(Ordering::Relaxed);
        let t_mic = if muted { 0.0 } else { self.shared.mic_gain.load() };
        let t_sound = self.shared.sound_gain.load();
        let t_master = self.shared.master_gain.load();
        let inv = 1.0 / frames.max(1) as f32;
        let (d_mic, d_sound, d_master) =
            ((t_mic - self.g_mic) * inv, (t_sound - self.g_sound) * inv, (t_master - self.g_master) * inv);

        let mut mic_peak = 0.0f32;
        let mut sound_peak = 0.0f32;
        for f in 0..frames {
            self.g_mic += d_mic;
            self.g_sound += d_sound;
            self.g_master += d_master;
            let m = self.mic_buf[f] * self.g_mic;
            let l = self.sound_buf[f * 2] * self.g_sound;
            let r = self.sound_buf[f * 2 + 1] * self.g_sound;
            mic_peak = mic_peak.max(m.abs());
            sound_peak = sound_peak.max(l.abs()).max(r.abs());
            out[f * 2] = (m + l) * self.g_master;
            out[f * 2 + 1] = (m + r) * self.g_master;
        }
        self.g_mic = t_mic;
        self.g_sound = t_sound;
        self.g_master = t_master;

        // Monitor gets the soundboard only (never the mic, so no self-echo or feedback).
        if let Some(mon) = self.monitor.as_mut() {
            let g = self.shared.monitor_gain.load() * self.g_sound;
            let n = mon.slots().min(frames * 2) & !1;
            if let Ok(mut chunk) = mon.write_chunk(n) {
                let (a, b) = chunk.as_mut_slices();
                for (i, s) in a.iter_mut().chain(b.iter_mut()).enumerate() {
                    *s = (self.sound_buf[i] * g).clamp(-1.0, 1.0);
                }
                chunk.commit_all();
            }
        }

        self.limiter.process(out);
        let out_peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        self.shared.mic_peak.fetch_max(mic_peak);
        self.shared.sound_peak.fetch_max(sound_peak);
        self.shared.out_peak.fetch_max(out_peak);
        let lg = self.limiter.take_min_gain();
        if lg < 1.0 {
            let cur = self.shared.limiter_gain.load();
            if lg < cur {
                self.shared.limiter_gain.store(lg);
            }
        }
    }

    pub fn latency_frames(&self) -> usize {
        self.limiter.latency_frames() + self.mic.as_ref().map(|m| m.resampler.target_frames()).unwrap_or(0)
    }
}

/// Number of blocks of look-ahead the decoder thread keeps queued (~340 ms at 48 kHz).
pub const SOUND_QUEUE_BLOCKS: usize = 64;

pub fn sound_queue() -> (Producer<Block>, Consumer<Block>) {
    rtrb::RingBuffer::new(SOUND_QUEUE_BLOCKS)
}

#[allow(dead_code)]
const _: () = assert!(BLOCK_FRAMES <= MAX_PERIOD);

#[cfg(test)]
mod tests {
    use super::*;

    fn block(generation: u64, v: f32) -> Block {
        let mut b = Block::empty(generation);
        b.frames = BLOCK_FRAMES;
        b.data.fill(v);
        b
    }

    /// Regression: a sound whose first block arrived alone used to start, run
    /// dry inside the same period and leave a gap right after its first 256 frames.
    #[test]
    fn new_sound_waits_until_enough_is_queued() {
        let shared = Arc::new(Shared::default());
        shared.sound_gain.store(1.0);
        let (mut tx, rx) = sound_queue();
        let (_ctl_tx, ctl_rx) = std::sync::mpsc::channel();
        let mut mixer = Mixer::new(shared.clone(), rx, ctl_rx, 48000);
        let mut out = vec![0.0f32; 480 * 2];

        shared.target_gen.store(1, Ordering::Release);
        tx.push(block(1, 0.5)).unwrap();
        mixer.process(&mut out);
        assert!(out.iter().all(|s| *s == 0.0), "must not start with one block queued");

        for _ in 0..3 {
            tx.push(block(1, 0.5)).unwrap();
        }
        shared.ready_gen.store(1, Ordering::Release);
        mixer.process(&mut out);
        let lim = mixer.limiter.latency_frames();
        // After the limiter delay the whole period carries the sound, with no hole.
        assert!(out[lim * 2..].iter().all(|s| (*s - 0.5).abs() < 1e-3), "gap inside the first period");
        assert_eq!(shared.sound_underruns.load(Ordering::Relaxed), 0);
    }
}
