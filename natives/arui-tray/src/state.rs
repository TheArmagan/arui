//! Pure tray bookkeeping: applies parsed NIM_* messages to the icon list and reports what
//! changed. Win32 work (icon conversion, exe lookup) is injected through [`Env`] so this module
//! is unit testable.

use arui_win::{IconStore, TrayAction, TrayIcon};

use crate::parse::*;

/// Side effects the state needs from the outside world.
pub trait Env {
    /// Converts a (foreign) HICON to PNG bytes. `None` if the handle is invalid.
    fn icon_png(&mut self, hicon: u64) -> Option<Vec<u8>>;
    /// Owner process id and exe path of a window.
    fn owner_process(&mut self, hwnd: u64) -> (u32, Option<String>);
}

#[derive(Debug, Clone, PartialEq)]
pub struct IconRecord {
    pub icon: TrayIcon,
    pub pid: u32,
    /// NIF_SHOWTIP seen: v4 apps that want the standard tooltip instead of NIN_POPUPOPEN.
    pub show_tip: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Added(TrayIcon),
    Updated(TrayIcon),
    Removed(String),
    Balloon {
        id: String,
        title: String,
        text: String,
    },
}

#[derive(Debug, Default)]
pub struct Outcome {
    /// Value returned from WM_COPYDATA (TRUE / FALSE), like explorer.
    pub result: bool,
    pub changes: Vec<Change>,
}

pub struct TrayState {
    icons: Vec<IconRecord>,
    store: IconStore,
    /// Monotonic counter for icon store keys, so every new image gets a fresh URL.
    next_key: u64,
}

pub fn icon_id(guid: Option<&str>, hwnd: u64, uid: u32) -> String {
    match guid {
        Some(g) => format!("guid:{g}"),
        None => format!("{hwnd}:{uid}"),
    }
}

impl TrayState {
    pub fn new(store: IconStore) -> Self {
        Self {
            icons: Vec::new(),
            store,
            next_key: 0,
        }
    }

    pub fn icons(&self) -> Vec<TrayIcon> {
        self.icons.iter().map(|r| r.icon.clone()).collect()
    }

    pub fn get(&self, id: &str) -> Option<&IconRecord> {
        self.icons.iter().find(|r| r.icon.id == id)
    }

    pub fn owners(&self) -> impl Iterator<Item = u64> + '_ {
        self.icons.iter().map(|r| r.icon.owner_hwnd)
    }

    /// Finds an icon the way explorer / ManagedShell match: by GUID when the message carries one,
    /// otherwise by (hWnd, uID), which also matches icons that were added with a GUID.
    pub fn find(&self, guid: Option<&str>, hwnd: u64, uid: u32) -> Option<usize> {
        if let Some(g) = guid {
            if let Some(i) = self
                .icons
                .iter()
                .position(|r| r.icon.guid.as_deref() == Some(g))
            {
                return Some(i);
            }
        }
        self.icons
            .iter()
            .position(|r| r.icon.owner_hwnd == hwnd && r.icon.uid == uid && hwnd != 0)
    }

    pub fn apply(&mut self, message: u32, nid: &NotifyIconData, env: &mut dyn Env) -> Outcome {
        let guid = nid.identity_guid();
        // Explorer rejects icons with neither an owner window nor a GUID.
        if nid.hwnd == 0 && (guid.is_none() || message == NIM_ADD) {
            return Outcome::default();
        }
        let idx = self.find(guid, nid.hwnd, nid.uid);
        match message {
            NIM_ADD | NIM_MODIFY => {
                let mut out = Outcome::default();
                match idx {
                    Some(i) => {
                        let changed = self.update_fields(i, nid, env);
                        if changed {
                            out.changes
                                .push(Change::Updated(self.icons[i].icon.clone()));
                        }
                        out.result = true;
                    }
                    None => {
                        // ManagedShell behavior: an unknown NIM_MODIFY still creates the icon
                        // (apps that never saw TaskbarCreated keep working), but returns FALSE
                        // like explorer so well-behaved apps follow up with NIM_ADD.
                        let i = self.insert(nid, env);
                        out.changes.push(Change::Added(self.icons[i].icon.clone()));
                        out.result = message == NIM_ADD;
                    }
                }
                if nid.has(NIF_INFO) && !(nid.info.is_empty() && nid.info_title.is_empty()) {
                    let id = idx.map_or_else(
                        || {
                            self.icons
                                .last()
                                .map(|r| r.icon.id.clone())
                                .unwrap_or_default()
                        },
                        |i| self.icons[i].icon.id.clone(),
                    );
                    out.changes.push(Change::Balloon {
                        id,
                        title: nid.info_title.clone(),
                        text: nid.info.clone(),
                    });
                }
                out
            }
            NIM_DELETE => match idx {
                Some(i) => Outcome {
                    result: true,
                    changes: vec![self.remove_at(i)],
                },
                None => Outcome::default(),
            },
            NIM_SETVERSION => {
                let v = nid.version_or_timeout;
                if v > 4 {
                    return Outcome::default();
                }
                let mut out = Outcome {
                    result: true,
                    changes: vec![],
                };
                if let Some(i) = idx {
                    if self.icons[i].icon.version != v {
                        self.icons[i].icon.version = v;
                        out.changes
                            .push(Change::Updated(self.icons[i].icon.clone()));
                    }
                }
                out
            }
            NIM_SETFOCUS => Outcome {
                result: true,
                changes: vec![],
            },
            _ => Outcome::default(),
        }
    }

    fn insert(&mut self, nid: &NotifyIconData, env: &mut dyn Env) -> usize {
        let guid = nid.identity_guid().map(str::to_owned);
        let id = icon_id(guid.as_deref(), nid.hwnd, nid.uid);
        let (pid, exe_path) = env.owner_process(nid.hwnd);
        self.icons.push(IconRecord {
            icon: TrayIcon {
                id,
                owner_hwnd: nid.hwnd,
                uid: nid.uid,
                guid,
                callback_message: 0,
                version: 0,
                tooltip: String::new(),
                icon_key: String::new(),
                visible: true,
                exe_path,
            },
            pid,
            show_tip: false,
        });
        let i = self.icons.len() - 1;
        self.update_fields(i, nid, env);
        i
    }

    /// Applies the fields selected by uFlags. Returns whether anything visible changed.
    fn update_fields(&mut self, i: usize, nid: &NotifyIconData, env: &mut dyn Env) -> bool {
        let before = self.icons[i].clone();
        if nid.has(NIF_MESSAGE) {
            self.icons[i].icon.callback_message = nid.callback_message;
        }
        if nid.has(NIF_TIP) {
            self.icons[i].icon.tooltip = nid.tip.clone();
        }
        if nid.has(NIF_STATE) && nid.state_mask & NIS_HIDDEN != 0 {
            self.icons[i].icon.visible = nid.state & NIS_HIDDEN == 0;
        }
        if nid.has(NIF_SHOWTIP) {
            self.icons[i].show_tip = true;
        }
        // An icon added by (hWnd, uID) may later be given a GUID; keep the original id though,
        // the UI already knows the icon under it.
        if let Some(g) = nid.identity_guid() {
            self.icons[i].icon.guid = Some(g.to_owned());
        }
        if nid.has(NIF_ICON) {
            self.set_image(i, nid.hicon, env);
        }
        self.icons[i] != before
    }

    fn set_image(&mut self, i: usize, hicon: u64, env: &mut dyn Env) {
        let old_key = self.icons[i].icon.icon_key.clone();
        if hicon == 0 {
            // Explorer shows an empty slot for a NULL icon (apps use this to blink).
            if !old_key.is_empty() {
                self.store.remove(&old_key);
                self.icons[i].icon.icon_key.clear();
            }
            return;
        }
        let Some(png) = env.icon_png(hicon) else {
            return; // keep the previous image if the new handle can't be read
        };
        if !old_key.is_empty() {
            if let Some(cur) = self.store.get(&old_key) {
                if cur.as_slice() == png.as_slice() {
                    return; // same pixels, keep the cached URL
                }
            }
            self.store.remove(&old_key);
        }
        self.next_key += 1;
        let key = format!("tray:{}:{}", self.icons[i].icon.id, self.next_key);
        self.store.put(key.clone(), png);
        self.icons[i].icon.icon_key = key;
    }

    fn remove_at(&mut self, i: usize) -> Change {
        let rec = self.icons.remove(i);
        if !rec.icon.icon_key.is_empty() {
            self.store.remove(&rec.icon.icon_key);
        }
        Change::Removed(rec.icon.id)
    }

    /// Removes every icon owned by `hwnd` (its window was destroyed).
    pub fn remove_owner(&mut self, hwnd: u64) -> Vec<Change> {
        self.remove_where(|r| r.icon.owner_hwnd == hwnd)
    }

    pub fn remove_id(&mut self, id: &str) -> Vec<Change> {
        self.remove_where(|r| r.icon.id == id)
    }

    pub fn remove_where(&mut self, mut pred: impl FnMut(&IconRecord) -> bool) -> Vec<Change> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.icons.len() {
            if pred(&self.icons[i]) {
                out.push(self.remove_at(i));
            } else {
                i += 1;
            }
        }
        out
    }

    /// Drops every icon and its images (used on shutdown).
    pub fn clear(&mut self) {
        for r in self.icons.drain(..) {
            if !r.icon.icon_key.is_empty() {
                self.store.remove(&r.icon.icon_key);
            }
        }
    }
}

// ---------------------------------------------------------------- click forwarding

pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_LBUTTONDBLCLK: u32 = 0x0203;
pub const WM_RBUTTONDOWN: u32 = 0x0204;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_MBUTTONDOWN: u32 = 0x0207;
pub const WM_MBUTTONUP: u32 = 0x0208;
pub const WM_CONTEXTMENU: u32 = 0x007B;
pub const NIN_SELECT: u32 = 0x0400;
pub const NIN_POPUPOPEN: u32 = 0x0406;
pub const NIN_POPUPCLOSE: u32 = 0x0407;

/// The notifications explorer would send for a user action, in order (ManagedShell's choices):
/// NIN_SELECT / WM_CONTEXTMENU from version 3 on, NIN_POPUPOPEN / NIN_POPUPCLOSE for version 4
/// apps that did not ask for the standard tooltip (NIF_SHOWTIP).
pub fn action_notifications(version: u32, show_tip: bool, action: TrayAction) -> Vec<u32> {
    let v3 = version >= 3;
    let popups = version >= 4 && !show_tip;
    let mut m = Vec::new();
    match action {
        TrayAction::LeftClick => {
            m.extend([WM_LBUTTONDOWN, WM_LBUTTONUP]);
            if v3 {
                m.push(NIN_SELECT);
            }
        }
        TrayAction::DoubleClick => {
            m.extend([WM_LBUTTONDBLCLK, WM_LBUTTONUP]);
            if v3 {
                m.push(NIN_SELECT);
            }
        }
        TrayAction::RightClick => {
            m.extend([WM_RBUTTONDOWN, WM_RBUTTONUP]);
            if v3 {
                m.push(WM_CONTEXTMENU);
            }
        }
        TrayAction::MiddleClick => m.extend([WM_MBUTTONDOWN, WM_MBUTTONUP]),
        TrayAction::HoverEnter => {
            m.push(WM_MOUSEMOVE);
            if popups {
                m.push(NIN_POPUPOPEN);
            }
        }
        TrayAction::HoverLeave => {
            if popups {
                m.push(NIN_POPUPCLOSE);
            }
        }
    }
    m
}

/// (wParam, lParam) for the app's callback message.
/// Version < 4: wParam = uID, lParam = notification.
/// Version 4:   wParam = MAKEWPARAM(x, y) anchor, lParam = MAKELPARAM(notification, uID).
pub fn callback_params(
    version: u32,
    uid: u32,
    notification: u32,
    x: i32,
    y: i32,
) -> (usize, isize) {
    if version >= 4 {
        (
            make_long(x, y) as usize,
            make_long(notification as i32, uid as i32) as isize,
        )
    } else {
        (uid as usize, notification as isize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::tests::{NidBuilder, GUID_BYTES, GUID_STR};

    struct FakeEnv {
        conversions: usize,
    }

    impl Env for FakeEnv {
        fn icon_png(&mut self, hicon: u64) -> Option<Vec<u8>> {
            self.conversions += 1;
            if hicon == 0xDEAD {
                None
            } else {
                Some(hicon.to_le_bytes().to_vec())
            }
        }
        fn owner_process(&mut self, hwnd: u64) -> (u32, Option<String>) {
            (hwnd as u32 + 1000, Some(format!("C:\\apps\\{hwnd}.exe")))
        }
    }

    fn nid(hwnd: u32, uid: u32, flags: u32, f: impl FnOnce(&mut NidBuilder)) -> NotifyIconData {
        let mut b = NidBuilder::new(NID32_SIZE);
        b.ids(hwnd, uid, flags, 0x8010, 0x100);
        f(&mut b);
        parse_notify_icon_data(&b.buf).unwrap()
    }

    fn setup() -> (TrayState, FakeEnv, IconStore) {
        let store = IconStore::new();
        (
            TrayState::new(store.clone()),
            FakeEnv { conversions: 0 },
            store,
        )
    }

    #[test]
    fn add_modify_delete_by_hwnd_uid() {
        let (mut s, mut env, store) = setup();
        let add = nid(42, 1, NIF_MESSAGE | NIF_ICON | NIF_TIP, |b| {
            b.wstr(24, "hello");
        });
        let out = s.apply(NIM_ADD, &add, &mut env);
        assert!(out.result);
        let Change::Added(icon) = &out.changes[0] else {
            panic!("{:?}", out.changes)
        };
        assert_eq!(icon.id, "42:1");
        assert_eq!(icon.tooltip, "hello");
        assert_eq!(icon.callback_message, 0x8010);
        assert_eq!(icon.exe_path.as_deref(), Some("C:\\apps\\42.exe"));
        assert!(icon.icon_key.starts_with("tray:42:1:"));
        assert!(store.contains(&icon.icon_key));
        let first_key = icon.icon_key.clone();

        // New image -> new key, old key evicted.
        let modify = nid(42, 1, NIF_ICON, |b| {
            b.u32(20, 0x200);
        });
        let out = s.apply(NIM_MODIFY, &modify, &mut env);
        assert!(out.result);
        let Change::Updated(icon) = &out.changes[0] else {
            panic!()
        };
        assert_ne!(icon.icon_key, first_key);
        assert!(!store.contains(&first_key));
        assert_eq!(icon.tooltip, "hello", "tip untouched without NIF_TIP");

        // Same pixels again -> no change event, key kept.
        let out = s.apply(NIM_MODIFY, &modify, &mut env);
        assert!(out.result);
        assert!(out.changes.is_empty());

        let last_key = s.icons()[0].icon_key.clone();
        let out = s.apply(NIM_DELETE, &nid(42, 1, 0, |_| {}), &mut env);
        assert!(out.result);
        assert_eq!(out.changes, vec![Change::Removed("42:1".into())]);
        assert!(s.icons().is_empty());
        assert!(!store.contains(&last_key));
        // Second delete fails like explorer.
        assert!(!s.apply(NIM_DELETE, &nid(42, 1, 0, |_| {}), &mut env).result);
    }

    #[test]
    fn guid_identity() {
        let (mut s, mut env, _) = setup();
        let add = nid(42, 1, NIF_ICON | NIF_GUID, |b| {
            b.bytes(936, &GUID_BYTES);
        });
        let out = s.apply(NIM_ADD, &add, &mut env);
        let Change::Added(icon) = &out.changes[0] else {
            panic!()
        };
        assert_eq!(icon.id, format!("guid:{GUID_STR}"));
        assert_eq!(icon.guid.as_deref(), Some(GUID_STR));

        // A modify from a different hwnd with the same GUID targets the same icon.
        let modify = nid(99, 5, NIF_TIP | NIF_GUID, |b| {
            b.bytes(936, &GUID_BYTES).wstr(24, "via guid");
        });
        let out = s.apply(NIM_MODIFY, &modify, &mut env);
        assert!(out.result);
        assert_eq!(s.icons().len(), 1);
        assert_eq!(s.icons()[0].tooltip, "via guid");

        // hWnd == 0 with a GUID is allowed for modify/delete.
        let del = nid(0, 0, NIF_GUID, |b| {
            b.bytes(936, &GUID_BYTES);
        });
        assert!(s.apply(NIM_DELETE, &del, &mut env).result);
        assert!(s.icons().is_empty());
    }

    #[test]
    fn hwnd_zero_without_guid_rejected() {
        let (mut s, mut env, _) = setup();
        assert!(
            !s.apply(NIM_ADD, &nid(0, 1, NIF_ICON, |_| {}), &mut env)
                .result
        );
        assert!(s.icons().is_empty());
    }

    #[test]
    fn modify_unknown_adds_but_returns_false() {
        let (mut s, mut env, _) = setup();
        let out = s.apply(
            NIM_MODIFY,
            &nid(7, 3, NIF_TIP, |b| {
                b.wstr(24, "x");
            }),
            &mut env,
        );
        assert!(!out.result);
        assert!(matches!(out.changes[0], Change::Added(_)));
        // The follow-up NIM_ADD then updates it.
        let out = s.apply(
            NIM_ADD,
            &nid(7, 3, NIF_TIP, |b| {
                b.wstr(24, "y");
            }),
            &mut env,
        );
        assert!(out.result);
        assert!(matches!(out.changes[0], Change::Updated(_)));
        assert_eq!(s.icons().len(), 1);
    }

    #[test]
    fn hidden_state_respects_mask() {
        let (mut s, mut env, _) = setup();
        s.apply(
            NIM_ADD,
            &nid(1, 1, NIF_STATE, |b| {
                b.u32(280, NIS_HIDDEN).u32(284, NIS_HIDDEN);
            }),
            &mut env,
        );
        assert!(!s.icons()[0].visible);
        // State bit without the mask bit is ignored.
        s.apply(
            NIM_MODIFY,
            &nid(1, 1, NIF_STATE, |b| {
                b.u32(280, 0).u32(284, NIS_SHAREDICON);
            }),
            &mut env,
        );
        assert!(!s.icons()[0].visible);
        s.apply(
            NIM_MODIFY,
            &nid(1, 1, NIF_STATE, |b| {
                b.u32(280, 0).u32(284, NIS_HIDDEN);
            }),
            &mut env,
        );
        assert!(s.icons()[0].visible);
    }

    #[test]
    fn setversion_and_showtip() {
        let (mut s, mut env, _) = setup();
        s.apply(NIM_ADD, &nid(1, 1, NIF_SHOWTIP, |_| {}), &mut env);
        let out = s.apply(
            NIM_SETVERSION,
            &nid(1, 1, 0, |b| {
                b.u32(800, 4);
            }),
            &mut env,
        );
        assert!(out.result);
        assert_eq!(s.icons()[0].version, 4);
        assert!(s.get("1:1").unwrap().show_tip);
        assert!(
            !s.apply(
                NIM_SETVERSION,
                &nid(1, 1, 0, |b| {
                    b.u32(800, 5);
                }),
                &mut env
            )
            .result
        );
        assert!(
            s.apply(NIM_SETFOCUS, &nid(1, 1, 0, |_| {}), &mut env)
                .result
        );
        assert!(!s.apply(99, &nid(1, 1, 0, |_| {}), &mut env).result);
    }

    #[test]
    fn balloon() {
        let (mut s, mut env, _) = setup();
        s.apply(NIM_ADD, &nid(1, 1, 0, |_| {}), &mut env);
        let out = s.apply(
            NIM_MODIFY,
            &nid(1, 1, NIF_INFO, |b| {
                b.wstr(288, "Body").wstr(804, "Title");
            }),
            &mut env,
        );
        assert_eq!(
            out.changes,
            vec![Change::Balloon {
                id: "1:1".into(),
                title: "Title".into(),
                text: "Body".into()
            }]
        );
        // Empty info = hide balloon, no event.
        let out = s.apply(NIM_MODIFY, &nid(1, 1, NIF_INFO, |_| {}), &mut env);
        assert!(out.changes.is_empty());
        // Balloon on a fresh add uses the new id.
        let out = s.apply(
            NIM_ADD,
            &nid(2, 2, NIF_INFO, |b| {
                b.wstr(288, "B");
            }),
            &mut env,
        );
        assert!(matches!(&out.changes[1], Change::Balloon { id, .. } if id == "2:2"));
    }

    #[test]
    fn null_or_bad_icon() {
        let (mut s, mut env, store) = setup();
        s.apply(NIM_ADD, &nid(1, 1, NIF_ICON, |_| {}), &mut env);
        let key = s.icons()[0].icon_key.clone();
        // Unreadable handle keeps the old image.
        s.apply(
            NIM_MODIFY,
            &nid(1, 1, NIF_ICON, |b| {
                b.u32(20, 0xDEAD);
            }),
            &mut env,
        );
        assert_eq!(s.icons()[0].icon_key, key);
        // NULL icon clears it.
        s.apply(
            NIM_MODIFY,
            &nid(1, 1, NIF_ICON, |b| {
                b.u32(20, 0);
            }),
            &mut env,
        );
        assert_eq!(s.icons()[0].icon_key, "");
        assert!(!store.contains(&key));
    }

    #[test]
    fn remove_owner() {
        let (mut s, mut env, _) = setup();
        s.apply(NIM_ADD, &nid(1, 1, 0, |_| {}), &mut env);
        s.apply(NIM_ADD, &nid(1, 2, 0, |_| {}), &mut env);
        s.apply(NIM_ADD, &nid(2, 1, 0, |_| {}), &mut env);
        let ch = s.remove_owner(1);
        assert_eq!(ch.len(), 2);
        assert_eq!(s.icons().len(), 1);
        assert_eq!(s.icons()[0].id, "2:1");
    }

    #[test]
    fn v1_struct_through_state() {
        let (mut s, mut env, _) = setup();
        let mut b = NidBuilder::new(NID32_V1_SIZE);
        b.ids(5, 1, NIF_TIP | NIF_MESSAGE, 0x400, 0)
            .wstr(24, "legacy");
        let n = parse_notify_icon_data(&b.buf).unwrap();
        s.apply(NIM_ADD, &n, &mut env);
        assert_eq!(s.icons()[0].tooltip, "legacy");
        assert_eq!(s.icons()[0].callback_message, 0x400);
    }

    #[test]
    fn notifications_per_version() {
        use TrayAction::*;
        assert_eq!(
            action_notifications(0, false, LeftClick),
            vec![WM_LBUTTONDOWN, WM_LBUTTONUP]
        );
        assert_eq!(
            action_notifications(3, false, LeftClick),
            vec![WM_LBUTTONDOWN, WM_LBUTTONUP, NIN_SELECT]
        );
        assert_eq!(
            action_notifications(4, false, RightClick),
            vec![WM_RBUTTONDOWN, WM_RBUTTONUP, WM_CONTEXTMENU]
        );
        assert_eq!(
            action_notifications(0, false, RightClick),
            vec![WM_RBUTTONDOWN, WM_RBUTTONUP]
        );
        assert_eq!(
            action_notifications(4, false, HoverEnter),
            vec![WM_MOUSEMOVE, NIN_POPUPOPEN]
        );
        assert_eq!(
            action_notifications(4, true, HoverEnter),
            vec![WM_MOUSEMOVE]
        );
        assert_eq!(
            action_notifications(3, false, HoverEnter),
            vec![WM_MOUSEMOVE]
        );
        assert_eq!(
            action_notifications(4, false, HoverLeave),
            vec![NIN_POPUPCLOSE]
        );
        assert!(action_notifications(0, false, HoverLeave).is_empty());
        assert_eq!(
            action_notifications(0, false, DoubleClick),
            vec![WM_LBUTTONDBLCLK, WM_LBUTTONUP]
        );
        assert_eq!(
            action_notifications(4, false, MiddleClick),
            vec![WM_MBUTTONDOWN, WM_MBUTTONUP]
        );
    }

    #[test]
    fn params_encoding() {
        assert_eq!(
            callback_params(0, 7, WM_LBUTTONUP, 10, 20),
            (7, WM_LBUTTONUP as isize)
        );
        assert_eq!(
            callback_params(3, 7, NIN_SELECT, 10, 20),
            (7, NIN_SELECT as isize)
        );
        let (w, l) = callback_params(4, 7, WM_CONTEXTMENU, 1920, 1050);
        assert_eq!(w, (1050 << 16) | 1920);
        assert_eq!(l, (7 << 16) | WM_CONTEXTMENU as isize);
        // Negative coordinates on a monitor left of the primary.
        let (w, _) = callback_params(4, 1, NIN_SELECT, -100, 5);
        assert_eq!(w, (5 << 16) | 0xFF9C);
    }
}
