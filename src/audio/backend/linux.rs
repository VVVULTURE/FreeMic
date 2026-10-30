//! Linux backend (PulseAudio protocol: works with PulseAudio and with
//! PipeWire's pipewire-pulse server).
//!
//! The virtual microphone is made of two server modules that FreeMic loads at
//! start-up and unloads on exit (nothing is written to disk):
//!   * `module-null-sink`  "freemic_sink"  - FreeMic plays the mix into it
//!   * `module-remap-source` "freemic_mic" - exposes the sink's monitor as an
//!     ordinary microphone ("FreeMic Virtual Microphone") that apps can select.
//! Remapping matters: many apps (Chromium/Discord, games) hide ".monitor"
//! sources from their microphone lists.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use libpulse_binding as pa;
use pa::callbacks::ListResult;
use pa::context::{Context, FlagSet as CtxFlags, State as CtxState};
use pa::def::BufferAttr;
use pa::mainloop::standard::{IterateResult, Mainloop};
use pa::operation::State as OpState;
use pa::sample::{Format, Spec};
use pa::stream::{FlagSet as StreamFlags, PeekResult, SeekMode, State as StreamState, Stream};
use rtrb::{Consumer, Producer};

use super::{ChangeCallback, DeviceChange, DeviceInfo, RunCtl, Started, VirtualMicInfo};
use crate::audio::drift::DriftResampler;
use crate::audio::mixer::Mixer;

pub const SINK_NAME: &str = "freemic_sink";
pub const SOURCE_NAME: &str = "freemic_mic";
pub const SINK_DESC: &str = "FreeMic Mixer (internal)";
pub const SOURCE_DESC: &str = "FreeMic Virtual Microphone";
const RATE: u32 = 48000;

/// Module indices this process loaded (unloaded again in `cleanup`).
static LOADED: Mutex<Vec<u32>> = Mutex::new(Vec::new());

struct Pa {
    ml: Mainloop,
    ctx: Context,
}

#[derive(Clone)]
struct Node {
    name: String,
    desc: String,
    is_monitor: bool,
}

impl Pa {
    fn connect() -> Result<Pa, String> {
        let mut ml = Mainloop::new().ok_or("could not create a PulseAudio main loop")?;
        let mut ctx = Context::new(&ml, "FreeMic").ok_or("could not create a PulseAudio context")?;
        ctx.connect(None, CtxFlags::NOAUTOSPAWN, None)
            .map_err(|e| format!("could not connect to PulseAudio/PipeWire: {e}"))?;
        loop {
            match ml.iterate(true) {
                IterateResult::Success(_) => {}
                _ => return Err("the PulseAudio main loop failed".into()),
            }
            match ctx.get_state() {
                CtxState::Ready => break,
                CtxState::Failed | CtxState::Terminated => {
                    return Err("could not connect to PulseAudio or PipeWire (pipewire-pulse). Is the sound server running in your session?".into());
                }
                _ => {}
            }
        }
        Ok(Pa { ml, ctx })
    }

    fn wait<T: ?Sized>(&mut self, op: pa::operation::Operation<T>) {
        while op.get_state() == OpState::Running {
            if !matches!(self.ml.iterate(true), IterateResult::Success(_)) {
                break;
            }
        }
    }

    fn server(&mut self) -> (String, String, String) {
        let out = Rc::new(RefCell::new((String::new(), String::new(), String::new())));
        let o = out.clone();
        let op = self.ctx.introspect().get_server_info(move |i| {
            *o.borrow_mut() = (
                i.server_name.as_deref().unwrap_or("").to_string(),
                i.default_source_name.as_deref().unwrap_or("").to_string(),
                i.default_sink_name.as_deref().unwrap_or("").to_string(),
            );
        });
        self.wait(op);
        out.borrow().clone()
    }

    fn sources(&mut self) -> Vec<Node> {
        let out = Rc::new(RefCell::new(Vec::new()));
        let o = out.clone();
        let op = self.ctx.introspect().get_source_info_list(move |r| {
            if let ListResult::Item(i) = r {
                o.borrow_mut().push(Node {
                    name: i.name.as_deref().unwrap_or("").to_string(),
                    desc: i.description.as_deref().unwrap_or("").to_string(),
                    is_monitor: i.monitor_of_sink.is_some(),
                });
            }
        });
        self.wait(op);
        out.take()
    }

    fn sinks(&mut self) -> Vec<Node> {
        let out = Rc::new(RefCell::new(Vec::new()));
        let o = out.clone();
        let op = self.ctx.introspect().get_sink_info_list(move |r| {
            if let ListResult::Item(i) = r {
                o.borrow_mut().push(Node {
                    name: i.name.as_deref().unwrap_or("").to_string(),
                    desc: i.description.as_deref().unwrap_or("").to_string(),
                    is_monitor: false,
                });
            }
        });
        self.wait(op);
        out.take()
    }

    fn modules(&mut self) -> Vec<(u32, String, String)> {
        let out = Rc::new(RefCell::new(Vec::new()));
        let o = out.clone();
        let op = self.ctx.introspect().get_module_info_list(move |r| {
            if let ListResult::Item(m) = r {
                o.borrow_mut().push((
                    m.index,
                    m.name.as_deref().unwrap_or("").to_string(),
                    m.argument.as_deref().unwrap_or("").to_string(),
                ));
            }
        });
        self.wait(op);
        out.take()
    }

    fn load(&mut self, name: &str, args: &str) -> Result<u32, String> {
        let out = Rc::new(RefCell::new(u32::MAX));
        let o = out.clone();
        let op = self.ctx.introspect().load_module(name, args, move |idx| *o.borrow_mut() = idx);
        self.wait(op);
        let idx = *out.borrow();
        if idx == u32::MAX {
            Err(format!("the sound server refused to load {name}"))
        } else {
            Ok(idx)
        }
    }

    fn unload(&mut self, idx: u32) {
        let op = self.ctx.introspect().unload_module(idx, |_| {});
        self.wait(op);
    }
}

impl Drop for Pa {
    fn drop(&mut self) {
        self.ctx.disconnect();
    }
}

/// FreeMic's own modules (including leftovers from a previous crash).
fn our_modules(pa: &mut Pa) -> Vec<(u32, String)> {
    pa.modules()
        .into_iter()
        .filter(|(_, _, arg)| arg.contains(&format!("sink_name={SINK_NAME}")) || arg.contains(&format!("source_name={SOURCE_NAME}")))
        .map(|(i, n, _)| (i, n))
        .collect()
}

/// Create the virtual microphone if it does not exist yet.
fn ensure_virtual_mic(pa: &mut Pa) -> Result<(), String> {
    let has_sink = pa.sinks().iter().any(|s| s.name == SINK_NAME);
    let has_src = pa.sources().iter().any(|s| s.name == SOURCE_NAME);
    if has_sink && has_src {
        return Ok(());
    }
    // Half-present (e.g. after a crash or a server restart): start clean.
    for (idx, name) in our_modules(pa).into_iter().rev() {
        log::info!("removing leftover module {name} #{idx}");
        pa.unload(idx);
    }
    let sink_args = format!(
        "sink_name={SINK_NAME} rate={RATE} channels=2 channel_map=front-left,front-right sink_properties='device.description=\"{SINK_DESC}\"'"
    );
    let sink = pa.load("module-null-sink", &sink_args)?;
    LOADED.lock().unwrap().push(sink);
    let src_args = format!(
        "master={SINK_NAME}.monitor source_name={SOURCE_NAME} channels=2 channel_map=front-left,front-right source_properties='device.description=\"{SOURCE_DESC}\"'"
    );
    match pa.load("module-remap-source", &src_args) {
        Ok(src) => LOADED.lock().unwrap().push(src),
        Err(e) => {
            pa.unload(sink);
            LOADED.lock().unwrap().retain(|i| *i != sink);
            return Err(format!("{e} (module-remap-source)"));
        }
    }
    log::info!("created virtual microphone ({SOURCE_DESC})");
    Ok(())
}

pub fn list_inputs() -> Result<Vec<DeviceInfo>, String> {
    let mut pa = Pa::connect()?;
    Ok(pa
        .sources()
        .into_iter()
        .filter(|s| !s.is_monitor && s.name != SOURCE_NAME)
        .map(|s| DeviceInfo { id: s.name, name: s.desc })
        .collect())
}

pub fn list_outputs() -> Result<Vec<DeviceInfo>, String> {
    let mut pa = Pa::connect()?;
    Ok(pa
        .sinks()
        .into_iter()
        .filter(|s| s.name != SINK_NAME)
        .map(|s| DeviceInfo { id: s.name, name: s.desc })
        .collect())
}

pub fn virtual_mic(chosen: Option<&str>) -> VirtualMicInfo {
    let mut pa = match Pa::connect() {
        Ok(p) => p,
        Err(e) => return VirtualMicInfo { problem: Some(e), ..Default::default() },
    };
    if let Some(sink) = chosen {
        return match pa.sinks().into_iter().find(|s| s.name == sink) {
            Some(s) => VirtualMicInfo {
                mic_name: Some(format!("Monitor of {}", s.desc)),
                feed: Some(DeviceInfo { id: s.name, name: s.desc }),
                problem: None,
            },
            None => VirtualMicInfo {
                problem: Some("The selected output device is not available. Choose Automatic in Settings.".into()),
                ..Default::default()
            },
        };
    }
    match ensure_virtual_mic(&mut pa) {
        Ok(()) => VirtualMicInfo {
            feed: Some(DeviceInfo { id: SINK_NAME.into(), name: SINK_DESC.into() }),
            mic_name: Some(SOURCE_DESC.into()),
            problem: None,
        },
        Err(e) => VirtualMicInfo { problem: Some(format!("Could not create the virtual microphone: {e}")), ..Default::default() },
    }
}

pub fn feedback_inputs(_vm: &VirtualMicInfo) -> Vec<String> {
    vec![SOURCE_NAME.into(), format!("{SINK_NAME}.monitor")]
}

fn spec(rate: u32, channels: u8) -> Spec {
    Spec { format: Format::FLOAT32NE, channels, rate }
}

fn as_bytes(s: &[f32]) -> &[u8] {
    // SAFETY: f32 has no invalid bit patterns and u8 has alignment 1.
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

/// One audio stream with its own connection and main loop, driven from the
/// calling thread. Streams use DONT_MOVE: if a device disappears the stream
/// fails (and FreeMic retries) instead of the server silently re-routing it,
/// which could otherwise send the microphone mix to the real speakers.
struct PaStream {
    pa: Pa,
    stream: Stream,
    what: String,
}

impl PaStream {
    fn open(device: &str, rate: u32, channels: u8, record: bool, attr: BufferAttr, title: &str) -> Result<PaStream, String> {
        let mut pa = Pa::connect()?;
        let mut stream = Stream::new(&mut pa.ctx, title, &spec(rate, channels), None).ok_or("could not create a stream")?;
        let flags = StreamFlags::DONT_MOVE | StreamFlags::ADJUST_LATENCY | StreamFlags::AUTO_TIMING_UPDATE | StreamFlags::INTERPOLATE_TIMING;
        let r = if record {
            stream.connect_record(Some(device), Some(&attr), flags)
        } else {
            stream.connect_playback(Some(device), Some(&attr), flags, None, None)
        };
        r.map_err(|e| format!("could not open \"{device}\": {e}"))?;
        let mut s = PaStream { pa, stream, what: device.to_string() };
        loop {
            match s.stream.get_state() {
                StreamState::Ready => return Ok(s),
                StreamState::Failed | StreamState::Terminated => {
                    return Err(format!("could not open \"{device}\" (device missing or busy)"));
                }
                _ => s.iterate()?,
            }
        }
    }

    /// Block until the server sends something (a data request, captured audio, a state change).
    fn iterate(&mut self) -> Result<(), String> {
        match self.pa.ml.iterate(true) {
            IterateResult::Success(_) => {}
            _ => return Err("lost the connection to the sound server".into()),
        }
        match self.stream.get_state() {
            StreamState::Failed | StreamState::Terminated => Err(format!("\"{}\" was disconnected", self.what)),
            _ => Ok(()),
        }
    }

    /// Write audio produced by `fill` whenever the server asks for at least `period` frames.
    fn run_playback(&mut self, period: usize, ctl: &RunCtl, mut fill: impl FnMut(&mut [f32])) -> Result<(), String> {
        let mut buf = vec![0.0f32; period * 2];
        let period_bytes = period * 8;
        while !ctl.should_stop() {
            let writable = self.stream.writable_size().unwrap_or(0);
            if writable >= period_bytes {
                fill(&mut buf);
                self.stream
                    .write(as_bytes(&buf), None, 0, SeekMode::Relative)
                    .map_err(|e| format!("{}: {e}", self.what))?;
            } else {
                self.iterate()?;
            }
        }
        Ok(())
    }
}

impl Drop for PaStream {
    fn drop(&mut self) {
        let _ = self.stream.disconnect();
    }
}

fn playback_attr(period: usize) -> BufferAttr {
    BufferAttr {
        maxlength: u32::MAX,
        tlength: period as u32 * 3 * 8, // ~30 ms queued in the server
        prebuf: u32::MAX,
        minreq: period as u32 * 8,
        fragsize: u32::MAX,
    }
}

pub fn run_output(
    vm: &VirtualMicInfo,
    mixer: &mut Mixer,
    ctl: &RunCtl,
    started: &mut dyn FnMut(Started, &mut Mixer),
) -> Result<(), String> {
    let feed = vm.feed.as_ref().ok_or("no virtual microphone device")?;
    let period = (RATE / 100) as usize;
    let mut s = PaStream::open(&feed.id, RATE, 2, false, playback_attr(period), "Virtual microphone mix")?;
    s.what = feed.name.clone();
    started(Started { name: feed.name.clone(), rate: RATE, period_frames: period }, mixer);
    s.run_playback(period, ctl, |buf| mixer.process(buf))
}

pub fn run_monitor(
    device: &str,
    rate: u32,
    cons: &mut Consumer<f32>,
    ctl: &RunCtl,
    started: &mut dyn FnMut(Started),
) -> Result<(), String> {
    let period = (rate / 100) as usize;
    let mut s = PaStream::open(device, rate, 2, false, playback_attr(period), "Soundboard monitor")?;
    let mut rs = DriftResampler::new(2, rate, rate, period * 2);
    started(Started { name: device.to_string(), rate, period_frames: period });
    s.run_playback(period, ctl, |buf| rs.process(cons, buf))
}

pub fn run_capture(
    device: Option<&str>,
    avoid: &[String],
    rate: u32,
    prod: &mut Producer<f32>,
    ctl: &RunCtl,
    started: &mut dyn FnMut(Started),
) -> Result<(), String> {
    let (name, desc) = {
        let mut pa = Pa::connect()?;
        let sources = pa.sources();
        let pick = |n: &str| sources.iter().find(|s| s.name == n).cloned();
        match device {
            Some(d) => {
                let s = pick(d).ok_or("the selected microphone is not connected")?;
                (s.name, s.desc)
            }
            None => {
                let (_, default_src, _) = pa.server();
                match pick(&default_src).filter(|s| !s.is_monitor && !avoid.contains(&s.name)) {
                    Some(s) => (s.name, s.desc),
                    None => {
                        // Default is a monitor or our own virtual mic: use the first real microphone.
                        let s = sources
                            .iter()
                            .find(|s| !s.is_monitor && !avoid.contains(&s.name))
                            .ok_or("no microphone found")?;
                        (s.name.clone(), s.desc.clone())
                    }
                }
            }
        }
    };
    let period = (rate / 100) as usize;
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: u32::MAX,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: period as u32 * 4,
    };
    let mut s = PaStream::open(&name, rate, 1, true, attr, "Microphone passthrough")?;
    s.what = desc.clone();
    started(Started { name: desc.clone(), rate, period_frames: period });
    while !ctl.should_stop() {
        match s.stream.peek().map_err(|e| format!("{desc}: {e}"))? {
            PeekResult::Empty => s.iterate()?,
            PeekResult::Hole(n) => {
                let k = (n / 4).min(prod.slots());
                if let Ok(chunk) = prod.write_chunk_uninit(k) {
                    chunk.fill_from_iter(std::iter::repeat(0.0f32));
                }
                let _ = s.stream.discard();
            }
            PeekResult::Data(d) => {
                let k = (d.len() / 4).min(prod.slots());
                if let Ok(chunk) = prod.write_chunk_uninit(k) {
                    chunk.fill_from_iter(d.chunks_exact(4).map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]])));
                }
                let _ = s.stream.discard();
            }
        }
    }
    Ok(())
}

pub fn watch(cb: ChangeCallback) -> Option<Box<dyn std::any::Any + Send>> {
    use pa::context::subscribe::{Facility, InterestMaskSet, Operation as SubOp};
    std::thread::Builder::new()
        .name("freemic-pa-watch".into())
        .spawn(move || loop {
            if let Ok(mut pa) = Pa::connect() {
                let c = cb.clone();
                pa.ctx.set_subscribe_callback(Some(Box::new(move |fac, op, _idx| match (fac, op) {
                    (Some(Facility::Server), _) => c(DeviceChange::DefaultInput),
                    (Some(Facility::Source | Facility::Sink), Some(SubOp::New | SubOp::Removed)) => c(DeviceChange::Topology),
                    _ => {}
                })));
                pa.ctx.subscribe(InterestMaskSet::SINK | InterestMaskSet::SOURCE | InterestMaskSet::SERVER, |_| {});
                // Blocks in poll(); wakes only when the server sends events.
                while let IterateResult::Success(_) = pa.ml.iterate(true) {
                    if pa.ctx.get_state() != CtxState::Ready {
                        break;
                    }
                }
                cb(DeviceChange::Topology);
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        })
        .ok();
    None
}

/// Remove the virtual microphone again (called on exit).
pub fn cleanup() {
    let Ok(mut pa) = Pa::connect() else { return };
    let mut ours: Vec<u32> = our_modules(&mut pa).into_iter().map(|(i, _)| i).collect();
    ours.sort_unstable();
    // Unload the remap source before the sink it depends on.
    for idx in ours.into_iter().rev() {
        pa.unload(idx);
    }
    LOADED.lock().unwrap().clear();
    log::info!("removed virtual microphone");
}
