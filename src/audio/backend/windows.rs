//! Windows backend (WASAPI, shared mode, event driven).
//!
//! Windows has no user-mode API for creating a microphone that other
//! applications can select, so FreeMic writes its mix into the playback side
//! of a virtual audio cable driver (VB-CABLE by default: "CABLE Input") and
//! applications record from the cable's capture side ("CABLE Output").

use std::sync::Arc;

use rtrb::{Consumer, Producer};
use wasapi::{
    initialize_mta, AudioClient, DeviceEnumerator, DeviceEventCallbacks, Direction, Role, SampleType, StreamMode,
    WaveFormat,
};

use super::{ChangeCallback, DeviceChange, DeviceInfo, RunCtl, Started, VirtualMicInfo};
use crate::audio::drift::DriftResampler;
use crate::audio::mixer::Mixer;

fn com() {
    // S_FALSE / RPC_E_CHANGED_MODE are fine: COM is usable either way.
    let _ = initialize_mta();
}

fn describe(e: wasapi::WasapiError) -> String {
    let text = e.to_string();
    // AUDCLNT_E_DEVICE_INVALIDATED, E_NOTFOUND, AUDCLNT_E_DEVICE_IN_USE
    if text.contains("0x88890004") {
        "the device was disconnected or disabled".into()
    } else if text.contains("0x80070490") {
        "the device is not available".into()
    } else if text.contains("0x8889000A") {
        "the device is being used exclusively by another application".into()
    } else {
        text
    }
}

fn list(dir: Direction) -> Result<Vec<DeviceInfo>, String> {
    com();
    let en = DeviceEnumerator::new().map_err(describe)?;
    let coll = en.get_device_collection(&dir).map_err(describe)?;
    let mut out = Vec::new();
    for dev in &coll {
        let Ok(dev) = dev else { continue };
        if let (Ok(id), Ok(name)) = (dev.get_id(), dev.get_friendlyname()) {
            out.push(DeviceInfo { id, name });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

pub fn list_inputs() -> Result<Vec<DeviceInfo>, String> {
    list(Direction::Capture)
}

pub fn list_outputs() -> Result<Vec<DeviceInfo>, String> {
    list(Direction::Render)
}

/// Rank playback endpoints that are known to be the input side of a virtual cable.
fn cable_rank(name: &str) -> Option<u32> {
    let n = name.to_ascii_lowercase();
    if n.starts_with("cable input") {
        Some(0)
    } else if n.starts_with("cable-") && n.contains(" input") {
        Some(1)
    } else if n.contains("(virtual audio cable)") && n.starts_with("line") {
        Some(2)
    } else if n.starts_with("cable in ") {
        Some(3) // "CABLE In 16ch": multichannel variant of the same cable
    } else {
        None
    }
}

/// Name of the recording endpoint that carries what is played into `render_name`.
pub fn cable_partner_name(render_name: &str, captures: &[DeviceInfo]) -> Option<String> {
    let n = render_name.to_ascii_lowercase();
    let candidate = if n.starts_with("cable in 16ch") {
        // "CABLE In 16ch (VB-Audio Virtual Cable)" -> "CABLE Output (VB-Audio Virtual Cable)"
        render_name.replacen("In 16ch", "Output", 1)
    } else if n.contains(" input") {
        let idx = n.find(" input").unwrap();
        format!("{} Output{}", &render_name[..idx], &render_name[idx + 6..])
    } else {
        render_name.to_string() // Virtual Audio Cable uses identical names ("Line 1")
    };
    captures
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(&candidate))
        .map(|c| c.name.clone())
}

pub fn virtual_mic(chosen: Option<&str>) -> VirtualMicInfo {
    let outputs = match list_outputs() {
        Ok(o) => o,
        Err(e) => return VirtualMicInfo { problem: Some(format!("Could not list audio devices: {e}")), ..Default::default() },
    };
    let inputs = list_inputs().unwrap_or_default();
    let feed = match chosen {
        Some(id) => match outputs.iter().find(|d| d.id == id) {
            Some(d) => Some(d.clone()),
            None => {
                return VirtualMicInfo {
                    problem: Some("The selected virtual microphone device is not available. Pick another one in Settings or choose Automatic.".into()),
                    ..Default::default()
                };
            }
        },
        None => outputs
            .iter()
            .filter_map(|d| cable_rank(&d.name).map(|r| (r, d)))
            .min_by_key(|(r, _)| *r)
            .map(|(_, d)| d.clone()),
    };
    match feed {
        Some(f) => {
            let mic_name = cable_partner_name(&f.name, &inputs);
            VirtualMicInfo { feed: Some(f), mic_name, problem: None }
        }
        None => VirtualMicInfo {
            feed: None,
            mic_name: None,
            problem: Some(
                "No virtual audio cable found. FreeMic needs the free VB-CABLE driver on Windows so other apps can use its microphone. Open Settings to install it."
                    .into(),
            ),
        },
    }
}

pub fn feedback_inputs(vm: &VirtualMicInfo) -> Vec<String> {
    let Some(mic_name) = &vm.mic_name else { return Vec::new() };
    list_inputs()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| &d.name == mic_name)
        .map(|d| d.id)
        .collect()
}

pub fn promote_thread() {
    use windows::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
    let mut idx = 0u32;
    // "Pro Audio" MMCSS class gives the thread real-time scheduling without admin rights.
    let _ = unsafe { AvSetMmThreadCharacteristicsW(windows::core::w!("Pro Audio"), &mut idx) };
}

struct Stream {
    client: AudioClient,
    rate: u32,
    channels: usize,
    period_frames: usize,
}

fn open_stream(dev: &wasapi::Device, dir: Direction, rate: Option<u32>, channels: usize) -> Result<Stream, String> {
    let mut client = dev.get_iaudioclient().map_err(describe)?;
    let mix = client.get_mixformat().map_err(describe)?;
    let rate = rate.unwrap_or(mix.get_samplespersec());
    let fmt = WaveFormat::new(32, 32, &SampleType::Float, rate as usize, channels, None);
    let (def, _min) = client.get_device_period().map_err(describe)?;
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: def };
    client.initialize_client(&fmt, &dir, &mode).map_err(describe)?;
    let period_frames = (def as u64 * rate as u64 / 10_000_000) as usize;
    Ok(Stream { client, rate, channels, period_frames })
}

fn as_bytes(s: &[f32]) -> &[u8] {
    // SAFETY: f32 has no invalid bit patterns and u8 has alignment 1.
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

fn render_loop(
    stream: &Stream,
    ctl: &RunCtl,
    mut fill: impl FnMut(&mut [f32]),
) -> Result<(), String> {
    let client = &stream.client;
    let h = client.set_get_eventhandle().map_err(describe)?;
    let rc = client.get_audiorenderclient().map_err(describe)?;
    let buf_frames = client.get_buffer_size().map_err(describe)? as usize;
    let mut buf = vec![0.0f32; buf_frames * stream.channels];
    // Pre-fill one period so the stream starts without an underrun.
    let avail = client.get_available_space_in_frames().map_err(describe)? as usize;
    let pre = avail.min(stream.period_frames.max(1));
    fill(&mut buf[..pre * stream.channels]);
    rc.write_to_device(pre, as_bytes(&buf[..pre * stream.channels]), None).map_err(describe)?;
    client.start_stream().map_err(describe)?;
    let result = loop {
        if ctl.should_stop() {
            break Ok(());
        }
        if h.wait_for_event(1000).is_err() {
            break Err("the device stopped responding".to_string());
        }
        let avail = match client.get_available_space_in_frames() {
            Ok(a) => a as usize,
            Err(e) => break Err(describe(e)),
        };
        if avail == 0 {
            continue;
        }
        let n = avail.min(buf_frames);
        let slice = &mut buf[..n * stream.channels];
        fill(slice);
        if let Err(e) = rc.write_to_device(n, as_bytes(slice), None) {
            break Err(describe(e));
        }
    };
    let _ = client.stop_stream();
    result
}

pub fn run_output(
    vm: &VirtualMicInfo,
    mixer: &mut Mixer,
    ctl: &RunCtl,
    started: &mut dyn FnMut(Started, &mut Mixer),
) -> Result<(), String> {
    com();
    let feed = vm.feed.as_ref().ok_or("no virtual microphone device")?;
    let en = DeviceEnumerator::new().map_err(describe)?;
    let dev = en.get_device(&feed.id).map_err(|e| format!("{}: {}", feed.name, describe(e)))?;
    let stream = open_stream(&dev, Direction::Render, None, 2).map_err(|e| format!("{}: {e}", feed.name))?;
    started(Started { name: feed.name.clone(), rate: stream.rate, period_frames: stream.period_frames }, mixer);
    render_loop(&stream, ctl, |buf| mixer.process(buf)).map_err(|e| format!("{}: {e}", feed.name))
}

pub fn run_monitor(
    device: &str,
    rate: u32,
    cons: &mut Consumer<f32>,
    ctl: &RunCtl,
    started: &mut dyn FnMut(Started),
) -> Result<(), String> {
    com();
    let en = DeviceEnumerator::new().map_err(describe)?;
    let dev = en.get_device(device).map_err(describe)?;
    let name = dev.get_friendlyname().unwrap_or_else(|_| "monitor".into());
    // Ask Windows for our engine rate so only clock drift needs correcting here.
    let stream = open_stream(&dev, Direction::Render, Some(rate), 2).map_err(|e| format!("{name}: {e}"))?;
    let target = stream.period_frames + rate as usize / 50;
    let mut rs = DriftResampler::new(2, rate, stream.rate, target);
    started(Started { name: name.clone(), rate: stream.rate, period_frames: stream.period_frames });
    render_loop(&stream, ctl, |buf| rs.process(cons, buf)).map_err(|e| format!("{name}: {e}"))
}

pub fn run_capture(
    device: Option<&str>,
    avoid: &[String],
    rate: u32,
    prod: &mut Producer<f32>,
    ctl: &RunCtl,
    started: &mut dyn FnMut(Started),
) -> Result<(), String> {
    com();
    let en = DeviceEnumerator::new().map_err(describe)?;
    let dev = match device {
        Some(id) => en.get_device(id).map_err(|_| "the selected microphone is not connected".to_string())?,
        None => {
            let def = en
                .get_default_device_for_role(&Direction::Capture, &Role::Console)
                .map_err(|_| "no microphone found".to_string())?;
            let def_id = def.get_id().unwrap_or_default();
            if avoid.contains(&def_id) {
                // The Windows default microphone is the virtual mic itself; use the first real one.
                let coll = en.get_device_collection(&Direction::Capture).map_err(describe)?;
                let mut pick = None;
                for d in &coll {
                    let Ok(d) = d else { continue };
                    if !avoid.contains(&d.get_id().unwrap_or_default()) {
                        pick = Some(d);
                        break;
                    }
                }
                pick.ok_or("no physical microphone found (the Windows default microphone is the virtual cable)")?
            } else {
                def
            }
        }
    };
    let name = dev.get_friendlyname().unwrap_or_else(|_| "microphone".into());
    let stream = match open_stream(&dev, Direction::Capture, Some(rate), 1) {
        Ok(s) => s,
        Err(_) => open_stream(&dev, Direction::Capture, Some(rate), 2).map_err(|e| format!("{name}: {e}"))?,
    };
    let client = &stream.client;
    let h = client.set_get_eventhandle().map_err(describe)?;
    let cc = client.get_audiocaptureclient().map_err(describe)?;
    let ch = stream.channels;
    let mut bytes = vec![0u8; client.get_buffer_size().map_err(describe)? as usize * ch * 4 * 2];
    client.start_stream().map_err(|e| format!("{name}: {}", describe(e)))?;
    started(Started { name: name.clone(), rate: stream.rate, period_frames: stream.period_frames });
    let mut silent_waits = 0u32;
    let result: Result<(), String> = 'outer: loop {
        if ctl.should_stop() {
            break Ok(());
        }
        if h.wait_for_event(500).is_err() {
            // Some drivers stop signalling when unplugged; give it a moment before giving up.
            silent_waits += 1;
            if silent_waits >= 4 {
                break Err(format!("{name}: the microphone stopped delivering audio (disconnected?)"));
            }
            continue;
        }
        silent_waits = 0;
        loop {
            let n = match cc.get_next_packet_size() {
                Ok(Some(n)) => n as usize,
                Ok(None) => 0,
                Err(e) => break 'outer Err(format!("{name}: {}", describe(e))),
            };
            if n == 0 {
                break;
            }
            let need = n * ch * 4;
            if bytes.len() < need {
                bytes.resize(need, 0);
            }
            let (got, info) = match cc.read_from_device(&mut bytes[..need]) {
                Ok(r) => r,
                Err(e) => break 'outer Err(format!("{name}: {}", describe(e))),
            };
            let got = got as usize;
            let k = got.min(prod.slots());
            if let Ok(mut chunk) = prod.write_chunk(k) {
                let (a, b) = chunk.as_mut_slices();
                for (i, s) in a.iter_mut().chain(b.iter_mut()).enumerate() {
                    *s = if info.flags.silent {
                        0.0
                    } else if ch == 1 {
                        f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
                    } else {
                        let l = f32::from_le_bytes(bytes[i * 8..i * 8 + 4].try_into().unwrap());
                        let r = f32::from_le_bytes(bytes[i * 8 + 4..i * 8 + 8].try_into().unwrap());
                        0.5 * (l + r)
                    };
                }
                chunk.commit_all();
            }
        }
    };
    let _ = client.stop_stream();
    result
}

struct Watch {
    _reg: wasapi::DeviceEventRegistration,
}
// SAFETY: the registration wraps free-threaded MMDevice COM objects (MTA).
unsafe impl Send for Watch {}

pub fn watch(cb: ChangeCallback) -> Option<Box<dyn std::any::Any + Send>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        com();
        let reg = (|| {
            let en = DeviceEnumerator::new().ok()?;
            let mut cbs = DeviceEventCallbacks::new();
            let c1: ChangeCallback = Arc::clone(&cb);
            cbs.set_device_added_callback(move |_| c1(DeviceChange::Topology));
            let c2 = Arc::clone(&cb);
            cbs.set_device_removed_callback(move |_| c2(DeviceChange::Topology));
            let c3 = Arc::clone(&cb);
            cbs.set_device_state_callback(move |_, _| c3(DeviceChange::Topology));
            let c4 = Arc::clone(&cb);
            cbs.set_default_device_callback(move |dir, role, _| {
                if matches!(role, Role::Console) {
                    match dir {
                        Direction::Capture => c4(DeviceChange::DefaultInput),
                        Direction::Render => c4(DeviceChange::DefaultOutput),
                    }
                }
            });
            en.register_notification_callback(cbs).ok()
        })();
        let _ = tx.send(reg.map(|r| Watch { _reg: r }));
    });
    let w = rx.recv().ok().flatten()?;
    Some(Box::new(w))
}

pub fn cleanup() {}
