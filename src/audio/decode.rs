//! Audio file decoding (MP3, WAV, OGG Vorbis, FLAC, M4A/AAC/ALAC) to
//! interleaved stereo `f32` at the engine's sample rate.
//!
//! Decoding is streaming: `StreamDecoder::read` produces a little audio at a
//! time so long files start instantly and never sit fully in memory.

use std::fs::File;
use std::path::Path;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "oga", "flac", "m4a", "mp4", "aac"];

#[derive(Debug, Clone)]
pub struct SoundInfo {
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_secs: Option<f64>,
}

pub fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn friendly(err: SymError) -> String {
    match err {
        SymError::IoError(e) => format!("could not read the file ({e})"),
        SymError::DecodeError(e) => format!("the file is damaged or not valid audio ({e})"),
        SymError::Unsupported(e) => format!("unsupported audio format ({e})"),
        other => other.to_string(),
    }
}

struct Source {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    rate: u32,
    channels: usize,
    duration_secs: Option<f64>,
}

fn open_source(path: &Path) -> Result<Source, String> {
    let file = File::open(path).map_err(|e| format!("could not open the file ({e})"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| match e {
            SymError::Unsupported(_) => "not a supported audio file (MP3, WAV, OGG, FLAC or M4A)".to_string(),
            other => friendly(other),
        })?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "the file contains no audio track".to_string())?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| "the file's audio track has no usable codec parameters".to_string())?
        .clone();
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| match e {
            SymError::Unsupported(_) => "the audio codec in this file is not supported".to_string(),
            other => friendly(other),
        })?;
    let rate = params.sample_rate.ok_or_else(|| "unknown sample rate".to_string())?;
    if rate == 0 || rate > 768_000 {
        return Err(format!("invalid sample rate {rate}"));
    }
    let channels = params.channels.as_ref().map(|c| c.count()).unwrap_or(0);
    let duration_secs = match (track.num_frames, track.time_base) {
        (Some(n), _) => Some(n as f64 / rate as f64),
        _ => None,
    };
    let track_id = track.id;
    Ok(Source { format, decoder, track_id, rate, channels, duration_secs })
}

/// Down/up-mix one interleaved frame of `ch` channels into stereo.
#[inline]
fn to_stereo(frame: &[f32]) -> (f32, f32) {
    match frame.len() {
        0 => (0.0, 0.0),
        1 => (frame[0], frame[0]),
        2 => (frame[0], frame[1]),
        n => {
            // Assume SMPTE/WAVE order: FL FR FC LFE BL BR SL SR ...
            let c = frame[2] * 0.707;
            let (mut l, mut r) = (frame[0] + c, frame[1] + c);
            for (i, s) in frame.iter().enumerate().skip(4) {
                if i % 2 == 0 { l += s * 0.707 } else { r += s * 0.707 }
            }
            let _ = n;
            (l, r)
        }
    }
}

pub struct StreamDecoder {
    src: Source,
    out_rate: u32,
    resampler: Option<Async<f32>>,
    /// Stereo frames at the source rate waiting for the resampler.
    pending: Vec<f32>,
    scratch: Vec<f32>,
    rs_out: Vec<f32>,
    /// Output frames still to discard (resampler group delay).
    trim: usize,
    in_frames_total: u64,
    out_frames_total: u64,
    eof: bool,
    finished: bool,
    decode_errors: u32,
    flushes: u32,
}

impl StreamDecoder {
    pub fn open(path: &Path, out_rate: u32) -> Result<Self, String> {
        let src = open_source(path)?;
        let resampler = if src.rate != out_rate {
            let params = SincInterpolationParameters::new(64, WindowFunction::BlackmanHarris2)
                .oversampling_factor(128)
                .interpolation(SincInterpolationType::Cubic);
            let rs = Async::<f32>::new_sinc(
                out_rate as f64 / src.rate as f64,
                1.0,
                &params,
                1024,
                2,
                FixedAsync::Input,
            )
            .map_err(|e| format!("resampler setup failed: {e}"))?;
            Some(rs)
        } else {
            None
        };
        let trim = resampler.as_ref().map(|r| r.output_delay()).unwrap_or(0);
        let rs_cap = resampler.as_ref().map(|r| r.output_frames_max()).unwrap_or(0);
        Ok(Self {
            src,
            out_rate,
            resampler,
            pending: Vec::with_capacity(8192),
            scratch: Vec::new(),
            rs_out: vec![0.0; rs_cap * 2],
            trim,
            in_frames_total: 0,
            out_frames_total: 0,
            eof: false,
            finished: false,
            decode_errors: 0,
            flushes: 0,
        })
    }

    pub fn info(&self) -> SoundInfo {
        SoundInfo {
            sample_rate: self.src.rate,
            channels: self.src.channels,
            duration_secs: self.src.duration_secs,
        }
    }

    /// Decode the next packet into `pending` (source-rate stereo). Returns false at end of stream.
    fn decode_packet(&mut self) -> Result<bool, String> {
        loop {
            let packet = match self.src.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
                Err(SymError::ResetRequired) => return Ok(false),
                Err(e) => return Err(friendly(e)),
            };
            if packet.track_id != self.src.track_id {
                continue;
            }
            match self.src.decoder.decode(&packet) {
                Ok(buf) => {
                    let ch = buf.spec().channels().count().max(1);
                    self.scratch.resize(buf.samples_interleaved(), 0.0);
                    buf.copy_to_slice_interleaved(&mut self.scratch[..]);
                    for frame in self.scratch.chunks_exact(ch) {
                        let (l, r) = to_stereo(frame);
                        self.pending.push(l);
                        self.pending.push(r);
                    }
                    return Ok(true);
                }
                Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => {
                    // Skip a corrupt packet; give up if the file is mostly garbage.
                    self.decode_errors += 1;
                    if self.decode_errors > 50 {
                        return Err("too many decoding errors; the file appears to be damaged".into());
                    }
                }
                Err(e) => return Err(friendly(e)),
            }
        }
    }

    /// Append up to roughly `want_frames` stereo frames at the output rate to `out`.
    /// Returns `Ok(false)` once the stream is completely finished.
    pub fn read(&mut self, out: &mut Vec<f32>, want_frames: usize) -> Result<bool, String> {
        if self.finished {
            return Ok(false);
        }
        let start_len = out.len();
        while (out.len() - start_len) / 2 < want_frames && !self.finished {
            match self.resampler.as_mut() {
                None => {
                    if self.pending.is_empty() {
                        if self.eof || !self.decode_packet()? {
                            self.eof = true;
                            self.finished = true;
                            break;
                        }
                    }
                    out.extend_from_slice(&self.pending);
                    self.in_frames_total += (self.pending.len() / 2) as u64;
                    self.out_frames_total += (self.pending.len() / 2) as u64;
                    self.pending.clear();
                }
                Some(_) => {
                    let need = self.resampler.as_ref().unwrap().input_frames_next();
                    while !self.eof && self.pending.len() / 2 < need {
                        if !self.decode_packet()? {
                            self.eof = true;
                        }
                    }
                    self.resample_step(out, need)?;
                }
            }
        }
        Ok(!self.finished || out.len() > start_len)
    }

    fn resample_step(&mut self, out: &mut Vec<f32>, need: usize) -> Result<(), String> {
        let rs = self.resampler.as_mut().unwrap();
        let have = self.pending.len() / 2;
        let ratio = self.out_rate as f64 / self.src.rate as f64;
        let mut indexing = Indexing::new();
        let used_in;
        if have >= need {
            used_in = need;
        } else {
            // Final partial chunk (or pure flush once input is exhausted).
            used_in = have;
            indexing.partial_len = Some(have);
            self.pending.resize(need * 2, 0.0);
        }
        let input = InterleavedSlice::new(&self.pending[..need * 2], 2, need)
            .map_err(|e| format!("resampler input error: {e}"))?;
        let cap = self.rs_out.len() / 2;
        let mut output = InterleavedSlice::new_mut(&mut self.rs_out[..], 2, cap)
            .map_err(|e| format!("resampler output error: {e}"))?;
        let (_n_in, n_out) = rs
            .process_into_buffer(&input, &mut output, Some(&indexing))
            .map_err(|e| format!("resampling failed: {e}"))?;
        self.in_frames_total += used_in as u64;
        self.pending.drain(..(used_in * 2).min(self.pending.len()));
        if used_in < need {
            self.pending.clear();
            self.flushes += 1;
        }

        let expected_total = (self.in_frames_total as f64 * ratio).round() as u64;
        let mut produced = &self.rs_out[..n_out * 2];
        if self.trim > 0 {
            let t = self.trim.min(n_out);
            produced = &produced[t * 2..];
            self.trim -= t;
        }
        let mut take = produced.len() / 2;
        if self.eof {
            let remaining = expected_total.saturating_sub(self.out_frames_total) as usize;
            take = take.min(remaining);
        }
        out.extend_from_slice(&produced[..take * 2]);
        self.out_frames_total += take as u64;
        if self.eof && self.pending.is_empty() && (self.out_frames_total >= expected_total || self.flushes > 16) {
            self.finished = true;
        }
        Ok(())
    }
}

/// Validate that a file can be decoded and return basic information.
/// Decodes the first second of audio to catch damaged files at import time.
pub fn probe(path: &Path) -> Result<SoundInfo, String> {
    let mut dec = StreamDecoder::open(path, 48000)?;
    let mut buf = Vec::new();
    dec.read(&mut buf, 48000)?;
    if buf.is_empty() {
        return Err("the file contains no audio".into());
    }
    Ok(dec.info())
}

/// Decode a whole file (used by tests and tools).
pub fn decode_all(path: &Path, out_rate: u32) -> Result<Vec<f32>, String> {
    let mut dec = StreamDecoder::open(path, out_rate)?;
    let mut out = Vec::new();
    while dec.read(&mut out, 8192)? {}
    Ok(out)
}
