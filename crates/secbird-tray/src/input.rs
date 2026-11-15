//! Key presses and clicks for the Reactive and Ripple effects, through Raw Input on the tray
//! window. Registered only while a scene uses them; only Razer devices count, and nothing but
//! "which key went down on which device" leaves this module.

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
    on: bool,
    /// Raw Input device handle -> Razer product id (None for other makers).
    pids: HashMap<usize, Option<u16>>,
    /// Keys held, so auto-repeat doesn't count as more presses.
    down: HashSet<u16>,
}

impl Input {
    /// Listen or stop listening.
    pub fn want(&mut self, hwnd: HWND, on: bool) {
        if on == self.on {
            return;
        }
        let flags = if on { RIDEV_INPUTSINK } else { RIDEV_REMOVE };
        let target = if on { hwnd } else { null_mut() };
        let list = [
            RAWINPUTDEVICE { usUsagePage: 1, usUsage: KEYBOARD, dwFlags: flags, hwndTarget: target },
            RAWINPUTDEVICE { usUsagePage: 1, usUsage: MOUSE, dwFlags: flags, hwndTarget: target },
        ];
        let ok = unsafe { RegisterRawInputDevices(list.as_ptr(), list.len() as u32, size_of::<RAWINPUTDEVICE>() as u32) } != 0;
        self.on = on && ok;
        self.down.clear();
    }

    /// A `WM_INPUT`: the Razer device's product id and the key's names (empty for a click),
    /// when it is a press.
    pub fn read(&mut self, lparam: LPARAM) -> Option<(u16, &'static [&'static str])> {
        let raw = unsafe { read_raw(lparam)? };
        let pid = self.pid(raw.header.hDevice)?;
        match raw.header.dwType {
            t if t == RIM_TYPEKEYBOARD => {
                let k = unsafe { raw.data.keyboard };
                let code = k.MakeCode | if k.Flags as u32 & RI_KEY_E0 != 0 { 0xE000 } else { 0 };
                if k.Flags as u32 & RI_KEY_BREAK != 0 {
                    self.down.remove(&code);
                    return None;
                }
                self.down.insert(code).then(|| (pid, key_names(code)))
            }
            t if t == RIM_TYPEMOUSE => {
                let flags = unsafe { raw.data.mouse.Anonymous.Anonymous.usButtonFlags };
                (flags & BUTTONS_DOWN != 0).then_some((pid, &[][..]))
            }
            _ => None,
        }
    }

    fn pid(&mut self, device: HANDLE) -> Option<u16> {
        *self.pids.entry(device as usize).or_insert_with(|| unsafe { razer_pid(device) })
    }
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
    }
}
