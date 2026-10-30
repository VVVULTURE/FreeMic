//! Platform audio backends.
//!
//! Every backend provides the same small set of operations: list devices,
//! locate the virtual microphone, and run the output / capture / monitor
//! streams until told to stop. The `null` backend touches no audio hardware
//! at all; it exists so the whole application can be tested in isolation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rtrb::{Consumer, Producer};

use super::mixer::Mixer;

pub mod null;
#[cfg(windows)]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

/// What the app knows about the virtual microphone on this system.
#[derive(Clone, Debug, Default)]
pub struct VirtualMicInfo {
    /// Device FreeMic writes the mix to (e.g. "CABLE Input"), if found.
    pub feed: Option<DeviceInfo>,
    /// Name other applications should select as their microphone.
    pub mic_name: Option<String>,
    /// Human readable explanation when no virtual microphone is available.
    pub problem: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    /// The native audio system (WASAPI on Windows, PulseAudio/PipeWire on Linux).
    Native,
    /// No devices: deterministic, clock-driven, for tests.
    Null,
}

/// Information reported by a stream once it is running.
#[derive(Clone, Debug)]
pub struct Started {
    pub name: String,
    pub rate: u32,
    /// Device period / buffer size in frames.
    pub period_frames: usize,
}

/// Tells a running stream when to stop (shutdown or reconfiguration).
pub struct RunCtl {
    global: Arc<AtomicBool>,
    restart: Arc<AtomicU64>,
    started_at: u64,
}

impl RunCtl {
    pub fn new(global: Arc<AtomicBool>, restart: Arc<AtomicU64>) -> Self {
        let started_at = restart.load(Ordering::Acquire);
        Self { global, restart, started_at }
    }
    pub fn should_stop(&self) -> bool {
        self.global.load(Ordering::Relaxed) || self.restart.load(Ordering::Relaxed) != self.started_at
    }
}

pub type ChangeCallback = Arc<dyn Fn(DeviceChange) + Send + Sync>;

#[derive(Clone, Debug)]
pub enum DeviceChange {
    /// A device appeared, vanished or changed state.
    Topology,
    /// The system default capture device changed.
    DefaultInput,
    /// The system default render device changed.
    DefaultOutput,
}

/// Dispatch helpers so the engine does not need `cfg` everywhere.
///
/// For testing, single streams can be switched to the null backend with
/// `FREEMIC_TEST_MIC=null` / `FREEMIC_TEST_OUTPUT=null` (e.g. real output
/// device with a synthetic microphone).
pub struct Backend {
    pub kind: BackendKind,
    capture: BackendKind,
    output: BackendKind,
}

impl Backend {
    pub fn new(kind: BackendKind) -> Self {
        let pick = |var: &str| match std::env::var(var).as_deref() {
            Ok("null") => BackendKind::Null,
            _ => kind,
        };
        let (capture, output) = (pick("FREEMIC_TEST_MIC"), pick("FREEMIC_TEST_OUTPUT"));
        if capture != kind || output != kind {
            log::warn!("test override: capture {capture:?}, output {output:?}");
        }
        Self { kind, capture, output }
    }

    pub fn name(&self) -> &'static str {
        match self.kind {
            BackendKind::Null => "null (no audio devices)",
            BackendKind::Native => native_name(),
        }
    }

    pub fn list_inputs(&self) -> Result<Vec<DeviceInfo>, String> {
        match self.capture {
            BackendKind::Null => Ok(null::list_inputs()),
            BackendKind::Native => native::list_inputs(),
        }
    }

    pub fn list_outputs(&self) -> Result<Vec<DeviceInfo>, String> {
        match self.kind {
            BackendKind::Null => Ok(null::list_outputs()),
            BackendKind::Native => native::list_outputs(),
        }
    }

    pub fn virtual_mic(&self, chosen_feed: Option<&str>) -> VirtualMicInfo {
        match self.output {
            BackendKind::Null => null::virtual_mic(),
            BackendKind::Native => native::virtual_mic(chosen_feed),
        }
    }

    /// Microphones that must never be captured because they would feed the
    /// virtual microphone back into itself.
    pub fn feedback_inputs(&self, vm: &VirtualMicInfo) -> Vec<String> {
        match self.capture {
            BackendKind::Null => Vec::new(),
            BackendKind::Native => native::feedback_inputs(vm),
        }
    }

    pub fn run_output(
        &self,
        vm: &VirtualMicInfo,
        mixer: &mut Mixer,
        ctl: &RunCtl,
        started: &mut dyn FnMut(Started, &mut Mixer),
    ) -> Result<(), String> {
        match self.output {
            BackendKind::Null => null::run_output(mixer, ctl, started),
            BackendKind::Native => native::run_output(vm, mixer, ctl, started),
        }
    }

    pub fn run_capture(
        &self,
        device: Option<&str>,
        avoid: &[String],
        rate: u32,
        prod: &mut Producer<f32>,
        ctl: &RunCtl,
        started: &mut dyn FnMut(Started),
    ) -> Result<(), String> {
        match self.capture {
            BackendKind::Null => null::run_capture(rate, prod, ctl, started),
            BackendKind::Native => native::run_capture(device, avoid, rate, prod, ctl, started),
        }
    }

    pub fn run_monitor(
        &self,
        device: &str,
        rate: u32,
        cons: &mut Consumer<f32>,
        ctl: &RunCtl,
        started: &mut dyn FnMut(Started),
    ) -> Result<(), String> {
        match self.kind {
            BackendKind::Null => null::run_monitor(rate, cons, ctl, started),
            BackendKind::Native => native::run_monitor(device, rate, cons, ctl, started),
        }
    }

    /// Start watching for device changes. The returned guard keeps the watch alive.
    pub fn watch(&self, cb: ChangeCallback) -> Option<Box<dyn std::any::Any + Send>> {
        match self.kind {
            BackendKind::Null => None,
            BackendKind::Native => native::watch(cb),
        }
    }

    /// Release system resources (e.g. the Linux virtual devices) at shutdown.
    pub fn cleanup(&self) {
        if self.kind == BackendKind::Native {
            native::cleanup();
        }
    }
}

#[cfg(windows)]
use self::windows as native;
#[cfg(target_os = "linux")]
use self::linux as native;

fn native_name() -> &'static str {
    #[cfg(windows)]
    {
        "WASAPI"
    }
    #[cfg(target_os = "linux")]
    {
        "PulseAudio / PipeWire"
    }
}

/// Best-effort removal of system-side resources from a panic hook.
pub fn emergency_cleanup() {
    #[cfg(target_os = "linux")]
    linux::cleanup();
}

/// Raise the current thread to real-time audio priority where the platform allows it.
pub fn promote_audio_thread() {
    #[cfg(windows)]
    windows::promote_thread();
}
