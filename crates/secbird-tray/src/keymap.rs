//! Key remaps done on the PC while the Tartarus V2 Pro is in driver mode (its own keymap is
//! off then): a low-level keyboard hook swaps a keystroke for its remap when the keypad's
//! report says the keypad has that key down. The deciding is in [`secbird_core::keymap`].
//!
//! The hook runs on a thread of its own with nothing else to do, so a slow device call
//! elsewhere never holds up typing. It only ever waits, briefly ([`WAIT`]), for a key that
//! is remapped and not yet seen on the keypad.

use secbird_core::keymap::{Decision, Matcher, Remaps, WAIT};
use std::ptr::null_mut;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Instant;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, HC_ACTION, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

pub struct Shared {
    matcher: Mutex<Matcher>,
    reported: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Matcher> {
        self.matcher.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The remaps to do now: empty unless the keypad is in driver mode (its profile has the
    /// stick as keys). Keys held under the old remaps are let go.
    pub fn set(&self, remaps: Remaps) {
        let mut m = self.lock();
        if *m.remaps() != remaps {
            for code in m.release_all() {
                crate::stick::press(code, false);
            }
            m.set_remaps(remaps);
        }
    }

    /// The keypad's report 04: its thumb inputs down. A remapped one is pressed.
    pub fn razer_keys(&self, keys: &[u8]) {
        let presses = self.lock().razer_keys(keys);
        for (code, down) in presses {
            crate::stick::press(code, down);
        }
    }

    /// The keypad's report 0x0B: the keys it has down.
    pub fn report(&self, keys: &[u8]) {
        self.lock().report(keys, Instant::now());
        self.reported.notify_all();
    }
}

pub fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared { matcher: Mutex::new(Matcher::default()), reported: Condvar::new() })
}

/// Start the hook's thread.
pub fn start() {
    let _ = std::thread::Builder::new().name("keymap".into()).spawn(|| unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), GetModuleHandleW(std::ptr::null()), 0);
        if hook.is_null() {
            secbird_core::log::append("keyboard hook could not start; keypad remaps work only in normal mode");
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lparam as *const KBDLLHOOKSTRUCT);
        // Our own keys (and the stick's) come back through here marked injected.
        if k.flags & LLKHF_INJECTED == 0 {
            let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let ext = if k.flags & LLKHF_EXTENDED != 0 { 0xE000 } else { 0 };
            let scan = (k.scanCode as u16 & 0xFF) | ext;
            if let Decision::Replace(to, to_down) = decide(scan, down) {
                crate::stick::press(to, to_down);
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

fn decide(scan: u16, down: bool) -> Decision {
    let shared = shared();
    let mut m = shared.lock();
    if m.remaps().is_empty() {
        return Decision::Pass;
    }
    // The keystroke can beat the keypad's report: wait a moment for it.
    let deadline = Instant::now() + WAIT;
    while m.worth_waiting(scan, down, Instant::now()) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        m = shared.reported.wait_timeout(m, left).unwrap_or_else(|p| p.into_inner()).0;
    }
    m.key(scan, down, Instant::now())
}
