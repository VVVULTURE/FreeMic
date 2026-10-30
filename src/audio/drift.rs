//! Drift-compensating resampler.
//!
//! Two audio devices never share a clock: a USB microphone running at
//! "48 kHz" and a virtual cable running at "48 kHz" differ by a few hundred
//! ppm. Without compensation the buffer between them slowly fills up (adding
//! latency) or runs dry (clicks). This resampler reads from a ring buffer that
//! the capture thread fills and adjusts its read speed with a gentle PI
//! controller so the buffer stays near its goal. Rate corrections are limited
//! to +/-0.4 % (a few cents, inaudible on voice). Large one-off offsets (e.g.
//! a big first device request or a stall) are removed with a ~1 ms
//! duck-skip-unduck instead of seconds of audible pitch slewing.

use rtrb::Consumer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Prebuffering,
    /// Just started: run at the nominal rate for a moment and measure the
    /// real average buffer level (it depends on the phase between the
    /// capture and output periods), then correct the offset in one step.
    Settling,
    Running,
}

pub struct DriftResampler {
    channels: usize,
    in_rate: f64,
    nominal: f64,
    /// Safety margin kept in the buffer on top of one output period.
    target: usize,
    /// Smoothed output period (frames per call), so the goal does not jump
    /// when a device occasionally asks for a bigger chunk.
    period_ema: f64,
    /// Four frames of history for cubic interpolation: x[-1], x[0], x[1], x[2].
    hist: Vec<f32>,
    frac: f64,
    state: State,
    filt_fill: f64,
    integ: f64,
    high_for: f64,
    /// Duck envelope used to hide a skip: gain, direction and frames to drop at the bottom.
    duck_gain: f32,
    duck_down: bool,
    pending_skip: usize,
    pending_hold: usize,
    hold_left: usize,
    settle_time: f64,
    settle_sum: f64,
    settle_n: u32,
    pub underruns: u64,
    pub overruns: u64,
    pub skips: u64,
    last_step: f64,
}

const MAX_ADJ: f64 = 0.004;
const DUCK_FRAMES: f32 = 48.0;

impl DriftResampler {
    pub fn new(channels: usize, in_rate: u32, out_rate: u32, target_frames: usize) -> Self {
        Self {
            channels,
            in_rate: in_rate as f64,
            nominal: in_rate as f64 / out_rate as f64,
            target: target_frames.max(16),
            period_ema: 0.0,
            hist: vec![0.0; channels * 4],
            frac: 0.0,
            state: State::Prebuffering,
            filt_fill: target_frames as f64,
            integ: 0.0,
            high_for: 0.0,
            duck_gain: 1.0,
            duck_down: false,
            pending_skip: 0,
            pending_hold: 0,
            hold_left: 0,
            settle_time: 0.0,
            settle_sum: 0.0,
            settle_n: 0,
            underruns: 0,
            overruns: 0,
            skips: 0,
            last_step: in_rate as f64 / out_rate as f64,
        }
    }

    pub fn target_frames(&self) -> usize {
        self.target
    }

    /// Buffer level the controller aims for (frames at the input rate).
    pub fn goal_frames(&self) -> usize {
        self.target + (self.period_ema * self.nominal).round() as usize
    }

    pub fn current_ratio(&self) -> f64 {
        self.last_step / self.nominal
    }

    /// True while the input is flowing (not waiting for the buffer to fill).
    pub fn is_running(&self) -> bool {
        self.state != State::Prebuffering
    }

    fn push_frame(&mut self, cons: &mut Consumer<f32>) -> bool {
        let ch = self.channels;
        if cons.slots() < ch {
            return false;
        }
        self.hist.copy_within(ch.., 0);
        let base = 3 * ch;
        if let Ok(chunk) = cons.read_chunk(ch) {
            let (a, b) = chunk.as_slices();
            for (i, s) in a.iter().chain(b.iter()).enumerate() {
                self.hist[base + i] = *s;
            }
            chunk.commit_all();
        }
        true
    }

    fn drop_frames(&mut self, cons: &mut Consumer<f32>, frames: usize) {
        let n = (frames * self.channels).min(cons.slots() / self.channels * self.channels);
        if let Ok(c) = cons.read_chunk(n) {
            c.commit_all();
        }
    }

    /// Fill `out` (interleaved, `channels` wide) with resampled input.
    pub fn process(&mut self, cons: &mut Consumer<f32>, out: &mut [f32]) {
        let ch = self.channels;
        let frames_out = out.len() / ch;
        if frames_out == 0 {
            return;
        }
        self.period_ema = if self.period_ema == 0.0 {
            frames_out as f64
        } else {
            self.period_ema + (frames_out as f64 - self.period_ema) * 0.02
        };
        // Never start (or keep running) with less than this call needs.
        let goal = self.goal_frames().max(self.target + (frames_out as f64 * self.nominal).ceil() as usize);
        let fill = cons.slots() / ch;
        let dt = frames_out as f64 / (self.in_rate / self.nominal);

        match self.state {
            State::Prebuffering => {
                if fill < goal {
                    out.fill(0.0);
                    return;
                }
                self.state = State::Settling;
                self.settle_time = 0.0;
                self.settle_sum = 0.0;
                self.settle_n = 0;
                // Drop anything above the goal so we start at the desired latency.
                self.drop_frames(cons, fill - goal);
                self.hist.fill(0.0);
                for _ in 0..3 {
                    self.push_frame(cons);
                }
                self.frac = 0.0;
                self.filt_fill = goal as f64;
                self.integ = 0.0;
                self.high_for = 0.0;
                self.duck_gain = 0.0; // fade in from silence
                self.duck_down = false;
                self.pending_skip = 0;
                self.pending_hold = 0;
                self.hold_left = 0;
            }
            State::Settling => {
                self.settle_time += dt;
                self.settle_sum += fill as f64;
                self.settle_n += 1;
                if self.settle_time >= 0.3 {
                    let err = self.settle_sum / self.settle_n as f64 - goal as f64;
                    let small = self.in_rate * 0.001;
                    if err > small {
                        self.pending_skip = err as usize;
                        self.duck_down = true;
                    } else if err < -small {
                        self.pending_hold = (-err) as usize;
                        self.duck_down = true;
                    }
                    self.state = State::Running;
                    self.filt_fill = goal as f64;
                    self.integ = 0.0;
                }
            }
            State::Running => {
                let hard_limit = goal * 4 + (self.in_rate * 0.1) as usize;
                if fill > hard_limit {
                    // A long stall elsewhere left a large backlog: jump back to the goal.
                    self.overruns += 1;
                    self.drop_frames(cons, fill - goal);
                    self.filt_fill = goal as f64;
                    self.integ = 0.0;
                }
            }
        }

        // PI controller on the smoothed buffer level, error in seconds of audio.
        let fill = (cons.slots() / ch) as f64 + self.hold_left as f64 + self.pending_hold as f64 - self.pending_skip as f64;
        let alpha = (dt / 0.3).min(1.0);
        self.filt_fill += (fill - self.filt_fill) * alpha;
        let err_frames = self.filt_fill - goal as f64;
        let err = err_frames / self.in_rate;
        // A persistent large surplus is removed at once (hidden by a short duck).
        let jump = (self.in_rate * 0.008).max(goal as f64 * 0.4);
        self.high_for = if err_frames > jump { self.high_for + dt } else { 0.0 };
        if self.state == State::Running && self.high_for > 0.2 && self.pending_skip == 0 && !self.duck_down {
            self.pending_skip = err_frames as usize;
            self.duck_down = true;
            self.high_for = 0.0;
        }
        self.integ = (self.integ + err * dt).clamp(-0.05, 0.05);
        let mut adj = (0.25 * err + 0.05 * self.integ).clamp(-MAX_ADJ, MAX_ADJ);
        if self.state != State::Running || self.duck_down || self.hold_left > 0 {
            adj = 0.0;
            self.integ = 0.0;
        }
        let step = self.nominal * (1.0 + adj);
        self.last_step = step;

        for f in 0..frames_out {
            // Holding: emit silence without consuming input (grows the buffer).
            if self.hold_left > 0 {
                self.hold_left -= 1;
                out[f * ch..(f + 1) * ch].fill(0.0);
                continue;
            }
            // Duck envelope (only active around a skip/hold and right after start).
            if self.duck_down {
                self.duck_gain -= 1.0 / DUCK_FRAMES;
                if self.duck_gain <= 0.0 {
                    self.duck_gain = 0.0;
                    self.duck_down = false;
                    let n = std::mem::take(&mut self.pending_skip);
                    if n > 0 {
                        self.drop_frames(cons, n);
                        self.skips += 1;
                        for _ in 0..3 {
                            self.push_frame(cons);
                        }
                    }
                    self.hold_left = std::mem::take(&mut self.pending_hold);
                    self.filt_fill = (cons.slots() / ch) as f64 + self.hold_left as f64;
                    self.integ = 0.0;
                }
            } else if self.duck_gain < 1.0 {
                self.duck_gain = (self.duck_gain + 1.0 / DUCK_FRAMES).min(1.0);
            }
            let t = self.frac as f32;
            let g = self.duck_gain;
            for c in 0..ch {
                let xm1 = self.hist[c];
                let x0 = self.hist[ch + c];
                let x1 = self.hist[2 * ch + c];
                let x2 = self.hist[3 * ch + c];
                // Catmull-Rom / cubic Hermite interpolation between x0 and x1.
                let a = -0.5 * xm1 + 1.5 * x0 - 1.5 * x1 + 0.5 * x2;
                let b = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
                let cc = -0.5 * xm1 + 0.5 * x1;
                out[f * ch + c] = (((a * t + b) * t + cc) * t + x0) * g;
            }
            self.frac += step;
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                if !self.push_frame(cons) {
                    // Ran dry: fade out and wait for the buffer to refill.
                    self.underruns += 1;
                    self.state = State::Prebuffering;
                    let start = (f + 1) * ch;
                    let fade = (frames_out - f - 1).min(32);
                    for i in 0..fade {
                        let g = self.duck_gain * (1.0 - (i + 1) as f32 / (fade + 1) as f32);
                        for c in 0..ch {
                            out[start + i * ch + c] = self.hist[2 * ch + c] * g;
                        }
                    }
                    out[start + fade * ch..].fill(0.0);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulate a capture device whose clock runs `ppm` faster than the output.
    fn simulate(ppm: f64, seconds: f64) -> (DriftResampler, Vec<f32>, Vec<usize>) {
        let out_rate = 48000u32;
        let in_rate_true = 48000.0 * (1.0 + ppm * 1e-6);
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000);
        let mut rs = DriftResampler::new(1, 48000, out_rate, 960);
        let mut out_all = Vec::new();
        let mut fills = Vec::new();
        let period = 480usize; // 10 ms output period
        let mut in_phase = 0.0f64; // fractional input frames owed
        let mut n_in = 0u64;
        let steps = (seconds * 100.0) as usize;
        for _ in 0..steps {
            // Capture delivers what its (slightly different) clock produced in 10 ms.
            in_phase += in_rate_true * 0.01;
            let n = in_phase.floor() as usize;
            in_phase -= n as f64;
            for _ in 0..n {
                let s = (2.0 * std::f64::consts::PI * 1000.0 * n_in as f64 / in_rate_true).sin() as f32 * 0.5;
                n_in += 1;
                let _ = prod.push(s);
            }
            let mut out = vec![0.0f32; period];
            rs.process(&mut cons, &mut out);
            fills.push(cons.slots());
            out_all.extend_from_slice(&out);
        }
        (rs, out_all, fills)
    }

    #[test]
    fn tracks_fast_and_slow_clocks_without_underruns() {
        for ppm in [-1500.0, -300.0, 0.0, 300.0, 1500.0] {
            let (rs, _out, fills) = simulate(ppm, 120.0);
            assert_eq!(rs.underruns, 0, "ppm {ppm}: underruns");
            assert_eq!(rs.overruns, 0, "ppm {ppm}: overruns");
            // After settling (last minute), the buffer must stay near the target.
            let tail = &fills[6000..];
            let avg = tail.iter().sum::<usize>() as f64 / tail.len() as f64;
            assert!((avg - 960.0).abs() < 150.0, "ppm {ppm}: avg fill {avg}");
            let max = *tail.iter().max().unwrap();
            assert!(max < 960 + 600, "ppm {ppm}: max fill {max}");
        }
    }

    #[test]
    fn output_is_clean_sine() {
        let (_rs, out, _) = simulate(500.0, 10.0);
        // Measure the residual against the best-fit 1 kHz sine over a 0.1 s window late in the run.
        let start = 48000 * 8;
        let win = &out[start..start + 960];
        // Estimate amplitude/phase with a quadrature correlation at the true output frequency.
        let f = 1000.0 / (1.0 + 500e-6) * (1.0 + 500e-6); // output freq ~= 1 kHz once locked
        let (mut si, mut co) = (0.0f64, 0.0f64);
        for (i, s) in win.iter().enumerate() {
            let ph = 2.0 * std::f64::consts::PI * f * i as f64 / 48000.0;
            si += *s as f64 * ph.sin();
            co += *s as f64 * ph.cos();
        }
        let n = win.len() as f64;
        let (a, b) = (2.0 * si / n, 2.0 * co / n);
        let mut resid = 0.0f64;
        let mut energy = 0.0f64;
        for (i, s) in win.iter().enumerate() {
            let ph = 2.0 * std::f64::consts::PI * f * i as f64 / 48000.0;
            let fit = a * ph.sin() + b * ph.cos();
            resid += (*s as f64 - fit).powi(2);
            energy += (*s as f64).powi(2);
        }
        let snr_db = 10.0 * (energy / resid).log10();
        assert!(snr_db > 40.0, "SNR {snr_db:.1} dB");
    }

    #[test]
    fn recovers_from_underrun() {
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000);
        let mut rs = DriftResampler::new(1, 48000, 48000, 480);
        let mut out = vec![0.0f32; 480];
        for _ in 0..1000 {
            let _ = prod.push(0.3);
        }
        rs.process(&mut cons, &mut out);
        assert!(rs.is_running());
        // Starve it.
        for _ in 0..5 {
            rs.process(&mut cons, &mut out);
        }
        assert!(rs.underruns >= 1);
        assert!(!rs.is_running());
        assert!(out.iter().all(|s| *s == 0.0));
        for _ in 0..1000 {
            let _ = prod.push(0.3);
        }
        rs.process(&mut cons, &mut out);
        assert!(rs.is_running());
    }

    /// Regression (seen on VB-CABLE): capture and output periods with a phase
    /// offset leave the average level a period below the goal. The settle
    /// step must fix that quickly instead of slewing the pitch for seconds.
    #[test]
    fn phase_offset_is_corrected_without_long_slew() {
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000);
        let mut rs = DriftResampler::new(1, 48000, 48000, 960);
        let mut ratios = Vec::new();
        for call in 0..1000 {
            // Output pulls 10 ms *before* capture delivers its 10 ms in each cycle.
            let mut out = vec![0.0f32; 480];
            rs.process(&mut cons, &mut out);
            for _ in 0..480 {
                let _ = prod.push(0.1);
            }
            if call > 100 {
                ratios.push((rs.current_ratio() - 1.0).abs());
            }
        }
        let worst = ratios.iter().cloned().fold(0.0f64, f64::max);
        assert!(worst < 0.0005, "pitch deviation after 1 s: {worst}");
        assert_eq!(rs.underruns, 0);
    }

    /// Regression: a device that asks for a big first chunk (or a burst of
    /// extra input at start) must not cause seconds of pitch-shifted output.
    #[test]
    fn startup_surplus_does_not_cause_pitch_slew() {
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000);
        let mut rs = DriftResampler::new(1, 48000, 48000, 960);
        let mut max_dev_after = 0.0f64;
        for call in 0..600 {
            // Capture delivers 10 ms per 10 ms; at start an extra 30 ms burst arrives.
            let n = if call == 1 { 480 + 1440 } else { 480 };
            for _ in 0..n {
                let _ = prod.push(0.1);
            }
            let frames = if call == 0 || call == 3 { 960 } else { 480 };
            let mut out = vec![0.0f32; frames];
            rs.process(&mut cons, &mut out);
            if call > 60 {
                max_dev_after = max_dev_after.max((rs.current_ratio() - 1.0).abs());
            }
        }
        assert!(max_dev_after < 0.0015, "pitch deviation after 0.6 s: {max_dev_after}");
        assert!(rs.skips <= 1, "skips {}", rs.skips);
        assert_eq!(rs.underruns, 0);
        let fill = cons.slots();
        assert!((fill as i64 - rs.goal_frames() as i64 + 480).abs() < 400, "settled fill {fill}, goal {}", rs.goal_frames());
    }
}
