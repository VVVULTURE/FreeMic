pub mod backend;
pub mod decode;
pub mod drift;
pub mod engine;
pub mod limiter;
pub mod mixer;
pub mod player;
pub mod wav;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Frames per soundboard block passed from the decoder thread to the mixer.
pub const BLOCK_FRAMES: usize = 256;

/// A chunk of decoded soundboard audio (interleaved stereo at the engine rate).
#[derive(Clone)]
pub struct Block {
    pub generation: u64,
    pub frames: usize,
    /// Frame index of the first frame within the sound (for progress display).
    pub position: u64,
    /// Last block of a Play-Once sound.
    pub end: bool,
    pub data: [f32; BLOCK_FRAMES * 2],
}

impl Block {
    pub fn empty(generation: u64) -> Self {
        Self { generation, frames: 0, position: 0, end: false, data: [0.0; BLOCK_FRAMES * 2] }
    }
}

#[derive(Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
    /// Raise the stored value to at least `v` (peak metering).
    pub fn fetch_max(&self, v: f32) {
        let mut cur = self.0.load(Ordering::Relaxed);
        while f32::from_bits(cur) < v {
            match self.0.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(x) => cur = x,
            }
        }
    }
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0f32.to_bits(), Ordering::Relaxed))
    }
}

/// Lock-free state shared between the UI, the decoder thread and the audio threads.
pub struct Shared {
    pub sound_gain: AtomicF32,
    pub mic_gain: AtomicF32,
    pub master_gain: AtomicF32,
    pub monitor_gain: AtomicF32,
    pub mic_muted: AtomicBool,
    /// Generation the user most recently requested (play or stop bumps it).
    pub target_gen: AtomicU64,
    /// Highest generation whose playback has completely finished.
    pub finished_gen: AtomicU64,
    /// Generation of the soundboard audio most recently sent to the output.
    pub audible_gen: AtomicU64,
    /// Highest generation the decoder has queued enough of to start without a gap.
    pub ready_gen: AtomicU64,
    pub position_frames: AtomicU64,
    pub out_rate: AtomicU32,
    pub mic_peak: AtomicF32,
    pub sound_peak: AtomicF32,
    pub out_peak: AtomicF32,
    /// Lowest limiter gain since the UI last looked (1.0 = not limiting).
    pub limiter_gain: AtomicF32,
    pub sound_underruns: AtomicU64,
    pub mic_underruns: AtomicU64,
    pub output_callbacks: AtomicU64,
    /// Microphone buffer level (frames) and drift correction (ppm), for display/diagnostics.
    pub mic_fill_frames: AtomicU32,
    pub mic_goal_frames: AtomicU32,
    pub mic_ratio_ppm: std::sync::atomic::AtomicI32,
    pub mic_skips: AtomicU64,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            sound_gain: AtomicF32::new(0.5),
            mic_gain: AtomicF32::new(1.0),
            master_gain: AtomicF32::new(1.0),
            monitor_gain: AtomicF32::new(0.5),
            mic_muted: AtomicBool::new(false),
            target_gen: AtomicU64::new(0),
            finished_gen: AtomicU64::new(0),
            audible_gen: AtomicU64::new(0),
            ready_gen: AtomicU64::new(0),
            position_frames: AtomicU64::new(0),
            out_rate: AtomicU32::new(48000),
            mic_peak: AtomicF32::new(0.0),
            sound_peak: AtomicF32::new(0.0),
            out_peak: AtomicF32::new(0.0),
            limiter_gain: AtomicF32::new(1.0),
            sound_underruns: AtomicU64::new(0),
            mic_underruns: AtomicU64::new(0),
            output_callbacks: AtomicU64::new(0),
            mic_fill_frames: AtomicU32::new(0),
            mic_goal_frames: AtomicU32::new(0),
            mic_ratio_ppm: std::sync::atomic::AtomicI32::new(0),
            mic_skips: AtomicU64::new(0),
        }
    }
}

/// Map a 0..=max percent slider to a linear gain with a perceptual (squared) taper.
pub fn percent_to_gain(pct: f32) -> f32 {
    let x = (pct / 100.0).max(0.0);
    x * x
}
