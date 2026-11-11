//! arui-media: event-driven media sessions via GlobalSystemMediaTransportControls (GSMTC).
//!
//! # Threading model
//! * `start` spawns one worker thread (`arui-media`) that joins the COM multithreaded
//!   apartment (MTA). Every WinRT call, including all blocking `.join()` waits on async
//!   operations, runs on that thread.
//! * WinRT event handlers (SessionsChanged, CurrentSessionChanged and the three per-session
//!   events) run on WinRT thread pool threads. They do nothing but send a message to the
//!   worker's channel, so no callback ever blocks or re-enters WinRT.
//! * The worker coalesces per-session change bursts with a short debounce (75 ms after the
//!   last event, at most 300 ms after the first) using `recv_timeout`; it sleeps on the
//!   channel when nothing is pending, so there is no polling.
//! * `Media::snapshot` reads a mutex-protected copy; `Media::control` forwards the action to
//!   the worker and waits for the result (up to 5 s). Both are callable from any thread.
//! * Dropping `Media` stops the worker, which unregisters every event token and drops the
//!   artwork it put into the `IconStore`.

mod logic;

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arui_win::{
    EventSink, IconStore, MediaAction, MediaEvent, MediaSession, NativeEvent, PlaybackStatus,
};
use parking_lot::Mutex;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
};
use windows::Storage::Streams::{
    Buffer, DataReader, IRandomAccessStreamReference, InputStreamOptions,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows_core::{IUnknown, Interface};

pub use logic::{assign_id, datetime_to_unix_ms, ensure_png, map_status, timeline_ms};

const DEBOUNCE: Duration = Duration::from_millis(75);
const DEBOUNCE_MAX: Duration = Duration::from_millis(300);
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
/// Artwork larger than this is ignored (some apps hand out huge images).
const MAX_ART_BYTES: u64 = 16 * 1024 * 1024;

enum Msg {
    SessionsChanged,
    CurrentChanged,
    /// A per-session event fired. `props` = media properties (title/art) changed.
    Dirty { key: u64, props: bool },
    Control {
        id: Option<String>,
        action: MediaAction,
        reply: Sender<arui_win::Result<()>>,
    },
    Stop,
}

#[derive(Default)]
struct Shared {
    sessions: Vec<MediaSession>,
    current: Option<String>,
}

/// Handle to the media service. Stops the worker on drop.
pub struct Media {
    tx: Sender<Msg>,
    shared: Arc<Mutex<Shared>>,
    thread: Option<JoinHandle<()>>,
}

/// Starts watching media sessions. Emits `MediaEvent::Snapshot` before returning, then
/// `Updated` / `Removed` / `Current` as things change.
pub fn start(sink: EventSink, icons: IconStore) -> arui_win::Result<Media> {
    let (tx, rx) = mpsc::channel();
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (init_tx, init_rx) = mpsc::channel::<arui_win::Result<()>>();
    let worker_tx = tx.clone();
    let worker_shared = shared.clone();
    let thread = std::thread::Builder::new()
        .name("arui-media".into())
        .spawn(move || {
            let _com = Mta::init();
            let mut w = match Worker::new(sink, icons, worker_tx, worker_shared) {
                Ok(w) => w,
                Err(e) => {
                    let _ = init_tx.send(Err(e));
                    return;
                }
            };
            w.resync();
            w.update_current(false);
            w.emit_snapshot();
            w.started = true;
            let _ = init_tx.send(Ok(()));
            w.run(rx);
            w.shutdown();
        })
        .map_err(|e| arui_win::Error::Other(e.to_string()))?;

    match init_rx.recv() {
        Ok(Ok(())) => Ok(Media { tx, shared, thread: Some(thread) }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            Err(arui_win::Error::Other("media worker died during init".into()))
        }
    }
}

impl Media {
    /// Current sessions (in discovery order) and the id of the system's current session.
    pub fn snapshot(&self) -> (Vec<MediaSession>, Option<String>) {
        let s = self.shared.lock();
        (s.sessions.clone(), s.current.clone())
    }

    /// Sends a transport command. `id = None` targets the system's current session.
    /// Returns `Unsupported` if the app rejected the command, `NotFound` for unknown ids.
    pub fn control(&self, id: Option<&str>, action: MediaAction) -> arui_win::Result<()> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Msg::Control { id: id.map(str::to_string), action, reply })
            .map_err(|_| arui_win::Error::Other("media worker stopped".into()))?;
        rx.recv_timeout(CONTROL_TIMEOUT)
            .map_err(|_| arui_win::Error::Other("media control timed out".into()))?
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// MTA membership for the worker thread.
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

/// COM identity of an object (IUnknown pointer). Stable while we hold a reference.
fn identity<T: Interface>(obj: &T) -> usize {
    obj.cast::<IUnknown>().map(|u| u.as_raw() as usize).unwrap_or(0)
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

struct Tracked {
    key: u64,
    ident: usize,
    session: Session,
    id: String,
    /// SourceAppUserModelId (the id before disambiguation).
    app_id: String,
    tokens: [i64; 3],
    /// What we last emitted (None = never emitted yet).
    last: Option<MediaSession>,
    /// Last known media properties; kept between playback-only refreshes.
    title: String,
    artist: String,
    album: String,
    thumb: Option<IRandomAccessStreamReference>,
    thumb_ident: usize,
    art_hash: Option<u64>,
    art_n: u32,
    artwork_key: Option<String>,
}

struct Worker {
    sink: EventSink,
    icons: IconStore,
    tx: Sender<Msg>,
    shared: Arc<Mutex<Shared>>,
    manager: Manager,
    manager_tokens: [i64; 2],
    sessions: Vec<Tracked>,
    next_key: u64,
    current: Option<String>,
    /// False until the initial snapshot went out; suppresses `Updated` during the first load.
    started: bool,
}

impl Worker {
    fn new(
        sink: EventSink,
        icons: IconStore,
        tx: Sender<Msg>,
        shared: Arc<Mutex<Shared>>,
    ) -> arui_win::Result<Self> {
        let manager = Manager::RequestAsync()?.join()?;
        let t1 = tx.clone();
        let sessions_tok = manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t1.send(Msg::SessionsChanged);
            Ok(())
        }))?;
        let t2 = tx.clone();
        let current_tok = manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t2.send(Msg::CurrentChanged);
            Ok(())
        }));
        let current_tok = match current_tok {
            Ok(t) => t,
            Err(e) => {
                let _ = manager.RemoveSessionsChanged(sessions_tok);
                return Err(e.into());
            }
        };
        Ok(Self {
            sink,
            icons,
            tx,
            shared,
            manager,
            manager_tokens: [sessions_tok, current_tok],
            sessions: Vec::new(),
            next_key: 1,
            current: None,
            started: false,
        })
    }

    fn emit(&self, e: MediaEvent) {
        (self.sink)(NativeEvent::Media(e));
    }

    fn publish(&self) {
        let mut s = self.shared.lock();
        s.sessions = self.sessions.iter().filter_map(|t| t.last.clone()).collect();
        s.current = self.current.clone();
    }

    fn emit_snapshot(&self) {
        self.publish();
        let (sessions, current) = {
            let s = self.shared.lock();
            (s.sessions.clone(), s.current.clone())
        };
        self.emit(MediaEvent::Snapshot { sessions, current });
    }

    fn run(&mut self, rx: Receiver<Msg>) {
        // key -> whether media properties need re-reading
        let mut pending: HashMap<u64, bool> = HashMap::new();
        let mut first: Option<Instant> = None;
        let mut deadline: Option<Instant> = None;
        loop {
            let msg = match deadline {
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(d) => rx.recv_timeout(d.saturating_duration_since(Instant::now())),
            };
            match msg {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Msg::SessionsChanged) => {
                    self.resync();
                    self.update_current(true);
                }
                Ok(Msg::CurrentChanged) => self.update_current(true),
                Ok(Msg::Dirty { key, props }) => {
                    *pending.entry(key).or_insert(false) |= props;
                    let now = Instant::now();
                    let f = *first.get_or_insert(now);
                    deadline = Some((now + DEBOUNCE).min(f + DEBOUNCE_MAX));
                }
                Ok(Msg::Control { id, action, reply }) => {
                    let _ = reply.send(self.control(id.as_deref(), action));
                }
                Err(RecvTimeoutError::Timeout) => {
                    for (key, props) in pending.drain() {
                        if let Some(i) = self.sessions.iter().position(|t| t.key == key) {
                            self.refresh(i, props, true);
                        }
                    }
                    self.publish();
                    first = None;
                    deadline = None;
                }
            }
        }
    }

    /// Re-reads the session list: tracks new sessions, drops vanished ones.
    fn resync(&mut self) {
        let list: Vec<Session> = match self.manager.GetSessions() {
            Ok(v) => v.into_iter().collect(),
            Err(e) => {
                log::warn!("GetSessions failed: {e}");
                return;
            }
        };
        let idents: Vec<usize> = list.iter().map(identity).collect();

        // Removals.
        let mut i = 0;
        while i < self.sessions.len() {
            if idents.contains(&self.sessions[i].ident) {
                i += 1;
                continue;
            }
            let t = self.sessions.remove(i);
            self.untrack(&t);
            if t.last.is_some() {
                self.emit(MediaEvent::Removed { id: t.id.clone() });
            }
        }

        // Additions.
        for (session, ident) in list.into_iter().zip(idents) {
            if self.sessions.iter().any(|t| t.ident == ident) {
                continue;
            }
            let base = session.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default();
            let id = assign_id(&base, self.sessions.iter().map(|t| t.id.as_str()));
            let key = self.next_key;
            self.next_key += 1;
            let tokens = match self.subscribe(&session, key) {
                Ok(t) => t,
                Err(e) => {
                    log::warn!("subscribing to media session {id} failed: {e}");
                    continue;
                }
            };
            self.sessions.push(Tracked {
                key,
                ident,
                session,
                id,
                app_id: base,
                tokens,
                last: None,
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                thumb: None,
                thumb_ident: 0,
                art_hash: None,
                art_n: 0,
                artwork_key: None,
            });
            let idx = self.sessions.len() - 1;
            self.refresh(idx, true, self.started);
        }
        self.publish();
    }

    fn subscribe(&self, s: &Session, key: u64) -> windows_core::Result<[i64; 3]> {
        let t = self.tx.clone();
        let a = s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(Msg::Dirty { key, props: true });
            Ok(())
        }))?;
        let t = self.tx.clone();
        let b = match s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(Msg::Dirty { key, props: false });
            Ok(())
        })) {
            Ok(b) => b,
            Err(e) => {
                let _ = s.RemoveMediaPropertiesChanged(a);
                return Err(e);
            }
        };
        let t = self.tx.clone();
        let c = match s.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(Msg::Dirty { key, props: false });
            Ok(())
        })) {
            Ok(c) => c,
            Err(e) => {
                let _ = s.RemoveMediaPropertiesChanged(a);
                let _ = s.RemovePlaybackInfoChanged(b);
                return Err(e);
            }
        };
        Ok([a, b, c])
    }

    fn untrack(&self, t: &Tracked) {
        let _ = t.session.RemoveMediaPropertiesChanged(t.tokens[0]);
        let _ = t.session.RemovePlaybackInfoChanged(t.tokens[1]);
        let _ = t.session.RemoveTimelinePropertiesChanged(t.tokens[2]);
        if let Some(k) = &t.artwork_key {
            self.icons.remove(k);
        }
    }

    /// Reads the session state and emits `Updated` if it changed (when `emit` is set; the
    /// initial load only records state, the snapshot carries it).
    fn refresh(&mut self, idx: usize, props: bool, emit: bool) {
        let icons = self.icons.clone();
        let t = &mut self.sessions[idx];
        if props {
            read_properties(t, &icons);
        }
        let next = build_session(t);
        let redundant = t.last.as_ref().is_some_and(|old| logic::is_redundant(old, &next));
        if !redundant {
            t.last = Some(next.clone());
            if emit {
                (self.sink)(NativeEvent::Media(MediaEvent::Updated(next)));
            }
        }
    }

    fn update_current(&mut self, emit: bool) {
        // GetCurrentSession returns a different wrapper object than GetSessions, so COM
        // identity usually does not match; fall back to the app id (first session wins when an
        // app has several).
        let id = self.manager.GetCurrentSession().ok().and_then(|cur| {
            let ident = identity(&cur);
            let app = cur.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
            self.sessions
                .iter()
                .find(|t| t.ident == ident)
                .or_else(|| self.sessions.iter().find(|t| t.app_id == app))
                .map(|t| t.id.clone())
        });
        if id != self.current {
            self.current = id.clone();
            self.publish();
            if emit {
                self.emit(MediaEvent::Current { id });
            }
        }
    }

    fn control(&self, id: Option<&str>, action: MediaAction) -> arui_win::Result<()> {
        let session = match id {
            Some(id) => self
                .sessions
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.session.clone())
                .ok_or_else(|| arui_win::Error::NotFound(format!("media session {id}")))?,
            None => self
                .manager
                .GetCurrentSession()
                .map_err(|_| arui_win::Error::NotFound("no current media session".into()))?,
        };
        let op = match action {
            MediaAction::Play => session.TryPlayAsync()?,
            MediaAction::Pause => session.TryPauseAsync()?,
            MediaAction::TogglePlayPause => session.TryTogglePlayPauseAsync()?,
            MediaAction::Next => session.TrySkipNextAsync()?,
            MediaAction::Previous => session.TrySkipPreviousAsync()?,
            MediaAction::Stop => session.TryStopAsync()?,
        };
        if op.join()? {
            Ok(())
        } else {
            Err(arui_win::Error::Unsupported(format!("{action:?} rejected by the app")))
        }
    }

    fn shutdown(&mut self) {
        let _ = self.manager.RemoveSessionsChanged(self.manager_tokens[0]);
        let _ = self.manager.RemoveCurrentSessionChanged(self.manager_tokens[1]);
        for t in std::mem::take(&mut self.sessions) {
            self.untrack(&t);
        }
    }
}

/// Re-reads title/artist/album and, only when those or the thumbnail reference changed,
/// the artwork bytes.
fn read_properties(t: &mut Tracked, icons: &IconStore) {
    let props = match t.session.TryGetMediaPropertiesAsync().and_then(|op| op.join()) {
        Ok(p) => p,
        Err(e) => {
            log::debug!("TryGetMediaPropertiesAsync({}) failed: {e}", t.id);
            return;
        }
    };
    let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
    let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
    let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
    let thumb = props.Thumbnail().ok();
    let thumb_ident = thumb.as_ref().map(identity).unwrap_or(0);

    let meta_changed = title != t.title || artist != t.artist || album != t.album;
    let thumb_changed = thumb_ident != t.thumb_ident;
    t.title = title;
    t.artist = artist;
    t.album = album;
    if !meta_changed && !thumb_changed {
        return;
    }
    t.thumb = thumb;
    t.thumb_ident = thumb_ident;

    let png = t.thumb.as_ref().and_then(|r| match read_stream(r) {
        Ok(b) => ensure_png(b),
        Err(e) => {
            log::debug!("reading artwork of {} failed: {e}", t.id);
            None
        }
    });
    match png {
        Some(png) => {
            let h = logic::hash_bytes(&png);
            if t.art_hash == Some(h) {
                return; // same picture, keep the key so the UI does not reload it
            }
            if let Some(old) = t.artwork_key.take() {
                icons.remove(&old);
            }
            t.art_n += 1;
            let key = format!("art:{}:{}", t.id, t.art_n);
            icons.put(key.clone(), png);
            t.art_hash = Some(h);
            t.artwork_key = Some(key);
        }
        None => {
            if let Some(old) = t.artwork_key.take() {
                icons.remove(&old);
            }
            t.art_hash = None;
        }
    }
}

fn read_stream(r: &IRandomAccessStreamReference) -> windows_core::Result<Vec<u8>> {
    let stream = r.OpenReadAsync()?.join()?;
    let size = stream.Size()?;
    if size == 0 || size > MAX_ART_BYTES {
        return Ok(Vec::new());
    }
    let buffer = Buffer::Create(size as u32)?;
    let filled = stream.ReadAsync(&buffer, size as u32, InputStreamOptions::None)?.join()?;
    let len = filled.Length()? as usize;
    let mut bytes = vec![0u8; len];
    if len > 0 {
        DataReader::FromBuffer(&filled)?.ReadBytes(&mut bytes)?;
    }
    let _ = stream.Close();
    Ok(bytes)
}

/// Builds the contract struct from the session's live playback/timeline state plus the
/// cached media properties.
fn build_session(t: &Tracked) -> MediaSession {
    let s = &t.session;
    let mut out = MediaSession {
        id: t.id.clone(),
        app_id: t.app_id.clone(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        status: PlaybackStatus::Closed,
        can_play: false,
        can_pause: false,
        can_next: false,
        can_previous: false,
        position_ms: None,
        duration_ms: None,
        position_at_ms: None,
        artwork_key: t.artwork_key.clone(),
    };
    if let Ok(info) = s.GetPlaybackInfo() {
        if let Ok(st) = info.PlaybackStatus() {
            out.status = map_status(st.0);
        }
        if let Ok(c) = info.Controls() {
            out.can_play = c.IsPlayEnabled().unwrap_or(false);
            out.can_pause = c.IsPauseEnabled().unwrap_or(false);
            out.can_next = c.IsNextEnabled().unwrap_or(false);
            out.can_previous = c.IsPreviousEnabled().unwrap_or(false);
        }
    }
    if let Ok(tl) = s.GetTimelineProperties() {
        let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
        let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
        let pos = tl.Position().map(|t| t.Duration).unwrap_or(0);
        let (p, d) = timeline_ms(start, end, pos);
        out.position_ms = p;
        out.duration_ms = d;
        if p.is_some() {
            out.position_at_ms = Some(
                tl.LastUpdatedTime()
                    .ok()
                    .and_then(|d| datetime_to_unix_ms(d.UniversalTime))
                    .unwrap_or_else(now_ms),
            );
        }
    }
    out
}
