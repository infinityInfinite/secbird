//! Raw Input on the tray window, for the two things that listen to it: the Reactive and
//! Ripple effects (key presses and clicks, while a scene uses them) and the watch for a
//! wireless device that is asleep (its first input says it woke). Windows keeps one
//! registration per kind of input for the whole program, so this module is the only one that
//! asks for it: registered while either of the two needs it, dropped when neither does.
//! Only Razer devices count, and nothing but "which device" and, for the effects, "which key
//! went down" leaves this module.

use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows_sys::Win32::UI::Input::*;

/// Generic desktop page: mouse and keyboard.
const MOUSE: u16 = 2;
const KEYBOARD: u16 = 6;
/// Button-down bits of `usButtonFlags` (left, right, middle, 4, 5).
/// `RAWKEYBOARD.Flags` bits.
const RI_KEY_BREAK: u32 = 1;
const RI_KEY_E0: u32 = 2;
const BUTTONS_DOWN: u16 = 0x0001 | 0x0004 | 0x0010 | 0x0040 | 0x0100;

#[derive(Default)]
pub struct Input {
    /// The Studio listens: keys and clicks.
    studio: bool,
    /// The tray watches for input from a device that is asleep: from keyboards, from mice.
    watch: (bool, bool),
    /// What is registered with Windows right now: keyboards, mice.
    on: (bool, bool),
    /// Raw Input device handle -> Razer product id (None for other makers).
    pids: HashMap<usize, Option<u16>>,
    /// Keys held, so auto-repeat doesn't count as more presses.
    down: HashSet<u16>,
}

/// What a `WM_INPUT` from a Razer device says.
pub struct Seen {
    /// The device's product id.
    pub pid: u16,
    /// For the Studio: a press, with the key's names (empty for a click). `None` for
    /// anything else (a release, a repeat, a movement) and while the Studio doesn't listen.
    pub press: Option<&'static [&'static str]>,
}

impl Input {
    /// The Studio listens or stops listening.
    pub fn want(&mut self, hwnd: HWND, on: bool) {
        if on != self.studio {
            self.studio = on;
            self.down.clear();
        }
        self.register(hwnd);
    }

    /// Watch for input from a device that is asleep, by its kind (keyboards, mice), or stop.
    pub fn watch(&mut self, hwnd: HWND, keys: bool, mice: bool) {
        self.watch = (keys, mice);
        self.register(hwnd);
    }

    /// Whether a watch that was asked for is on.
    pub fn watching(&self) -> bool {
        (self.watch.0 && self.on.0) || (self.watch.1 && self.on.1)
    }

    /// Ask Windows for what the Studio and the watch need together, kind by kind, and only
    /// for a kind whose need changed: one of them stopping never drops what the other uses.
    /// A change Windows refused (to register, or to stop) is asked for again on the next call.
    fn register(&mut self, hwnd: HWND) {
        let (keys, mice) = needed(self.studio, self.watch);
        for (usage, on, want) in [(KEYBOARD, &mut self.on.0, keys), (MOUSE, &mut self.on.1, mice)] {
            if *on == want {
                continue;
            }
            let device = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: usage,
                // INPUTSINK: also while another window has the focus (this one never does).
                dwFlags: if want { RIDEV_INPUTSINK } else { RIDEV_REMOVE },
                hwndTarget: if want { hwnd } else { null_mut() },
            };
            if unsafe { RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) } != 0 {
                *on = want;
            }
        }
    }

    /// A `WM_INPUT`: `None` unless a Razer device sent it.
    pub fn read(&mut self, lparam: LPARAM) -> Option<Seen> {
        if !self.studio {
            // Only the watch listens: who sent it is all that is read, never the input itself.
            let header = unsafe { read_header(lparam)? };
            return self.pid(header.hDevice).map(|pid| Seen { pid, press: None });
        }
        let raw = unsafe { read_raw(lparam)? };
        let pid = self.pid(raw.header.hDevice)?;
        let press = match raw.header.dwType {
            t if t == RIM_TYPEKEYBOARD => {
                let k = unsafe { raw.data.keyboard };
                let code = k.MakeCode | if k.Flags as u32 & RI_KEY_E0 != 0 { 0xE000 } else { 0 };
                if k.Flags as u32 & RI_KEY_BREAK != 0 {
                    self.down.remove(&code);
                    None
                } else {
                    self.down.insert(code).then(|| key_names(code))
                }
            }
            t if t == RIM_TYPEMOUSE => {
                let flags = unsafe { raw.data.mouse.Anonymous.Anonymous.usButtonFlags };
                (flags & BUTTONS_DOWN != 0).then_some(&[][..])
            }
            _ => None,
        };
        Some(Seen { pid, press })
    }

    fn pid(&mut self, device: HANDLE) -> Option<u16> {
        *self.pids.entry(device as usize).or_insert_with(|| unsafe { razer_pid(device) })
    }
}

/// What to register for: the Studio takes keys and clicks, the watch the kinds it named.
fn needed(studio: bool, watch: (bool, bool)) -> (bool, bool) {
    (studio || watch.0, studio || watch.1)
}

/// The header of a raw input alone: which device sent it, and nothing of what it sent.
unsafe fn read_header(lparam: LPARAM) -> Option<RAWINPUTHEADER> {
    let size_of_header = size_of::<RAWINPUTHEADER>() as u32;
    let mut header: RAWINPUTHEADER = std::mem::zeroed();
    let mut size = size_of_header;
    let got = GetRawInputData(lparam as HRAWINPUT, RID_HEADER, (&mut header as *mut RAWINPUTHEADER).cast(), &mut size, size_of_header);
    (got == size_of_header).then_some(header)
}

unsafe fn read_raw(lparam: LPARAM) -> Option<RAWINPUT> {
    let header = size_of::<RAWINPUTHEADER>() as u32;
    let mut raw: RAWINPUT = std::mem::zeroed();
    let mut size = size_of::<RAWINPUT>() as u32;
    let got = GetRawInputData(lparam as HRAWINPUT, RID_INPUT, (&mut raw as *mut RAWINPUT).cast(), &mut size, header);
    (got != u32::MAX && got >= header).then_some(raw)
}

/// `\\?\HID#VID_1532&PID_0099&MI_00#...` -> 0x0099.
unsafe fn razer_pid(device: HANDLE) -> Option<u16> {
    let mut len = 0u32;
    GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, null_mut(), &mut len);
    if len == 0 || len > 1024 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    if GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, buf.as_mut_ptr().cast(), &mut len) == u32::MAX {
        return None;
    }
    parse_pid(&String::from_utf16_lossy(&buf))
}

fn parse_pid(name: &str) -> Option<u16> {
    let name = name.to_ascii_uppercase();
    if !name.contains("VID_1532") {
        return None;
    }
    let at = name.find("PID_")? + 4;
    u16::from_str_radix(name.get(at..at + 4)?, 16).ok()
}

/// Scan code (set 1, 0xE0xx for extended keys) -> the key's names as the editor's drawing
/// names them; the first that the drawing has wins (ISO boards call 0x2B `iso-hash`).
fn key_names(code: u16) -> &'static [&'static str] {
    match code {
        0x01 => &["esc"],
        0x3B..=0x44 => &FKEYS[(code - 0x3B) as usize..(code - 0x3B + 1) as usize],
        0x57 => &["f11"],
        0x58 => &["f12"],
        0x29 => &["grave"],
        0x02..=0x0B => &DIGITS[(code - 0x02) as usize..(code - 0x02 + 1) as usize],
        0x0C => &["minus"],
        0x0D => &["equal"],
        0x0E => &["backspace"],
        0x0F => &["tab"],
        0x10..=0x19 => &QROW[(code - 0x10) as usize..(code - 0x10 + 1) as usize],
        0x1A => &["lbracket"],
        0x1B => &["rbracket"],
        0x2B => &["backslash", "iso-hash"],
        0x3A => &["capslock"],
        0x1E..=0x26 => &AROW[(code - 0x1E) as usize..(code - 0x1E + 1) as usize],
        0x27 => &["semicolon"],
        0x28 => &["quote"],
        0x1C => &["enter"],
        0x2A => &["lshift"],
        0x56 => &["iso-backslash"],
        0x2C..=0x32 => &ZROW[(code - 0x2C) as usize..(code - 0x2C + 1) as usize],
        0x33 => &["comma"],
        0x34 => &["period"],
        0x35 => &["slash"],
        0x36 => &["rshift"],
        0x1D => &["lctrl"],
        0xE05B => &["lwin"],
        0xE05C => &["rwin", "lwin"],
        0x38 => &["lalt"],
        0x39 => &["space"],
        0xE038 => &["ralt"],
        0xE05D => &["menu"],
        0xE01D => &["rctrl"],
        0xE037 => &["printscreen"],
        0x46 => &["scrolllock"],
        0x45 | 0xE045 => &["numlock", "pause"],
        0xE052 => &["insert"],
        0xE047 => &["home"],
        0xE049 => &["pageup"],
        0xE053 => &["delete"],
        0xE04F => &["end"],
        0xE051 => &["pagedown"],
        0xE048 => &["up"],
        0xE050 => &["down"],
        0xE04B => &["left"],
        0xE04D => &["right"],
        0xE035 => &["num-slash"],
        0x37 => &["num-star"],
        0x4A => &["num-minus"],
        0x4E => &["num-plus"],
        0xE01C => &["num-enter"],
        0x53 => &["num-period"],
        0x52 => &["num0"],
        0x4F => &["num1"],
        0x50 => &["num2"],
        0x51 => &["num3"],
        0x4B => &["num4"],
        0x4C => &["num5"],
        0x4D => &["num6"],
        0x47 => &["num7"],
        0x48 => &["num8"],
        0x49 => &["num9"],
        _ => &[],
    }
}

const FKEYS: [&str; 10] = ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10"];
const DIGITS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
const QROW: [&str; 10] = ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"];
const AROW: [&str; 9] = ["a", "s", "d", "f", "g", "h", "j", "k", "l"];
const ZROW: [&str; 7] = ["z", "x", "c", "v", "b", "n", "m"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_pids() {
        assert_eq!(key_names(0x11), ["w"]);
        assert_eq!(key_names(0x1E), ["a"]);
        assert_eq!(key_names(0x32), ["m"]);
        assert_eq!(key_names(0x44), ["f10"]);
        assert_eq!(key_names(0x0B), ["0"]);
        assert_eq!(key_names(0xE048), ["up"]);
        assert!(key_names(0x7F).is_empty());
        assert_eq!(parse_pid(r"\\?\HID#VID_1532&PID_0099&MI_00#7&1"), Some(0x0099));
        assert_eq!(parse_pid(r"\\?\HID#VID_046D&PID_C52B"), None);
        // The keyboard collection of a DeathStalker V2 Pro receiver, as raw input names it.
        // The same keyboard on its cable is another product (0292).
        assert_eq!(parse_pid(r"\\?\HID#VID_1532&PID_0290&MI_00#a&1463a5&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}"), Some(0x0290));
    }

    #[test]
    fn the_studio_and_the_watch_need_one_registration() {
        // (keyboards, mice). The Studio takes both; the watch only the kind that is asleep.
        assert_eq!(needed(false, (false, false)), (false, false));
        assert_eq!(needed(true, (false, false)), (true, true));
        assert_eq!(needed(false, (true, false)), (true, false));
        assert_eq!(needed(false, (false, true)), (false, true));
        // Both at once, then either one stopping: the other keeps what it uses.
        assert_eq!(needed(true, (true, false)), (true, true));
        assert_eq!(needed(false, (true, false)), (true, false));
        assert_eq!(needed(true, (false, false)), (true, true));
    }

    /// The same against Windows itself: what stays registered when one of the two stops.
    #[test]
    fn one_listener_stopping_keeps_the_other_registered() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe { CreateWindowExW(0, class.as_ptr(), null_mut(), 0, 0, 0, 0, 0, HWND_MESSAGE, null_mut(), null_mut(), null_mut()) };
        assert!(!hwnd.is_null(), "no window to register raw input on");
        let mut input = Input::default();

        input.want(hwnd, true);
        input.watch(hwnd, true, false);
        assert_eq!((registered(), input.watching()), ((true, true), true));
        input.watch(hwnd, false, false); // the device answered: the Studio still listens
        assert_eq!((registered(), input.watching()), ((true, true), false));
        input.watch(hwnd, false, true);
        input.want(hwnd, false); // the scene ended: the watch still listens, for mice only
        assert_eq!((registered(), input.watching()), ((false, true), true));
        input.watch(hwnd, false, false);
        assert_eq!(registered(), (false, false));

        unsafe { DestroyWindow(hwnd) };
    }

    /// What this process has registered with Windows: (keyboards, mice).
    fn registered() -> (bool, bool) {
        let size = size_of::<RAWINPUTDEVICE>() as u32;
        let mut list: [RAWINPUTDEVICE; 8] = unsafe { std::mem::zeroed() };
        let mut count = list.len() as u32;
        let got = unsafe { GetRegisteredRawInputDevices(list.as_mut_ptr(), &mut count, size) };
        assert_ne!(got, u32::MAX, "GetRegisteredRawInputDevices failed");
        let has = |usage| list[..got as usize].iter().any(|d| d.usUsagePage == 1 && d.usUsage == usage);
        (has(KEYBOARD), has(MOUSE))
    }
}
