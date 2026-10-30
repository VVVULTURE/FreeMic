//! Soundboard decoder thread.
//!
//! Receives play/stop commands, decodes the selected file a little ahead of
//! real time and hands fixed-size blocks to the mixer. Only one sound is ever
//! decoded at a time: a new command abandons the current sound immediately,
//! and the mixer discards anything already queued for the old one.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use rtrb::Producer;

use super::decode::StreamDecoder;
use super::{Block, Shared, BLOCK_FRAMES};

/// Blocks queued before a new sound may start (~21 ms at 48 kHz), so the mixer
/// never runs dry inside its first output period.
pub const START_BLOCKS: u64 = 4;

pub enum PlayerCmd {
    Play { generation: u64, path: PathBuf, looped: bool },
    Shutdown,
}

pub type ErrorSink = Arc<dyn Fn(String) + Send + Sync>;

pub struct Player {
    shared: Arc<Shared>,
    rx: Receiver<PlayerCmd>,
    out: Producer<Block>,
    on_error: ErrorSink,
}

enum Outcome {
    Done,
    NewCommand(PlayerCmd),
}

impl Player {
    pub fn new(shared: Arc<Shared>, rx: Receiver<PlayerCmd>, out: Producer<Block>, on_error: ErrorSink) -> Self {
        Self { shared, rx, out, on_error }
    }

    pub fn run(mut self) {
        let mut next: Option<PlayerCmd> = None;
        loop {
            let cmd = match next.take() {
                Some(c) => c,
                None => match self.rx.recv() {
                    Ok(c) => c,
                    Err(_) => return,
                },
            };
            match cmd {
                PlayerCmd::Shutdown => return,
                PlayerCmd::Play { generation, path, looped } => match self.play(generation, &path, looped) {
                    Outcome::Done => {}
                    Outcome::NewCommand(c) => next = Some(c),
                },
            }
        }
    }

    fn superseded(&self, generation: u64) -> bool {
        self.shared.target_gen.load(Ordering::Acquire) != generation
    }

    fn fail(&self, generation: u64, msg: String) {
        self.shared.finished_gen.fetch_max(generation, Ordering::AcqRel);
        (self.on_error)(msg);
    }

    fn play(&mut self, generation: u64, path: &PathBuf, looped: bool) -> Outcome {
        let rate = self.shared.out_rate.load(Ordering::Acquire);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut dec = match StreamDecoder::open(path, rate) {
            Ok(d) => d,
            Err(e) => {
                self.fail(generation, format!("Can't play \"{name}\": {e}"));
                return Outcome::Done;
            }
        };
        let mut staging: Vec<f32> = Vec::with_capacity(BLOCK_FRAMES * 8);
        let mut position: u64 = 0;
        let mut produced_this_pass = 0u64;
        let mut queued = 0u64;
        loop {
            match self.rx.try_recv() {
                Ok(cmd) => return Outcome::NewCommand(cmd),
                Err(TryRecvError::Disconnected) => return Outcome::NewCommand(PlayerCmd::Shutdown),
                Err(TryRecvError::Empty) => {}
            }
            if self.superseded(generation) {
                return Outcome::Done;
            }
            if self.out.slots() == 0 {
                // Queue is full (~340 ms ahead): wait for the mixer to consume. Right after a
                // switch it is full of the old sound's blocks, which the mixer drops on its
                // next period, so poll faster until this sound has its start blocks queued.
                let wait = if queued < START_BLOCKS { 1 } else { 8 };
                std::thread::park_timeout(Duration::from_millis(wait));
                continue;
            }

            // Top up staging to at least one block.
            let mut more = true;
            while staging.len() < BLOCK_FRAMES * 2 && more {
                match dec.read(&mut staging, BLOCK_FRAMES) {
                    Ok(m) => more = m,
                    Err(e) => {
                        self.fail(generation, format!("Playback of \"{name}\" stopped: {e}"));
                        return Outcome::Done;
                    }
                }
            }
            let at_end = !more && staging.len() <= BLOCK_FRAMES * 2;

            if at_end && looped {
                if produced_this_pass + (staging.len() / 2) as u64 == 0 {
                    self.fail(generation, format!("\"{name}\" contains no audio"));
                    return Outcome::Done;
                }
                // Restart from the beginning for the next pass; partial blocks are fine.
                match StreamDecoder::open(path, rate) {
                    Ok(d) => dec = d,
                    Err(e) => {
                        self.fail(generation, format!("Can't loop \"{name}\": {e}"));
                        return Outcome::Done;
                    }
                }
                produced_this_pass = 0;
                if staging.is_empty() {
                    position = 0;
                    continue;
                }
            }

            let frames = (staging.len() / 2).min(BLOCK_FRAMES);
            let mut block = Block::empty(generation);
            block.frames = frames;
            block.position = position;
            block.end = at_end && !looped;
            block.data[..frames * 2].copy_from_slice(&staging[..frames * 2]);
            staging.drain(..frames * 2);
            position += frames as u64;
            produced_this_pass += frames as u64;
            if at_end && looped {
                position = 0;
            }
            if self.out.push(block).is_err() {
                // Cannot happen (slots checked), but never block or panic here.
                continue;
            }
            queued += 1;
            if queued >= START_BLOCKS || (at_end && !looped) {
                self.shared.ready_gen.fetch_max(generation, Ordering::AcqRel);
            }
            if at_end && !looped {
                return Outcome::Done;
            }
        }
    }
}
