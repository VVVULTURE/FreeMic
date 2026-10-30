//! The audio engine: owns the decoder thread and the supervisor threads that
//! keep the output (virtual microphone), capture (physical microphone) and
//! optional monitor streams running, restarting them when devices change or
//! disappear.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::backend::{Backend, BackendKind, DeviceChange, DeviceInfo, RunCtl, Started, VirtualMicInfo};
use super::drift::DriftResampler;
use super::mixer::{self, MicFeed, Mixer, MixerMsg};
use super::player::{Player, PlayerCmd};
use super::Shared;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum DevStatus {
    #[default]
    Off,
    Starting,
    Running { name: String, rate: u32, latency_ms: f32 },
    Error(String),
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub output: DevStatus,
    pub mic: DevStatus,
    pub monitor: DevStatus,
    pub virtual_mic: VirtualMicInfo,
    /// Informational notice about the microphone choice (e.g. a fallback was used).
    pub mic_notice: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct EngineConfig {
    /// Physical microphone id; `None` = system default.
    pub mic_device: Option<String>,
    pub mic_enabled: bool,
    /// Device to write the mix into; `None` = auto-detect the virtual microphone.
    pub output_device: Option<String>,
    /// Device for local monitoring of the soundboard; `None` = off.
    pub monitor_device: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NowPlaying {
    pub generation: u64,
    pub sound_id: u64,
    pub looped: bool,
}

pub type Notify = Arc<dyn Fn() + Send + Sync>;

struct Signals {
    stop: Arc<AtomicBool>,
    restart_out: Arc<AtomicU64>,
    restart_mic: Arc<AtomicU64>,
    restart_mon: Arc<AtomicU64>,
    /// Bumped on any change; supervisors sleeping after an error wake on it.
    wake: (Mutex<u64>, Condvar),
}

impl Signals {
    fn poke(&self) {
        let mut g = self.wake.0.lock().unwrap();
        *g += 1;
        self.wake.1.notify_all();
    }
    fn wait(&self, seen: u64, timeout: Duration) -> u64 {
        let g = self.wake.0.lock().unwrap();
        let (g, _) = self
            .wake
            .1
            .wait_timeout_while(g, timeout, |v| *v == seen && !self.stop.load(Ordering::Relaxed))
            .unwrap();
        *g
    }
    fn seen(&self) -> u64 {
        *self.wake.0.lock().unwrap()
    }
}

pub struct Engine {
    pub shared: Arc<Shared>,
    backend: Arc<Backend>,
    player_tx: Sender<PlayerCmd>,
    player_thread: Option<std::thread::Thread>,
    cfg: Arc<Mutex<EngineConfig>>,
    status: Arc<Mutex<Status>>,
    signals: Arc<Signals>,
    threads: Mutex<Vec<JoinHandle<()>>>,
    now_playing: Mutex<Option<NowPlaying>>,
    errors: Arc<Mutex<Vec<String>>>,
    _watch: Mutex<Option<Box<dyn std::any::Any + Send>>>,
}

const RETRY: Duration = Duration::from_secs(3);

impl Engine {
    pub fn start(kind: BackendKind, cfg: EngineConfig, shared: Arc<Shared>, notify: Notify) -> Engine {
        let backend = Arc::new(Backend::new(kind));
        let signals = Arc::new(Signals {
            stop: Arc::new(AtomicBool::new(false)),
            restart_out: Arc::new(AtomicU64::new(0)),
            restart_mic: Arc::new(AtomicU64::new(0)),
            restart_mon: Arc::new(AtomicU64::new(0)),
            wake: (Mutex::new(0), Condvar::new()),
        });
        let cfg = Arc::new(Mutex::new(cfg));
        let status = Arc::new(Mutex::new(Status::default()));
        let errors = Arc::new(Mutex::new(Vec::new()));

        let (sound_tx, sound_rx) = mixer::sound_queue();
        let (mixer_tx, mixer_rx) = mpsc::channel::<MixerMsg>();
        let (player_tx, player_rx) = mpsc::channel::<PlayerCmd>();

        let mut threads = Vec::new();

        // Decoder thread.
        let err_sink = {
            let errors = errors.clone();
            let notify = notify.clone();
            Arc::new(move |msg: String| {
                log::warn!("{msg}");
                errors.lock().unwrap().push(msg);
                notify();
            })
        };
        let player = Player::new(shared.clone(), player_rx, sound_tx, err_sink);
        let h = std::thread::Builder::new().name("freemic-decoder".into()).spawn(move || player.run()).unwrap();
        let player_thread = Some(h.thread().clone());
        threads.push(h);

        // Output supervisor (owns the mixer).
        {
            let (backend, shared, signals, status, cfg, notify) =
                (backend.clone(), shared.clone(), signals.clone(), status.clone(), cfg.clone(), notify.clone());
            let mut mixer = Mixer::new(shared.clone(), sound_rx, mixer_rx, shared.out_rate.load(Ordering::Relaxed));
            threads.push(
                std::thread::Builder::new()
                    .name("freemic-output".into())
                    .spawn(move || output_supervisor(backend, shared, signals, status, cfg, notify, &mut mixer))
                    .unwrap(),
            );
        }
        // Capture supervisor.
        {
            let (backend, shared, signals, status, cfg, notify, tx) =
                (backend.clone(), shared.clone(), signals.clone(), status.clone(), cfg.clone(), notify.clone(), mixer_tx.clone());
            threads.push(
                std::thread::Builder::new()
                    .name("freemic-capture".into())
                    .spawn(move || capture_supervisor(backend, shared, signals, status, cfg, notify, tx))
                    .unwrap(),
            );
        }
        // Monitor supervisor.
        {
            let (backend, shared, signals, status, cfg, notify, tx) =
                (backend.clone(), shared.clone(), signals.clone(), status.clone(), cfg.clone(), notify.clone(), mixer_tx);
            threads.push(
                std::thread::Builder::new()
                    .name("freemic-monitor".into())
                    .spawn(move || monitor_supervisor(backend, shared, signals, status, cfg, notify, tx))
                    .unwrap(),
            );
        }

        // FREEMIC_STATS=1: log mic buffer/drift statistics twice a second (diagnostics only).
        if std::env::var_os("FREEMIC_STATS").is_some() {
            let (shared, signals) = (shared.clone(), signals.clone());
            std::thread::spawn(move || {
                let mut tick = 0u32;
                let mut seen_underruns = 0;
                while !signals.stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(10));
                    let u = shared.sound_underruns.load(Ordering::Relaxed);
                    if u != seen_underruns {
                        seen_underruns = u;
                        log::info!(
                            "stats: soundboard underrun #{u}: target gen {} ready {} audible {} finished {} position {}",
                            shared.target_gen.load(Ordering::Relaxed),
                            shared.ready_gen.load(Ordering::Relaxed),
                            shared.audible_gen.load(Ordering::Relaxed),
                            shared.finished_gen.load(Ordering::Relaxed),
                            shared.position_frames.load(Ordering::Relaxed),
                        );
                    }
                    tick += 1;
                    if tick % 50 != 0 {
                        continue;
                    }
                    log::info!(
                        "stats: mic fill {} goal {} ratio {} ppm skips {} mic_underruns {} sound_underruns {}",
                        shared.mic_fill_frames.load(Ordering::Relaxed),
                        shared.mic_goal_frames.load(Ordering::Relaxed),
                        shared.mic_ratio_ppm.load(Ordering::Relaxed),
                        shared.mic_skips.load(Ordering::Relaxed),
                        shared.mic_underruns.load(Ordering::Relaxed),
                        shared.sound_underruns.load(Ordering::Relaxed),
                    );
                }
            });
        }

        // Device change notifications restart whatever is affected.
        let watch = {
            let signals = signals.clone();
            let cfg = cfg.clone();
            backend.watch(Arc::new(move |change: DeviceChange| {
                let c = cfg.lock().unwrap().clone();
                match change {
                    DeviceChange::DefaultInput if c.mic_device.is_none() => {
                        signals.restart_mic.fetch_add(1, Ordering::AcqRel);
                    }
                    DeviceChange::Topology => {}
                    _ => {}
                }
                signals.poke();
            }))
        };

        Engine {
            shared,
            backend,
            player_tx,
            player_thread,
            cfg,
            status,
            signals,
            threads: Mutex::new(threads),
            now_playing: Mutex::new(None),
            errors,
            _watch: Mutex::new(watch),
        }
    }

    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    pub fn list_inputs(&self) -> Vec<DeviceInfo> {
        self.backend.list_inputs().unwrap_or_default()
    }

    pub fn list_outputs(&self) -> Vec<DeviceInfo> {
        self.backend.list_outputs().unwrap_or_default()
    }

    /// Play a sound, replacing whatever is currently playing.
    pub fn play(&self, sound_id: u64, path: PathBuf, looped: bool) {
        let generation = self.shared.target_gen.fetch_add(1, Ordering::AcqRel) + 1;
        *self.now_playing.lock().unwrap() = Some(NowPlaying { generation, sound_id, looped });
        let _ = self.player_tx.send(PlayerCmd::Play { generation, path, looped });
        if let Some(t) = &self.player_thread {
            t.unpark();
        }
    }

    /// Stop the soundboard immediately (the microphone keeps running).
    pub fn stop_sound(&self) {
        let generation = self.shared.target_gen.fetch_add(1, Ordering::AcqRel) + 1;
        self.shared.finished_gen.fetch_max(generation, Ordering::AcqRel);
        *self.now_playing.lock().unwrap() = None;
        if let Some(t) = &self.player_thread {
            t.unpark();
        }
    }

    /// The sound that is currently playing, with its position in seconds.
    pub fn now_playing(&self) -> Option<(NowPlaying, f64)> {
        let mut guard = self.now_playing.lock().unwrap();
        let np = guard.clone()?;
        if self.shared.finished_gen.load(Ordering::Acquire) >= np.generation {
            *guard = None;
            return None;
        }
        let pos = if self.shared.audible_gen.load(Ordering::Relaxed) == np.generation {
            self.shared.position_frames.load(Ordering::Relaxed) as f64 / self.shared.out_rate.load(Ordering::Relaxed) as f64
        } else {
            0.0
        };
        Some((np, pos))
    }

    pub fn config(&self) -> EngineConfig {
        self.cfg.lock().unwrap().clone()
    }

    pub fn set_config(&self, new: EngineConfig) {
        let old = std::mem::replace(&mut *self.cfg.lock().unwrap(), new.clone());
        if old.output_device != new.output_device {
            self.signals.restart_out.fetch_add(1, Ordering::AcqRel);
        }
        if old.mic_device != new.mic_device || old.mic_enabled != new.mic_enabled || old.output_device != new.output_device {
            self.signals.restart_mic.fetch_add(1, Ordering::AcqRel);
        }
        if old.monitor_device != new.monitor_device {
            self.signals.restart_mon.fetch_add(1, Ordering::AcqRel);
        }
        self.signals.poke();
    }

    /// Ask every stream to reopen (e.g. after the user installed a driver).
    pub fn rescan(&self) {
        self.signals.restart_out.fetch_add(1, Ordering::AcqRel);
        self.signals.restart_mic.fetch_add(1, Ordering::AcqRel);
        self.signals.restart_mon.fetch_add(1, Ordering::AcqRel);
        self.signals.poke();
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut *self.errors.lock().unwrap())
    }

    /// Stop all streams and release devices. Safe to call more than once.
    pub fn shutdown(&self) {
        if self.signals.stop.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = self.player_tx.send(PlayerCmd::Shutdown);
        if let Some(t) = &self.player_thread {
            t.unpark();
        }
        self.signals.poke();
        let threads: Vec<_> = self.threads.lock().unwrap().drain(..).collect();
        for t in threads {
            let _ = t.join();
        }
        self.backend.cleanup();
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn set_status(status: &Mutex<Status>, notify: &Notify, f: impl FnOnce(&mut Status)) {
    {
        let mut s = status.lock().unwrap();
        f(&mut s);
    }
    notify();
}

fn output_supervisor(
    backend: Arc<Backend>,
    shared: Arc<Shared>,
    signals: Arc<Signals>,
    status: Arc<Mutex<Status>>,
    cfg: Arc<Mutex<EngineConfig>>,
    notify: Notify,
    mixer: &mut Mixer,
) {
    super::backend::promote_audio_thread();
    while !signals.stop.load(Ordering::Relaxed) {
        let seen = signals.seen();
        let chosen = cfg.lock().unwrap().output_device.clone();
        let vm = backend.virtual_mic(chosen.as_deref());
        set_status(&status, &notify, |s| {
            s.virtual_mic = vm.clone();
            s.output = DevStatus::Starting;
        });
        if vm.feed.is_none() {
            let msg = vm.problem.clone().unwrap_or_else(|| "No virtual microphone device found".into());
            set_status(&status, &notify, |s| s.output = DevStatus::Error(msg));
            signals.wait(seen, RETRY);
            continue;
        }
        let ctl = RunCtl::new(signals.stop.clone(), signals.restart_out.clone());
        let mut on_start = |st: Started, mixer: &mut Mixer| {
            let old_rate = shared.out_rate.swap(st.rate, Ordering::AcqRel);
            if old_rate != st.rate || mixer.rate() != st.rate {
                // Sound queued at the old rate would play at the wrong speed: stop it.
                let g = shared.target_gen.fetch_add(1, Ordering::AcqRel) + 1;
                shared.finished_gen.fetch_max(g, Ordering::AcqRel);
                mixer.set_rate(st.rate);
                signals.restart_mic.fetch_add(1, Ordering::AcqRel);
                signals.restart_mon.fetch_add(1, Ordering::AcqRel);
                signals.poke();
            }
            let latency_ms = st.period_frames as f32 * 1000.0 / st.rate as f32;
            log::info!("output started: {} @ {} Hz, period {} frames", st.name, st.rate, st.period_frames);
            set_status(&status, &notify, |s| {
                s.output = DevStatus::Running { name: st.name.clone(), rate: st.rate, latency_ms }
            });
        };
        let result = backend.run_output(&vm, mixer, &ctl, &mut on_start);
        match result {
            Ok(()) => set_status(&status, &notify, |s| s.output = DevStatus::Off),
            Err(e) => {
                log::warn!("output stream error: {e}");
                set_status(&status, &notify, |s| s.output = DevStatus::Error(e));
                signals.wait(seen, RETRY);
            }
        }
    }
}

fn capture_supervisor(
    backend: Arc<Backend>,
    shared: Arc<Shared>,
    signals: Arc<Signals>,
    status: Arc<Mutex<Status>>,
    cfg: Arc<Mutex<EngineConfig>>,
    notify: Notify,
    mixer_tx: Sender<MixerMsg>,
) {
    super::backend::promote_audio_thread();
    while !signals.stop.load(Ordering::Relaxed) {
        let seen = signals.seen();
        let c = cfg.lock().unwrap().clone();
        if !c.mic_enabled {
            let _ = mixer_tx.send(MixerMsg::Mic(None));
            set_status(&status, &notify, |s| s.mic = DevStatus::Off);
            signals.wait(seen, Duration::from_secs(3600));
            continue;
        }
        let vm = backend.virtual_mic(c.output_device.as_deref());
        let avoid = backend.feedback_inputs(&vm);
        if let Some(dev) = &c.mic_device {
            if avoid.iter().any(|a| a == dev) {
                set_status(&status, &notify, |s| {
                    s.mic = DevStatus::Error(
                        "The selected microphone is FreeMic's own virtual microphone. Choose your real microphone to avoid a feedback loop."
                            .into(),
                    )
                });
                signals.wait(seen, Duration::from_secs(3600));
                continue;
            }
        }
        let rate = shared.out_rate.load(Ordering::Acquire);
        set_status(&status, &notify, |s| s.mic = DevStatus::Starting);
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(rate as usize); // 1 s of headroom
        let ctl = RunCtl::new(signals.stop.clone(), signals.restart_mic.clone());
        let tx = mixer_tx.clone();
        let mut cons_slot = Some(cons);
        let mut on_start = |st: Started| {
            // Keep enough buffered to ride out one capture period plus one output period of jitter.
            let target = (st.period_frames + (st.rate as usize / 100)).max(st.rate as usize / 100);
            if let Some(consumer) = cons_slot.take() {
                let resampler = DriftResampler::new(1, st.rate, rate, target);
                let _ = tx.send(MixerMsg::Mic(Some(MicFeed { consumer, resampler })));
            }
            let latency_ms = target as f32 * 1000.0 / st.rate as f32;
            log::info!("mic started: {} @ {} Hz, period {} frames", st.name, st.rate, st.period_frames);
            set_status(&status, &notify, |s| {
                s.mic = DevStatus::Running { name: st.name.clone(), rate: st.rate, latency_ms };
                s.mic_notice = None;
            });
        };
        let result = backend.run_capture(c.mic_device.as_deref(), &avoid, rate, &mut prod, &ctl, &mut on_start);
        let _ = mixer_tx.send(MixerMsg::Mic(None));
        match result {
            Ok(()) => set_status(&status, &notify, |s| s.mic = DevStatus::Off),
            Err(e) => {
                log::warn!("mic stream error: {e}");
                set_status(&status, &notify, |s| s.mic = DevStatus::Error(e));
                signals.wait(seen, RETRY);
            }
        }
    }
}

fn monitor_supervisor(
    backend: Arc<Backend>,
    shared: Arc<Shared>,
    signals: Arc<Signals>,
    status: Arc<Mutex<Status>>,
    cfg: Arc<Mutex<EngineConfig>>,
    notify: Notify,
    mixer_tx: Sender<MixerMsg>,
) {
    while !signals.stop.load(Ordering::Relaxed) {
        let seen = signals.seen();
        let device = cfg.lock().unwrap().monitor_device.clone();
        let Some(device) = device else {
            let _ = mixer_tx.send(MixerMsg::Monitor(None));
            set_status(&status, &notify, |s| s.monitor = DevStatus::Off);
            signals.wait(seen, Duration::from_secs(3600));
            continue;
        };
        let rate = shared.out_rate.load(Ordering::Acquire);
        let (prod, mut cons) = rtrb::RingBuffer::<f32>::new(rate as usize); // 0.5 s stereo
        let _ = mixer_tx.send(MixerMsg::Monitor(Some(prod)));
        set_status(&status, &notify, |s| s.monitor = DevStatus::Starting);
        let ctl = RunCtl::new(signals.stop.clone(), signals.restart_mon.clone());
        let mut on_start = |st: Started| {
            set_status(&status, &notify, |s| {
                s.monitor = DevStatus::Running {
                    name: st.name.clone(),
                    rate: st.rate,
                    latency_ms: st.period_frames as f32 * 1000.0 / st.rate as f32,
                }
            });
        };
        let result = backend.run_monitor(&device, rate, &mut cons, &ctl, &mut on_start);
        let _ = mixer_tx.send(MixerMsg::Monitor(None));
        match result {
            Ok(()) => set_status(&status, &notify, |s| s.monitor = DevStatus::Off),
            Err(e) => {
                log::warn!("monitor stream error: {e}");
                set_status(&status, &notify, |s| s.monitor = DevStatus::Error(e));
                signals.wait(seen, RETRY);
            }
        }
    }
}
