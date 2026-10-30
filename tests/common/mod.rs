#![allow(dead_code)]
use std::path::PathBuf;

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
}

/// Goertzel power estimate: amplitude of `freq` in `x` (mono) at `rate`.
pub fn tone_amplitude(x: &[f32], freq: f64, rate: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * freq / rate;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &v in x {
        let s0 = v as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    2.0 * power.max(0.0).sqrt() / x.len() as f64
}

pub fn channel(inter: &[f32], ch: usize) -> Vec<f32> {
    inter.chunks_exact(2).map(|f| f[ch]).collect()
}

pub fn zero_crossing_freq(x: &[f32], rate: f64) -> f64 {
    let mut n = 0;
    for w in x.windows(2) {
        if (w[0] <= 0.0 && w[1] > 0.0) || (w[0] >= 0.0 && w[1] < 0.0) {
            n += 1;
        }
    }
    n as f64 / 2.0 / (x.len() as f64 / rate)
}

pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

pub fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}
