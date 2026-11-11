//! Lone Win key capture with a WH_KEYBOARD_LL hook on its own thread.
//!
//! Win down is always passed through so Win+<key> shortcuts keep working. When Win is released
//! without any other key pressed in between, we swallow the real release and inject
//! `[VK 0xE8 down, VK 0xE8 up, Win up]`: Windows then sees a "Win + unassigned key" combo and does
//! not open its Start menu (the same trick AutoHotkey uses with its menu mask key). Our own
//! injected events carry [`MAGIC`] in `dwExtraInfo` and are ignored by the hook.

use std::cell::RefCell;
use std::sync::mpsc;
use std::thread::JoinHandle;

use arui_win::{Error, EventSink, NativeEvent, Result, ShellEvent};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_LWIN, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

/// Marks input injected by us.
pub(crate) const MAGIC: usize = 0x4152_5549; // "ARUI"
/// Unassigned virtual key used as the mask key.
const VK_MASK: u16 = 0xE8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyAction {
    Pass,
    /// Lone Win release: emit StartKey, swallow and replace with the masked release.
    LoneWinRelease,
}

/// Pure state machine deciding when a Win press counts as "lone".
#[derive(Debug, Default)]
pub(crate) struct WinKeyMachine {
    held: Option<u32>,
    other: bool,
}

fn is_win(vk: u32) -> bool {
    vk == VK_LWIN.0 as u32 || vk == VK_RWIN.0 as u32
}

impl WinKeyMachine {
    pub fn on_key(&mut self, vk: u32, down: bool) -> KeyAction {
        if is_win(vk) {
            if down {
                match self.held {
                    None => {
                        self.held = Some(vk);
                        self.other = false;
                    }
                    Some(h) if h != vk => self.other = true, // both Win keys
                    Some(_) => {}                            // auto-repeat
                }
                KeyAction::Pass
            } else if self.held == Some(vk) {
                let lone = !self.other;
                self.held = None;
                self.other = false;
                if lone {
                    KeyAction::LoneWinRelease
                } else {
                    KeyAction::Pass
                }
            } else {
                KeyAction::Pass
            }
        } else {
            if down && self.held.is_some() {
                self.other = true;
            }
            KeyAction::Pass
        }
    }
}

fn key(vk: u16, up: bool, extended: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: MAGIC },
        },
    }
}

fn send(inputs: &[INPUT]) {
    unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
}

/// Presses and releases LWin, marked so our hook lets it through (opens Windows' Start).
pub(crate) fn tap_win() {
    send(&[key(VK_LWIN.0, false, true), key(VK_LWIN.0, true, true)]);
}

thread_local! {
    static MACHINE: RefCell<WinKeyMachine> = RefCell::new(WinKeyMachine::default());
    static SINK: RefCell<Option<EventSink>> = const { RefCell::new(None) };
}

unsafe extern "system" fn hook_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let msg = wp.0 as u32;
        let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        if kb.dwExtraInfo != MAGIC && (down || up) {
            let action = MACHINE.with(|m| m.borrow_mut().on_key(kb.vkCode, down));
            if action == KeyAction::LoneWinRelease {
                let vk = kb.vkCode as u16;
                send(&[key(VK_MASK, false, false), key(VK_MASK, true, false), key(vk, true, true)]);
                if let Some(sink) = SINK.with(|s| s.borrow().clone()) {
                    sink(NativeEvent::Shell(ShellEvent::StartKey));
                }
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

pub(crate) struct KeyHook {
    tid: u32,
    join: Option<JoinHandle<()>>,
}

impl KeyHook {
    pub fn spawn(sink: EventSink) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Result<u32>>();
        let join = std::thread::Builder::new()
            .name("arui-shell-keyhook".into())
            .spawn(move || unsafe {
                // Create the message queue before announcing the thread id.
                let mut msg = MSG::default();
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                SINK.with(|s| *s.borrow_mut() = Some(sink));
                let hinst = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
                let hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hinst, 0) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = tx.send(Err(e.into()));
                        return;
                    }
                };
                let _ = tx.send(Ok(GetCurrentThreadId()));
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                let _ = UnhookWindowsHookEx(hook);
            })
            .map_err(|e| Error::Other(e.to_string()))?;
        match rx.recv() {
            Ok(Ok(tid)) => Ok(Self { tid, join: Some(join) }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => Err(Error::Other("key hook thread died".into())),
        }
    }
}

impl Drop for KeyHook {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LWIN: u32 = 0x5B;
    const RWIN: u32 = 0x5C;
    const E: u32 = 0x45;
    const SHIFT: u32 = 0xA0;

    #[test]
    fn lone_press() {
        let mut m = WinKeyMachine::default();
        assert_eq!(m.on_key(LWIN, true), KeyAction::Pass);
        assert_eq!(m.on_key(LWIN, true), KeyAction::Pass); // auto-repeat
        assert_eq!(m.on_key(LWIN, false), KeyAction::LoneWinRelease);
        assert_eq!(m.on_key(RWIN, true), KeyAction::Pass);
        assert_eq!(m.on_key(RWIN, false), KeyAction::LoneWinRelease);
    }

    #[test]
    fn combos_pass() {
        let mut m = WinKeyMachine::default();
        m.on_key(LWIN, true);
        m.on_key(E, true);
        m.on_key(E, false);
        assert_eq!(m.on_key(LWIN, false), KeyAction::Pass);
        // Next lone press works again.
        m.on_key(LWIN, true);
        assert_eq!(m.on_key(LWIN, false), KeyAction::LoneWinRelease);
        // Win+Shift+S
        m.on_key(LWIN, true);
        m.on_key(SHIFT, true);
        assert_eq!(m.on_key(LWIN, false), KeyAction::Pass);
        m.on_key(SHIFT, false);
    }

    #[test]
    fn key_released_during_win_does_not_count() {
        let mut m = WinKeyMachine::default();
        m.on_key(SHIFT, true);
        m.on_key(LWIN, true);
        m.on_key(SHIFT, false); // only a release
        assert_eq!(m.on_key(LWIN, false), KeyAction::LoneWinRelease);
    }

    #[test]
    fn both_win_keys() {
        let mut m = WinKeyMachine::default();
        m.on_key(LWIN, true);
        m.on_key(RWIN, true);
        assert_eq!(m.on_key(RWIN, false), KeyAction::Pass);
        assert_eq!(m.on_key(LWIN, false), KeyAction::Pass);
        // Stray Win up without down.
        assert_eq!(m.on_key(LWIN, false), KeyAction::Pass);
    }
}
