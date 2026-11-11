//! arui-system: master volume and basic network state, both event driven.
//!
//! # Threading model
//! * `start` spawns one worker thread (`arui-system`) in the COM multithreaded apartment. It
//!   owns the `IMMDeviceEnumerator`, the current default render endpoint's
//!   `IAudioEndpointVolume`, and the WinRT `NetworkStatusChanged` registration. Every call into
//!   those objects (including `set_volume` / `set_muted`, which are forwarded to it) happens on
//!   that thread, so callers do not need COM initialized.
//! * `IAudioEndpointVolumeCallback::OnNotify` runs on an audio service thread. It only reads
//!   the notification payload, updates the cached state and emits `SystemEvent::Volume` when
//!   the state changed. It never calls back into the audio API.
//! * `IMMNotificationClient::OnDefaultDeviceChanged` and `NetworkStatusChanged` run on system
//!   threads and only post a message to the worker, which re-binds the volume callback to the
//!   new default device or re-reads the network state (network bursts are coalesced with a
//!   250 ms debounce). The worker sleeps on its channel otherwise: no polling.
//! * `volume()` / `network()` read a mutex-protected cache. Dropping `System` stops the worker,
//!   which unregisters every callback and event token.

mod logic;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arui_win::{EventSink, NativeEvent, NetworkKind, NetworkState, SystemEvent, VolumeState};
use parking_lot::Mutex;
use windows::Networking::Connectivity::{
    ConnectionProfile, NetworkInformation, NetworkStatusChangedEventHandler,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows_core::{implement, GUID, PCWSTR};

pub use logic::{clamp_level, derive_network, RawNetwork};

const NETWORK_DEBOUNCE: Duration = Duration::from_millis(250);
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Event context passed on our own volume changes (lets tools tell them apart from others).
pub const ARUI_VOLUME_CONTEXT: GUID = GUID::from_u128(0x7a2f_3c1e_6b4d_4e8a_9f10_a8c2_57d3_e901);

enum Msg {
    DefaultDeviceChanged,
    NetworkChanged,
    SetVolume(f32, Sender<arui_win::Result<()>>),
    SetMuted(bool, Sender<arui_win::Result<()>>),
    Stop,
}

struct Shared {
    sink: EventSink,
    volume: Mutex<VolumeState>,
    network: Mutex<NetworkState>,
    /// Incremented on every re-bind; callbacks of older endpoints are ignored.
    generation: AtomicU64,
    /// Set once the initial events went out; before that, changes are only cached.
    started: AtomicBool,
}

impl Shared {
    fn apply_volume(&self, v: VolumeState) {
        let changed = {
            let mut cur = self.volume.lock();
            let changed = *cur != v;
            *cur = v;
            changed
        };
        if changed && self.started.load(Ordering::SeqCst) {
            (self.sink)(NativeEvent::System(SystemEvent::Volume(v)));
        }
    }

    fn apply_network(&self, n: NetworkState) {
        let changed = {
            let mut cur = self.network.lock();
            let changed = *cur != n;
            *cur = n.clone();
            changed
        };
        if changed && self.started.load(Ordering::SeqCst) {
            (self.sink)(NativeEvent::System(SystemEvent::Network(n)));
        }
    }
}

/// Handle to the system service. Stops the worker on drop.
pub struct System {
    tx: Sender<Msg>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

/// Starts the service. Emits the initial `SystemEvent::Volume` and `SystemEvent::Network`
/// before returning, then one event per actual change.
pub fn start(sink: EventSink) -> arui_win::Result<System> {
    let shared = Arc::new(Shared {
        sink,
        volume: Mutex::new(VolumeState { level: 0.0, muted: true }),
        network: Mutex::new(NetworkState {
            connected: false,
            internet: false,
            kind: NetworkKind::None,
            name: None,
        }),
        generation: AtomicU64::new(0),
        started: AtomicBool::new(false),
    });
    let (tx, rx) = mpsc::channel();
    let (init_tx, init_rx) = mpsc::channel::<arui_win::Result<()>>();
    let worker_tx = tx.clone();
    let worker_shared = shared.clone();
    let thread = std::thread::Builder::new()
        .name("arui-system".into())
        .spawn(move || {
            let _com = Mta::init();
            let mut w = match Worker::new(worker_shared, worker_tx) {
                Ok(w) => w,
                Err(e) => {
                    let _ = init_tx.send(Err(e));
                    return;
                }
            };
            // Load the initial state silently, then always emit it once.
            w.bind_default_device();
            w.shared.apply_network(read_network());
            w.shared.started.store(true, Ordering::SeqCst);
            let v = *w.shared.volume.lock();
            let n = w.shared.network.lock().clone();
            (w.shared.sink)(NativeEvent::System(SystemEvent::Volume(v)));
            (w.shared.sink)(NativeEvent::System(SystemEvent::Network(n)));
            let _ = init_tx.send(Ok(()));
            w.run(rx);
            w.shutdown();
        })
        .map_err(|e| arui_win::Error::Other(e.to_string()))?;

    match init_rx.recv() {
        Ok(Ok(())) => Ok(System { tx, shared, thread: Some(thread) }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            Err(arui_win::Error::Other("system worker died during init".into()))
        }
    }
}

impl System {
    /// Master volume of the default console render device. `{ level: 0, muted: true }` when
    /// there is no audio device.
    pub fn volume(&self) -> VolumeState {
        *self.shared.volume.lock()
    }

    /// Sets the master volume (clamped to 0..=1).
    pub fn set_volume(&self, level: f32) -> arui_win::Result<()> {
        self.call(|r| Msg::SetVolume(clamp_level(level), r))
    }

    pub fn set_muted(&self, muted: bool) -> arui_win::Result<()> {
        self.call(|r| Msg::SetMuted(muted, r))
    }

    pub fn network(&self) -> NetworkState {
        self.shared.network.lock().clone()
    }

    fn call(&self, f: impl FnOnce(Sender<arui_win::Result<()>>) -> Msg) -> arui_win::Result<()> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(f(reply))
            .map_err(|_| arui_win::Error::Other("system worker stopped".into()))?;
        rx.recv_timeout(CALL_TIMEOUT)
            .map_err(|_| arui_win::Error::Other("system call timed out".into()))?
    }
}

impl Drop for System {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Mta(bool);
impl Mta {
    fn init() -> Self {
        Mta(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}
impl Drop for Mta {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

// ------------------------------------------------------------------ COM callbacks

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    shared: Arc<Shared>,
    generation: u64,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows_core::Result<()> {
        if data.is_null() || self.shared.generation.load(Ordering::SeqCst) != self.generation {
            return Ok(());
        }
        let d = unsafe { &*data };
        self.shared.apply_volume(VolumeState {
            level: clamp_level(d.fMasterVolume),
            muted: d.bMuted.as_bool(),
        });
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceClient {
    tx: Sender<Msg>,
}

impl IMMNotificationClient_Impl for DeviceClient_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows_core::Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> windows_core::Result<()> {
        // Must not call into the enumerator from here; hand it to the worker.
        if flow == eRender && role == eConsole {
            let _ = self.tx.send(Msg::DefaultDeviceChanged);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows_core::Result<()> {
        Ok(())
    }
}

// ------------------------------------------------------------------ worker

struct Endpoint {
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

struct Worker {
    shared: Arc<Shared>,
    enumerator: IMMDeviceEnumerator,
    device_client: IMMNotificationClient,
    endpoint: Option<Endpoint>,
    network_token: Option<i64>,
}

impl Worker {
    fn new(shared: Arc<Shared>, tx: Sender<Msg>) -> arui_win::Result<Self> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let device_client: IMMNotificationClient = DeviceClient { tx: tx.clone() }.into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&device_client)? };

        // Network monitoring is optional: volume still works if this fails.
        let ntx = tx.clone();
        let network_token = NetworkInformation::NetworkStatusChanged(
            &NetworkStatusChangedEventHandler::new(move |_| {
                let _ = ntx.send(Msg::NetworkChanged);
                Ok(())
            }),
        )
        .map_err(|e| log::warn!("NetworkStatusChanged registration failed: {e}"))
        .ok();

        Ok(Self { shared, enumerator, device_client, endpoint: None, network_token })
    }

    fn run(&mut self, rx: Receiver<Msg>) {
        let mut network_deadline: Option<Instant> = None;
        loop {
            let msg = match network_deadline {
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(d) => rx.recv_timeout(d.saturating_duration_since(Instant::now())),
            };
            match msg {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Msg::DefaultDeviceChanged) => self.bind_default_device(),
                Ok(Msg::NetworkChanged) => {
                    network_deadline.get_or_insert_with(|| Instant::now() + NETWORK_DEBOUNCE);
                }
                Ok(Msg::SetVolume(level, reply)) => {
                    let r = self.with_endpoint(|v| unsafe {
                        v.SetMasterVolumeLevelScalar(level, &ARUI_VOLUME_CONTEXT)
                    });
                    let _ = reply.send(r);
                }
                Ok(Msg::SetMuted(muted, reply)) => {
                    let r = self.with_endpoint(|v| unsafe { v.SetMute(muted, &ARUI_VOLUME_CONTEXT) });
                    let _ = reply.send(r);
                }
                Err(RecvTimeoutError::Timeout) => {
                    network_deadline = None;
                    self.shared.apply_network(read_network());
                }
            }
        }
    }

    /// Runs a setter on the current endpoint, then refreshes the cached state right away so
    /// `volume()` is correct even before the callback arrives.
    fn with_endpoint(
        &self,
        f: impl FnOnce(&IAudioEndpointVolume) -> windows_core::Result<()>,
    ) -> arui_win::Result<()> {
        let ep = self
            .endpoint
            .as_ref()
            .ok_or_else(|| arui_win::Error::NotFound("no default audio device".into()))?;
        f(&ep.volume)?;
        if let Some(v) = read_volume(&ep.volume) {
            self.shared.apply_volume(v);
        }
        Ok(())
    }

    fn unbind(&mut self) {
        if let Some(ep) = self.endpoint.take() {
            let _ = unsafe { ep.volume.UnregisterControlChangeNotify(&ep.callback) };
        }
    }

    /// (Re-)binds the volume callback to the current default console render device and emits
    /// its state.
    fn bind_default_device(&mut self) {
        self.unbind();
        let generation = self.shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let bound = (|| -> windows_core::Result<Endpoint> {
            let device = unsafe { self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
            let volume: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None)? };
            let callback: IAudioEndpointVolumeCallback =
                VolumeCallback { shared: self.shared.clone(), generation }.into();
            unsafe { volume.RegisterControlChangeNotify(&callback)? };
            Ok(Endpoint { volume, callback })
        })();
        match bound {
            Ok(ep) => {
                if let Some(v) = read_volume(&ep.volume) {
                    self.shared.apply_volume(v);
                }
                self.endpoint = Some(ep);
            }
            Err(e) => {
                log::info!("no default audio render device: {e}");
                self.shared.apply_volume(VolumeState { level: 0.0, muted: true });
            }
        }
    }

    fn shutdown(&mut self) {
        // Invalidate in-flight volume callbacks first.
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.unbind();
        let _ = unsafe { self.enumerator.UnregisterEndpointNotificationCallback(&self.device_client) };
        if let Some(t) = self.network_token.take() {
            let _ = NetworkInformation::RemoveNetworkStatusChanged(t);
        }
    }
}

fn read_volume(v: &IAudioEndpointVolume) -> Option<VolumeState> {
    unsafe {
        let level = v.GetMasterVolumeLevelScalar().ok()?;
        let muted = v.GetMute().ok()?.as_bool();
        Some(VolumeState { level: clamp_level(level), muted })
    }
}

fn read_profile(p: &ConnectionProfile) -> RawNetwork {
    let level = p.GetNetworkConnectivityLevel().map(|l| l.0).unwrap_or(logic::LEVEL_NONE);
    RawNetwork {
        has_profile: true,
        level,
        is_wlan: p.IsWlanConnectionProfile().unwrap_or(false),
        is_wwan: p.IsWwanConnectionProfile().unwrap_or(false),
        iana_type: p.NetworkAdapter().and_then(|a| a.IanaInterfaceType()).ok(),
        name: p.ProfileName().ok().map(|s| s.to_string()),
    }
}

/// Reads the network state: the internet profile if there is one, otherwise the best
/// connected (local-only) profile.
fn read_network() -> NetworkState {
    if let Ok(p) = NetworkInformation::GetInternetConnectionProfile() {
        let raw = read_profile(&p);
        if raw.level >= logic::LEVEL_LOCAL {
            return derive_network(&raw);
        }
    }
    let best = NetworkInformation::GetConnectionProfiles()
        .ok()
        .into_iter()
        .flatten()
        .map(|p| read_profile(&p))
        .filter(|r| r.level >= logic::LEVEL_LOCAL)
        .max_by_key(|r| r.level);
    derive_network(&best.unwrap_or_default())
}
