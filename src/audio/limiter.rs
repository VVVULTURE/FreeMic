//! Stereo-linked look-ahead peak limiter.
//!
//! The limiter sits at the very end of the mix so that microphone + soundboard
//! never exceed full scale. It delays the signal by `lookahead` frames and
//! lowers the gain *before* a peak reaches the output, so peaks are tamed
//! without hard clipping. A final clamp guarantees the output never leaves
//! [-1, 1] even in pathological cases.

use std::collections::VecDeque;

pub struct Limiter {
    threshold: f32,
    lookahead: usize,
    /// Delay line of interleaved stereo frames.
    delay: VecDeque<[f32; 2]>,
    /// Monotonic deque (index, required gain) for a sliding-window minimum.
    window: VecDeque<(u64, f32)>,
    index: u64,
    gain: f32,
    attack: f32,
    release: f32,
    /// Lowest gain applied since the last call to `take_min_gain` (for metering/tests).
    min_gain: f32,
}

impl Limiter {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8000) as f32;
        let lookahead = ((sr * 0.0015) as usize).max(8); // 1.5 ms
        let mut delay = VecDeque::with_capacity(lookahead + 1);
        for _ in 0..lookahead {
            delay.push_back([0.0, 0.0]);
        }
        Self {
            threshold: 0.891, // -1 dBFS
            lookahead,
            delay,
            window: VecDeque::with_capacity(lookahead + 2),
            index: 0,
            gain: 1.0,
            // Attack must (almost) reach the target within the look-ahead window.
            attack: 1.0 - (-5.0 / lookahead as f32).exp(),
            release: 1.0 - (-1.0 / (sr * 0.080)).exp(), // 80 ms
            min_gain: 1.0,
        }
    }

    pub fn latency_frames(&self) -> usize {
        self.lookahead
    }

    /// Process interleaved stereo samples in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for frame in buf.chunks_exact_mut(2) {
            let peak = frame[0].abs().max(frame[1].abs());
            let required = if peak > self.threshold { self.threshold / peak } else { 1.0 };

            // Sliding-window minimum over the last `lookahead + 1` required gains.
            while let Some(&(_, g)) = self.window.back() {
                if g >= required {
                    self.window.pop_back();
                } else {
                    break;
                }
            }
            self.window.push_back((self.index, required));
            let oldest = self.index.saturating_sub(self.lookahead as u64);
            while let Some(&(i, _)) = self.window.front() {
                if i < oldest {
                    self.window.pop_front();
                } else {
                    break;
                }
            }
            self.index += 1;
            let target = self.window.front().map(|w| w.1).unwrap_or(1.0);

            if target < self.gain {
                self.gain += (target - self.gain) * self.attack;
            } else {
                self.gain += (target - self.gain) * self.release;
            }
            if self.gain < self.min_gain {
                self.min_gain = self.gain;
            }

            self.delay.push_back([frame[0], frame[1]]);
            let out = self.delay.pop_front().unwrap_or([0.0, 0.0]);
            frame[0] = (out[0] * self.gain).clamp(-1.0, 1.0);
            frame[1] = (out[1] * self.gain).clamp(-1.0, 1.0);
        }
    }

    /// Returns the lowest gain applied since the previous call and resets it.
    pub fn take_min_gain(&mut self) -> f32 {
        std::mem::replace(&mut self.min_gain, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amp: f32, freq: f32, sr: u32, frames: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / sr as f32).sin();
            v.push(s);
            v.push(s);
        }
        v
    }

    #[test]
    fn quiet_signal_passes_unchanged_after_delay() {
        let mut lim = Limiter::new(48000);
        let input = sine(0.5, 440.0, 48000, 4800);
        let mut buf = input.clone();
        lim.process(&mut buf);
        let d = lim.latency_frames() * 2;
        for i in 0..(input.len() - d) {
            assert!((buf[i + d] - input[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn loud_signal_never_exceeds_full_scale() {
        let mut lim = Limiter::new(48000);
        let mut buf = sine(2.0, 1000.0, 48000, 48000); // +6 dBFS
        lim.process(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0);
        // After settling, the limiter should hold close to the threshold rather than hard clip.
        let settled = &buf[24000..];
        let settled_peak = settled.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(settled_peak < 0.95, "settled peak {settled_peak}");
        assert!(settled_peak > 0.80, "settled peak {settled_peak}");
    }

    #[test]
    fn transient_is_caught_by_lookahead() {
        let mut lim = Limiter::new(48000);
        let mut buf = vec![0.0f32; 2000];
        buf[1000] = 1.8;
        buf[1001] = -1.8;
        lim.process(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 0.95, "peak {peak}");
    }
}
