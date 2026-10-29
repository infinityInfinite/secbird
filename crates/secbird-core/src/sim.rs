//! Simulated Razer devices that answer Protocol 2.5 the way real hardware does.
//!
//! Tests use them to exercise discovery, retries, lighting and profiles without a
//! device on the desk. Setting `SECBIRD_SIM=all` (or a list of [`PRESETS`]) also makes the
//! CLI and tray show them, which is handy for working on the editor.
//!
//! The simulated product ids (`FFF1`-`FFF3`) are outside Razer's range, so they
//! can't be confused with real devices or their saved profiles.

use crate::commands as cmd;
use crate::db::DeviceSpec;
use crate::device::{Device, TXID_CANDIDATES};
use crate::error::{Error, Result};
use crate::proto::{checksum, Command, Report, Status, REPORT_LEN};
use crate::transport::Transport;
use std::collections::BTreeMap;
use std::ffi::CString;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

pub const MOUSE_PID: u16 = 0xFFF1;
pub const WIRELESS_MOUSE_PID: u16 = 0xFFF2;
pub const KEYBOARD_PID: u16 = 0xFFF3;
pub const OLD_KEYBOARD_PID: u16 = 0xFFF4;
pub const NAGA_PID: u16 = 0xFFF5;

const MOUSE_SPEC: &str = r#"{"pid":"FFF1","name":"Simulated Mouse","kind":"mouse","interface":0,
  "dpi":{"min":100,"max":26000,"step":50},"polling":"standard",
  "leds":[{"name":"scroll","id":1},{"name":"logo","id":4},{"name":"underglow","id":10}],
  "source":"simulator"}"#;

const WIRELESS_MOUSE_SPEC: &str = r#"{"pid":"FFF2","name":"Simulated Wireless Mouse","kind":"mouse",
  "interface":0,"dpi":{"min":100,"max":30000,"step":50},"polling":"standard",
  "leds":[{"name":"logo","id":4}],"wireless":true,"source":"simulator"}"#;

const NAGA_SPEC: &str = r#"{"pid":"FFF5","name":"Simulated Naga","kind":"mouse",
  "interface":0,"dpi":{"min":100,"max":30000,"step":50},"polling":"standard",
  "leds":[{"name":"logo","id":4}],"wireless":true,"source":"simulator"}"#;

const OLD_KEYBOARD_SPEC: &str = r#"{"pid":"FFF4","name":"Simulated Keyboard (older)","kind":"keyboard",
  "interface":0,"polling":"standard","lighting":"standard",
  "leds":[{"name":"backlight","id":5},{"name":"logo","id":4}],
  "matrix":{"rows":6,"cols":22},"source":"simulator"}"#;

const KEYBOARD_SPEC: &str = r#"{"pid":"FFF3","name":"Simulated Keyboard","kind":"keyboard",
  "interface":0,"polling":"standard","leds":[{"name":"backlight","id":5}],
  "matrix":{"rows":6,"cols":22},"source":"simulator"}"#;

/// A stored macro: its 250-byte header and its steps.
pub type SimMacro = (Vec<u8>, Vec<u8>);

/// Everything the simulated device remembers. Tests read and tweak it via [`Sim::state`].
#[derive(Debug, Clone)]
pub struct SimState {
    /// The only transaction id the device answers; others get no reply.
    pub txid: u8,
    /// A mouse dock's own light, answering [`crate::device::DOCK_TXID`] like the #39 dock:
    /// the last `0F:02` it took on LED 15 (`[effect, flags, rate, ...]`). It refuses the
    /// Battery Level effect (14). `None`: no dock answers that txid.
    pub dock_light: Option<Vec<u8>>,
    pub serial: String,
    pub firmware: (u8, u8),
    /// `None` = the device has no DPI sensor (keyboards).
    pub dpi: Option<(u16, u16)>,
    pub polling_code: u8,
    /// High-speed polling code (`00:C0`), or `None` when the device doesn't have it.
    pub polling_hs: Option<u8>,
    /// DPI stages as the device reports them: `(active, [(x, y)])`.
    pub dpi_stages: Option<(u8, Vec<(u16, u16)>)>,
    /// Sleep timer in seconds (`07:83`), and the auto-sleep switch where there is one.
    pub idle_seconds: Option<u16>,
    /// Laptops: Smart Dimming as (enabled, level), level 0 half, 1 quarter, 2 dark.
    pub smart_dimming: Option<(u8, u8)>,
    /// Keyboards: gaming mode bits (`00:08`); `None` where the device has none.
    pub game_mode: Option<u8>,
    pub auto_sleep: Option<bool>,
    /// The auto-sleep switch saved in the on-board slot, which the live one starts from.
    pub auto_sleep_saved: Option<bool>,
    /// Active on-board slot (`05:84`), when set. Lighting written to another slot is kept
    /// but not shown, like the DeathStalker V2 Pro in #37.
    pub active_slot: Option<u8>,
    /// On-device macro memory (`None` = no macros): id -> (header, steps). Sized like a Basilisk V3.
    pub macros: Option<BTreeMap<u16, SimMacro>>,
    /// Bytes freed by deleted macros, not reusable until the memory is compacted.
    pub macro_recyclable: u32,
    /// What a receiver reports as paired (`00:BF`): `(connected, pid)`.
    pub paired: Vec<(bool, u16)>,
    pub battery: Option<u8>,
    pub charging: bool,
    /// LED ids that exist on the device.
    pub leds: Vec<u8>,
    pub brightness: BTreeMap<u8, u8>,
    /// Brightness shown now after a write that isn't saved (store 0). Reads still answer
    /// the saved level, as a real Basilisk V3 does.
    pub live_brightness: BTreeMap<u8, u8>,
    /// Last effect per LED: `[effect, flags, rate, ...colors]`.
    pub effects: BTreeMap<u8, Vec<u8>>,
    /// Per-key colors `rows x cols`, row-major.
    pub matrix: Option<(u8, u8, Vec<[u8; 3]>)>,
    /// Speaks the older `03:xx` lighting commands instead of `0F:xx`.
    pub standard: bool,
    /// Older command set: per-LED on/off and color, and the whole-grid effect.
    pub led_on: BTreeMap<u8, bool>,
    pub led_rgb: BTreeMap<u8, [u8; 3]>,
    pub grid_effect: Vec<u8>,
    /// Button assignments by (slot, button id, layer): (function, data).
    pub buttons: BTreeMap<(u8, u8, u8), (u8, [u8; 5])>,
    /// The mouse's own button id list (02:84), or `None` for a device that doesn't answer it.
    pub button_list: Option<Vec<u8>>,
    /// Keyboard key assignments (02:0D) by (slot, key id, layer): (function, data). Empty:
    /// the device doesn't answer key commands.
    pub keys: BTreeMap<(u8, u8, u8), (u8, [u8; 5])>,
    /// Answer this many reads with Busy before the real reply.
    pub busy_reads: u32,
    /// Every well-formed command received, in order.
    pub log: Vec<Command>,
    reply: Option<Report>,
}

impl SimState {
    fn new(serial: &str) -> Self {
        SimState {
            txid: 0x1F,
            serial: serial.into(),
            firmware: (1, 2),
            dpi: None,
            polling_code: 0x01,
            polling_hs: None,
            dpi_stages: None,
            macros: None,
            macro_recyclable: 0,
            idle_seconds: None,
            smart_dimming: None,
            game_mode: None,
            auto_sleep: None,
            auto_sleep_saved: None,
            active_slot: None,
            paired: Vec::new(),
            dock_light: None,
            battery: None,
            charging: false,
            leds: Vec::new(),
            brightness: BTreeMap::new(),
            live_brightness: BTreeMap::new(),
            effects: BTreeMap::new(),
            matrix: None,
            standard: false,
            led_on: BTreeMap::new(),
            led_rgb: BTreeMap::new(),
            grid_effect: Vec::new(),
            buttons: BTreeMap::new(),
            button_list: None,
            keys: BTreeMap::new(),
            busy_reads: 0,
            log: Vec::new(),
            reply: None,
        }
    }

    /// A write to a saved slot that isn't the active one: accepted, but not what shows.
    /// The dock's own light (see [`SimState::dock_light`]).
    fn handle_dock(&mut self, c: Command, a: &[u8], size: u8) -> (Status, Vec<u8>) {
        let led = crate::device::DOCK_LED;
        match c {
            cmd::MATRIX_EFFECT if a[1] == led && a[2] == 14 => (Status::NotSupported, vec![]),
            cmd::MATRIX_EFFECT if a[1] == led => {
                self.dock_light = Some(a[2..size as usize].to_vec());
                (Status::Success, a[..size as usize].to_vec())
            }
            cmd::MATRIX_BRIGHTNESS if a[1] == led => (Status::Success, a[..3].to_vec()),
            _ => (Status::Failed, vec![]),
        }
    }

    fn hidden_slot(&self, store: u8) -> bool {
        store != 0 && self.active_slot.is_some_and(|slot| slot != store)
    }

    /// Macro memory commands, like the Basilisk V3: ids 1-100, 90 KB.
    fn handle_macro(&mut self, c: Command, a: &[u8], size: u8) -> (Status, Vec<u8>) {
        use Status::{Failed, NotSupported, Success};
        const MAX_BYTES: u32 = 92160;
        let recyclable = self.macro_recyclable;
        let Some(macros) = self.macros.as_mut() else { return (NotSupported, vec![]) };
        let used: u32 = macros.values().map(|(_, d)| crate::macros::footprint(d.len() as u32)).sum();
        let id = u16::from_be_bytes([a[0], a[1]]);
        let be32 = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
        match c {
            c if c == cmd::MACRO_STORAGE.get() => {
                let mut v = vec![0, 100];
                for n in [MAX_BYTES, MAX_BYTES - used - recyclable, recyclable] {
                    v.extend(n.to_be_bytes());
                }
                (Success, v)
            }
            c if c == cmd::MACRO_COUNT.get() => (Success, (macros.len() as u16).to_be_bytes().to_vec()),
            c if c == cmd::MACRO_IDS.get() => {
                let offset = id as usize; // the first two bytes are the offset here
                let n = (size as usize - 4) / 2;
                let mut v = vec![a[0], a[1], 0, n as u8];
                v.extend(macros.keys().skip(offset).take(n).flat_map(|k| k.to_be_bytes()));
                (Success, v)
            }
            cmd::MACRO_ALLOCATE => {
                if id == 0 || id > 100 || macros.contains_key(&id) {
                    return (Failed, vec![]);
                }
                macros.insert(id, (vec![0; cmd::MACRO_HEADER_LEN], vec![0; be32(&a[2..])]));
                (Success, a[..6].to_vec())
            }
            c if c == cmd::MACRO_ALLOCATE.get() => match macros.get(&id) {
                Some((_, d)) => {
                    let mut v = id.to_be_bytes().to_vec();
                    v.extend((d.len() as u32).to_be_bytes());
                    (Success, v)
                }
                None => (Failed, vec![]),
            },
            cmd::MACRO_NAME | cmd::MACRO_DATA => {
                let (start, at, len) = if c == cmd::MACRO_NAME {
                    (6, u16::from_be_bytes([a[2], a[3]]) as usize, size as usize - 6)
                } else {
                    (7, be32(&a[2..]), a[6] as usize)
                };
                let Some(entry) = macros.get_mut(&id) else { return (Failed, vec![]) };
                let buf = if c == cmd::MACRO_NAME { &mut entry.0 } else { &mut entry.1 };
                if at + len > buf.len() {
                    return (Failed, vec![]);
                }
                buf[at..at + len].copy_from_slice(&a[start..start + len]);
                (Success, a[..size as usize].to_vec())
            }
            c if c == cmd::MACRO_NAME.get() || c == cmd::MACRO_DATA.get() => {
                let Some(entry) = macros.get(&id) else { return (Failed, vec![]) };
                let (head, at, len, buf) = if c == cmd::MACRO_NAME.get() {
                    (6, u16::from_be_bytes([a[2], a[3]]) as usize, size as usize - 6, &entry.0)
                } else {
                    (7, be32(&a[2..]), a[6] as usize, &entry.1)
                };
                let end = (at + len).min(buf.len());
                let mut v = a[..head].to_vec();
                if c == cmd::MACRO_DATA.get() {
                    v[6] = end.saturating_sub(at) as u8;
                }
                v.extend_from_slice(buf.get(at..end).unwrap_or(&[]));
                (Success, v)
            }
            cmd::MACRO_DELETE => match macros.remove(&id) {
                Some((_, d)) => {
                    self.macro_recyclable += crate::macros::footprint(d.len() as u32);
                    (Success, a[..2].to_vec())
                }
                None => (Failed, vec![]),
            },
            _ => (NotSupported, vec![]),
        }
    }

    /// Per-key color at `(row, col)`.
    pub fn key(&self, row: u8, col: u8) -> Option<[u8; 3]> {
        let (rows, cols, frame) = self.matrix.as_ref()?;
        (row < *rows && col < *cols).then(|| frame[row as usize * *cols as usize + col as usize])
    }

    fn handle(&mut self, c: Command, a: &[u8], size: u8) -> (Status, Vec<u8>) {
        use Status::{Failed, NotSupported, Success};
        let ok = |v: Vec<u8>| (Success, v);
        // Each device speaks one lighting generation and ignores the other.
        let old_lighting = c.class == 0x03 && c.id & 0x80 == 0;
        if (self.standard && c.class == 0x0F) || (!self.standard && old_lighting) {
            return (NotSupported, vec![]);
        }
        let known = |s: &Self, led: u8| s.leds.contains(&led);
        match c {
            c if c == cmd::FIRMWARE.get() => ok(vec![self.firmware.0, self.firmware.1]),
            c if c == cmd::SERIAL.get() => {
                let mut v = self.serial.as_bytes().to_vec();
                v.resize(cmd::SERIAL_SIZE as usize, 0);
                ok(v)
            }
            c if c == cmd::DPI.get() => match self.dpi {
                Some((x, y)) => {
                    let ([xh, xl], [yh, yl]) = (x.to_be_bytes(), y.to_be_bytes());
                    ok(vec![a[0], xh, xl, yh, yl, 0, 0])
                }
                None => (NotSupported, vec![]),
            },
            cmd::DPI => {
                let x = u16::from_be_bytes([a[1], a[2]]);
                let y = u16::from_be_bytes([a[3], a[4]]);
                match self.dpi {
                    None => (NotSupported, vec![]),
                    Some(_) if x == 0 || y == 0 => (Failed, vec![]),
                    Some(_) => {
                        self.dpi = Some((x, y));
                        ok(a[..7].to_vec())
                    }
                }
            }
            c if c == cmd::BUTTON.get() => match self.buttons.get(&(a[0], a[1], a[2])) {
                // Like the real mouse: size byte 1 whatever the function's data length.
                Some(&(function, d)) => ok(vec![a[0], a[1], a[2], function, 1, d[0], d[1], d[2], d[3], d[4]]),
                None => (Failed, vec![]),
            },
            c if c == cmd::BUTTON_IDS.get() => match &self.button_list {
                Some(ids) => ok([vec![ids.len() as u8], ids.clone()].concat()),
                None => (NotSupported, vec![]),
            },
            cmd::BUTTON => {
                if !self.buttons.contains_key(&(1, a[1], a[2])) {
                    return (Failed, vec![]);
                }
                self.buttons.insert((a[0], a[1], a[2]), (a[3], [a[5], a[6], a[7], a[8], a[9]]));
                ok(a[..10].to_vec())
            }
            c if c == cmd::KEY.get() => match self.keys.get(&(a[0], a[1], a[2])) {
                Some(&(function, d)) => ok(vec![a[0], a[1], a[2], function, 1, d[0], d[1], d[2], d[3], d[4]]),
                None if self.keys.is_empty() => (NotSupported, vec![]),
                None => (Failed, vec![]),
            },
            cmd::KEY => {
                if !self.keys.contains_key(&(1, a[1], a[2])) {
                    return (if self.keys.is_empty() { NotSupported } else { Failed }, vec![]);
                }
                self.keys.insert((a[0], a[1], a[2]), (a[3], [a[5], a[6], a[7], a[8], a[9]]));
                ok(a[..10].to_vec())
            }
            c if c == cmd::KEY_IDS => {
                if self.keys.is_empty() {
                    return (NotSupported, vec![]);
                }
                let ids = self.keys.keys().filter(|k| k.0 == a[0] && k.2 == a[1]).map(|k| k.1);
                ok([a[0], a[1]].into_iter().chain(ids).collect())
            }
            c if c == cmd::ACTIVE_SLOT.get() => match (self.active_slot, self.buttons.is_empty()) {
                (Some(slot), _) => ok(vec![slot]),
                (None, true) => (NotSupported, vec![]),
                (None, false) => ok(vec![1]),
            },
            c if c == cmd::POLLING.get() => ok(vec![self.polling_code]),
            cmd::POLLING => {
                if cmd::polling_hz(a[0]).is_none() {
                    return (Failed, vec![]);
                }
                self.polling_code = a[0];
                ok(vec![a[0]])
            }
            c if c == cmd::DPI_STAGES.get() => match &self.dpi_stages {
                Some((active, stages)) => {
                    let mut v = vec![a[0], *active, stages.len() as u8];
                    for (i, &(x, y)) in stages.iter().enumerate() {
                        let ([xh, xl], [yh, yl]) = (x.to_be_bytes(), y.to_be_bytes());
                        v.extend([i as u8 + 1, xh, xl, yh, yl, 0, 0]);
                    }
                    ok(v)
                }
                None => (NotSupported, vec![]),
            },
            cmd::DPI_STAGES => {
                let (active, n) = (a[1], a[2] as usize);
                let well_formed = (1..=cmd::MAX_DPI_STAGES).contains(&n)
                    && (1..=n as u8).contains(&active)
                    && size as usize == 7 * n + 3
                    && (0..n).all(|i| a[3 + 7 * i] == i as u8);
                match (&self.dpi_stages, well_formed) {
                    (None, _) => (NotSupported, vec![]),
                    (Some(_), false) => (Failed, vec![]),
                    (Some(_), true) => {
                        let stages: Vec<(u16, u16)> = (0..n)
                            .map(|i| {
                                let s = &a[3 + 7 * i..];
                                (u16::from_be_bytes([s[1], s[2]]), u16::from_be_bytes([s[3], s[4]]))
                            })
                            .collect();
                        self.dpi = Some(stages[active as usize - 1]);
                        self.dpi_stages = Some((active, stages));
                        ok(a[..size as usize].to_vec())
                    }
                }
            }
            c if c == cmd::IDLE_TIME.get() => match self.idle_seconds {
                Some(secs) => ok(secs.to_be_bytes().to_vec()),
                None => (NotSupported, vec![]),
            },
            cmd::IDLE_TIME => match self.idle_seconds {
                Some(_) => {
                    // Like most mice: 1 to 15 minutes, or up to 60 with an auto-sleep switch.
                    let secs = u16::from_be_bytes([a[0], a[1]]);
                    let max = if self.auto_sleep.is_some() { 3600 } else { 900 };
                    if (60..=max).contains(&secs) {
                        self.idle_seconds = Some(secs);
                    }
                    ok(a[..2].to_vec())
                }
                None => (NotSupported, vec![]),
            },
            c if c == cmd::SMART_DIMMING.get() => match self.smart_dimming {
                Some((on, level)) => ok(vec![a[0], on, level]),
                None => (NotSupported, vec![]),
            },
            cmd::SMART_DIMMING => match self.smart_dimming {
                Some(_) if a[1] <= 1 && a[2] <= 2 => {
                    self.smart_dimming = Some((a[1], a[2]));
                    ok(a[..3].to_vec())
                }
                _ => (NotSupported, vec![]),
            },
            c if c == cmd::GAME_MODE.get() => match self.game_mode {
                Some(bits) => ok(vec![bits, 0, 0, 0]),
                None => (NotSupported, vec![]),
            },
            cmd::GAME_MODE => match self.game_mode {
                Some(_) => {
                    self.game_mode = Some(a[0]);
                    ok(a[..4].to_vec())
                }
                None => (NotSupported, vec![]),
            },
            c if c == cmd::AUTO_SLEEP.get() => match self.auto_sleep {
                Some(on) => ok(vec![0, on as u8]),
                None => (NotSupported, vec![]),
            },
            cmd::AUTO_SLEEP => match self.auto_sleep {
                Some(_) if a[0] != 0 => {
                    self.auto_sleep_saved = Some(a[1] == 1);
                    ok(a[..2].to_vec())
                }
                Some(_) => {
                    self.auto_sleep = Some(a[1] == 1);
                    ok(a[..2].to_vec())
                }
                None => (NotSupported, vec![]),
            },
            c if c.class == 0x06 => self.handle_macro(c, a, size),
            c if c == cmd::POLLING_HS.get() => match self.polling_hs {
                Some(code) => ok(vec![0, code]),
                None => (NotSupported, vec![]),
            },
            cmd::POLLING_HS => match (self.polling_hs, cmd::polling_hs_hz(a[1])) {
                (Some(_), Some(_)) => {
                    self.polling_hs = Some(a[1]);
                    ok(vec![0, a[1]])
                }
                (None, _) => (NotSupported, vec![]),
                (Some(_), None) => (Failed, vec![]),
            },
            c if c == cmd::WIRELESS_DEVICES.get() => match self.paired.is_empty() {
                true => (NotSupported, vec![]),
                false => {
                    let mut v = vec![self.paired.len() as u8];
                    for &(connected, pid) in &self.paired {
                        v.push(connected as u8);
                        v.extend(pid.to_be_bytes());
                    }
                    ok(v)
                }
            },
            c if c == cmd::BATTERY.get() => match self.battery {
                Some(level) => ok(vec![0, level]),
                None => (NotSupported, vec![]),
            },
            c if c == cmd::CHARGING.get() => match self.battery {
                Some(_) => ok(vec![0, self.charging as u8]),
                None => (NotSupported, vec![]),
            },
            c if c == cmd::MATRIX_BRIGHTNESS.get() => match self.brightness.get(&a[1]) {
                Some(&v) => ok(vec![a[0], a[1], v]),
                None if self.leds.contains(&a[1]) => ok(vec![a[0], a[1], 255]),
                None => (Failed, vec![]),
            },
            cmd::MATRIX_BRIGHTNESS => {
                if !self.leds.contains(&a[1]) {
                    return (Failed, vec![]);
                }
                if self.hidden_slot(a[0]) {
                    return ok(a[..3].to_vec());
                }
                if a[0] != cmd::NOSTORE {
                    self.brightness.insert(a[1], a[2]);
                }
                self.live_brightness.insert(a[1], a[2]);
                ok(a[..3].to_vec())
            }
            cmd::MATRIX_EFFECT => {
                // LED 0 addresses the whole device (used by the custom-frame effect).
                let led = a[1];
                if led != 0 && !self.leds.contains(&led) {
                    return (Failed, vec![]);
                }
                if a[2] == cmd::EFFECT_CUSTOM_FRAME && self.matrix.is_none() {
                    return (NotSupported, vec![]);
                }
                if self.hidden_slot(a[0]) {
                    return ok(a[..size as usize].to_vec());
                }
                self.effects.insert(led, a[2..size as usize].to_vec());
                ok(a[..size as usize].to_vec())
            }
            cmd::MATRIX_FRAME => {
                let Some((rows, cols, frame)) = self.matrix.as_mut() else {
                    return (NotSupported, vec![]);
                };
                let (row, start, end) = (a[2], a[3] as usize, a[4] as usize);
                let keys = end.wrapping_sub(start).wrapping_add(1);
                if row >= *rows || end < start || end >= *cols as usize || size as usize != 5 + 3 * keys {
                    return (Failed, vec![]);
                }
                let base = row as usize * *cols as usize;
                for (i, rgb) in a[5..5 + 3 * keys].chunks(3).enumerate() {
                    frame[base + start + i] = [rgb[0], rgb[1], rgb[2]];
                }
                ok(vec![])
            }
            cmd::LED_STATE | cmd::LED_RGB | cmd::LED_EFFECT | cmd::LED_BRIGHTNESS if !known(self, a[1]) => {
                (Failed, vec![])
            }
            cmd::LED_STATE => {
                self.led_on.insert(a[1], a[2] != 0);
                ok(a[..3].to_vec())
            }
            cmd::LED_RGB => {
                self.led_rgb.insert(a[1], [a[2], a[3], a[4]]);
                ok(a[..5].to_vec())
            }
            cmd::LED_EFFECT => {
                self.effects.insert(a[1], vec![a[2]]);
                ok(a[..3].to_vec())
            }
            cmd::LED_BRIGHTNESS => {
                self.brightness.insert(a[1], a[2]);
                ok(a[..3].to_vec())
            }
            cmd::GRID_EFFECT => {
                if self.matrix.is_none() {
                    return (NotSupported, vec![]);
                }
                self.grid_effect = a[..size as usize].to_vec();
                ok(self.grid_effect.clone())
            }
            cmd::GRID_FRAME => {
                let Some((rows, cols, frame)) = self.matrix.as_mut() else {
                    return (NotSupported, vec![]);
                };
                let (row, start, end) = (a[1], a[2] as usize, a[3] as usize);
                let keys = end.wrapping_sub(start).wrapping_add(1);
                if size != cmd::GRID_FRAME_SIZE || a[0] != 0xFF || row >= *rows || end < start
                    || end >= *cols as usize || 4 + 3 * keys > 80
                {
                    return (Failed, vec![]);
                }
                let base = row as usize * *cols as usize;
                for (i, rgb) in a[4..4 + 3 * keys].chunks(3).enumerate() {
                    frame[base + start + i] = [rgb[0], rgb[1], rgb[2]];
                }
                ok(vec![])
            }
            _ => (NotSupported, vec![]),
        }
    }
}

/// A simulated device. Clones share state, so a test can keep one and hand another
/// to a [`Device`].
#[derive(Clone)]
pub struct Sim(Arc<Mutex<SimState>>);

impl Sim {
    pub fn new(state: SimState) -> Self {
        Sim(Arc::new(Mutex::new(state)))
    }

    /// Wired mouse shaped like a Basilisk V3: DPI, polling, three lighting zones.
    pub fn mouse() -> Self {
        let mut s = SimState::new("SIMMOUSE0001");
        s.dpi = Some((800, 800));
        s.dpi_stages = Some((2, vec![(400, 400), (800, 800), (1000, 1000), (1300, 1300), (6400, 6400)]));
        s.macros = Some(BTreeMap::new());
        s.leds = vec![1, 4, 10];
        // The Basilisk V3's factory buttons, as read from the real mouse, in the live
        // slot and the active saved slot, both layers.
        let factory: [(u8, u8, [u8; 5]); 9] = [
            (1, 1, [1, 0, 0, 0, 0]),
            (2, 1, [2, 0, 0, 0, 0]),
            (3, 1, [3, 0, 0, 0, 0]),
            (4, 1, [4, 0, 0, 0, 0]),
            (5, 1, [5, 0, 0, 0, 0]),
            (14, 7, [4, 0, 0, 0, 0]),
            (15, 6, [5, 0x01, 0x90, 0x01, 0x90]),
            (52, 14, [0x68, 0, 0x14, 0, 0]),
            (96, 6, [6, 0, 0, 0, 0]),
        ];
        for slot in [0, 1] {
            for layer in [0, 1] {
                for (id, function, data) in factory {
                    s.buttons.insert((slot, id, layer), (function, data));
                }
            }
        }
        Sim::new(s)
    }

    pub fn wireless_mouse() -> Self {
        let mut s = SimState::new("SIMWLMOUSE01");
        s.txid = 0x3F;
        s.dpi = Some((1600, 1600));
        s.battery = Some(191); // 75 %
        s.idle_seconds = Some(300);
        s.charging = true;
        s.leds = vec![4];
        Sim::new(s)
    }

    /// Wireless Naga with the 12-button side grid (factory keys 1-9, 0, -, =).
    pub fn naga() -> Self {
        let mut s = SimState::new("SIMNAGA00001");
        s.dpi = Some((1600, 1600));
        s.dpi_stages = Some((3, vec![(400, 400), (800, 800), (1600, 1600), (3200, 3200), (6400, 6400)]));
        s.battery = Some(0xE9);
        s.idle_seconds = Some(180);
        s.auto_sleep = Some(true);
        s.macros = Some(BTreeMap::new());
        s.leds = vec![4];
        let mut factory: Vec<(u8, u8, [u8; 5])> = vec![
            (1, 1, [1, 0, 0, 0, 0]),
            (2, 1, [2, 0, 0, 0, 0]),
            (3, 1, [3, 0, 0, 0, 0]),
            (4, 1, [4, 0, 0, 0, 0]),
            (5, 1, [5, 0, 0, 0, 0]),
            (52, 14, [0x68, 0, 0x14, 0, 0]),
            (53, 14, [0x69, 0, 0x14, 0, 0]),
            (96, 6, [6, 0, 0, 0, 0]),
        ];
        let grid_keys = [0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E];
        factory.extend(grid_keys.iter().enumerate().map(|(i, &usage)| (64 + i as u8, 2, [0, usage, 0, 0, 0])));
        for slot in [0, 1] {
            for layer in [0, 1] {
                for &(id, function, data) in &factory {
                    s.buttons.insert((slot, id, layer), (function, data));
                }
            }
        }
        Sim::new(s)
    }

    /// Full-size keyboard: one backlight zone and a 6 x 22 per-key grid.
    pub fn keyboard() -> Self {
        let mut s = SimState::new("SIMKEYBOARD1");
        s.txid = 0xFF;
        s.leds = vec![5];
        s.matrix = Some((6, 22, vec![[0; 3]; 6 * 22]));
        s.game_mode = Some(0);
        with_keys(&mut s);
        Sim::new(s)
    }

    /// Pre-2019 style keyboard: older `03:xx` lighting commands, a logo LED and a 6 x 22 grid.
    pub fn old_keyboard() -> Self {
        let mut s = SimState::new("SIMOLDKEYBD1");
        s.txid = 0xFF;
        s.standard = true;
        s.leds = vec![5, 4];
        s.matrix = Some((6, 22, vec![[0; 3]; 6 * 22]));
        Sim::new(s)
    }

    pub fn state(&self) -> MutexGuard<'_, SimState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The database entry this simulator pretends to be, if it's one of the built-ins.
    pub fn spec_for(pid: u16) -> Option<DeviceSpec> {
        let json = match pid {
            MOUSE_PID => MOUSE_SPEC,
            WIRELESS_MOUSE_PID => WIRELESS_MOUSE_SPEC,
            KEYBOARD_PID => KEYBOARD_SPEC,
            OLD_KEYBOARD_PID => OLD_KEYBOARD_SPEC,
            NAGA_PID => NAGA_SPEC,
            _ => return None,
        };
        Some(serde_json::from_str(json).expect("built-in simulator spec"))
    }

    /// Open as a [`Device`], probing the txid like real discovery does.
    pub fn open(&self, pid: u16, spec: Option<DeviceSpec>) -> Option<Device> {
        let path = CString::new(format!("sim:{pid:04X}")).expect("no NUL");
        Device::open(Box::new(self.clone()), pid, 0, path, &TXID_CANDIDATES, spec)
    }
}

impl Transport for Sim {
    fn send_feature_report(&self, data: &[u8]) -> Result<()> {
        if data.len() != REPORT_LEN + 1 || data[0] != 0 {
            return Err(Error::InvalidArg(format!("simulator: bad report ({} bytes)", data.len())));
        }
        let mut body = [0u8; REPORT_LEN];
        body.copy_from_slice(&data[1..]);
        let req = Report(body);
        let mut s = self.state();
        let command = req.command();

        let to_dock = req.txid() == crate::device::DOCK_TXID && req.txid() != s.txid && s.dock_light.is_some();
        if req.txid() != s.txid && !to_dock {
            s.reply = None; // a real device ignores other transaction ids
            return Ok(());
        }
        let (status, args) = if checksum(&body) != body[88] {
            (Status::Failed, Vec::new())
        } else if to_dock {
            s.handle_dock(command, req.args(), body[5])
        } else {
            s.log.push(command);
            s.handle(command, req.args(), body[5])
        };
        let mut reply = Report::request(req.txid(), command, body[5], &args);
        reply.0[0] = match status {
            Status::New => 0,
            Status::Busy => 1,
            Status::Success => 2,
            Status::Failed => 3,
            Status::Timeout => 4,
            Status::NotSupported => 5,
            Status::Unknown(b) => b,
        };
        reply.0[88] = checksum(&reply.0);
        s.reply = Some(reply);
        Ok(())
    }

    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize> {
        let mut s = self.state();
        let mut out = s.reply.clone().map(|r| r.0).unwrap_or([0; REPORT_LEN]);
        if s.reply.is_some() && s.busy_reads > 0 {
            s.busy_reads -= 1;
            out[0] = 1;
        }
        let n = buf.len().min(REPORT_LEN + 1);
        buf[0] = 0;
        buf[1..n].copy_from_slice(&out[..n - 1]);
        Ok(n)
    }

    fn read_delay(&self) -> Duration {
        Duration::ZERO
    }
}

/// Simulated devices listed in `SECBIRD_SIM`: `all`, or comma-separated [`PRESETS`] names.
pub fn from_env() -> Vec<Device> {
    let Ok(list) = std::env::var("SECBIRD_SIM") else { return Vec::new() };
    let names: Vec<&str> = match list.trim() {
        "all" => PRESETS.to_vec(),
        list => list.split(',').map(str::trim).collect(),
    };
    let db = crate::db::Db::builtin().unwrap_or_default();
    names
        .into_iter()
        .filter_map(|name| {
            let (sim, pid) = preset(name)?;
            // Fake product ids carry their own spec; real ones use the device list, as for hardware.
            let spec = Sim::spec_for(pid).or_else(|| db.get(pid).cloned());
            let mut dev = sim.open(pid, spec)?;
            dev.identify_receiver(&db);
            Some(dev)
        })
        .collect()
}

/// Every preset `SECBIRD_SIM=all` loads.
pub const PRESETS: [&str; 13] =
    ["mouse", "wireless", "keyboard", "old-keyboard", "naga", "cobra", "dock", "hyperpolling", "blackwidow", "goliathus", "firefly", "blade", "base-station"];

/// A simulated device by name. The later ones copy devices users reported, under their real
/// product ids, so the editor treats them exactly as it treats the hardware.
pub fn preset(name: &str) -> Option<(Sim, u16)> {
    Some(match name {
        "mouse" => (Sim::mouse(), MOUSE_PID),
        "wireless" => (Sim::wireless_mouse(), WIRELESS_MOUSE_PID),
        "keyboard" => (Sim::keyboard(), KEYBOARD_PID),
        "old-keyboard" => (Sim::old_keyboard(), OLD_KEYBOARD_PID),
        "naga" => (Sim::naga(), NAGA_PID),
        // Cobra (wired), issue #1.
        "cobra" => {
            let mut s = SimState::new("SIMCOBRA0001");
            s.dpi = Some((800, 800));
            s.dpi_stages = Some((2, vec![(400, 400), (800, 800), (1600, 1600), (3200, 3200)]));
            s.leds = vec![4];
            s.macros = Some(BTreeMap::new());
            with_buttons(&mut s, &[(1, 1, [1, 0, 0, 0, 0]), (2, 1, [2, 0, 0, 0, 0]), (3, 1, [3, 0, 0, 0, 0]),
                (4, 1, [4, 0, 0, 0, 0]), (5, 1, [5, 0, 0, 0, 0]), (96, 6, [6, 0, 0, 0, 0])]);
            (Sim::new(s), 0x00A3)
        }
        // Mouse Dock Pro with a Basilisk V3 Pro on it, issues #3 and #6.
        "dock" => {
            let mut s = SimState::new("SIMDOCK00001");
            s.dpi = Some((1600, 1600));
            s.dpi_stages = Some((3, vec![(400, 400), (800, 800), (1600, 1600), (3200, 3200), (6400, 6400)]));
            s.polling_hs = Some(0x08);
            s.battery = Some(0xD0);
            s.charging = true;
            s.idle_seconds = Some(300);
            s.leds = vec![1, 4, 10];
            s.paired = vec![(true, 0x00AB)];
            s.dock_light = Some(vec![13, 1]);
            s.macros = Some(BTreeMap::new());
            with_buttons(&mut s, &[(1, 1, [1, 0, 0, 0, 0]), (2, 1, [2, 0, 0, 0, 0]), (3, 1, [3, 0, 0, 0, 0]),
                (4, 1, [4, 0, 0, 0, 0]), (5, 1, [5, 0, 0, 0, 0]), (14, 7, [4, 0, 0, 0, 0]),
                (15, 6, [5, 0x01, 0x90, 0x01, 0x90]), (52, 14, [0x68, 0, 0x14, 0, 0]), (53, 14, [0x69, 0, 0x14, 0, 0]), (96, 6, [6, 0, 0, 0, 0])]);
            (Sim::new(s), 0x00A4)
        }
        // HyperPolling dongle (00B3) with a DeathAdder V3 Pro paired to it.
        "hyperpolling" => {
            let mut s = SimState::new("PM2327H26600383");
            s.dpi = Some((1600, 1600));
            s.polling_hs = Some(0x04);
            s.battery = Some(0xF3);
            s.charging = false;
            s.paired = vec![(true, 0x00B7)];
            (Sim::new(s), 0x00B3)
        }
        // BlackWidow V4 75%, issue #6: per-key grid, high-speed polling, no DPI.
        "blackwidow" => {
            let mut s = SimState::new("SIMBW75KEYS1");
            s.polling_hs = Some(0x08);
            s.leds = vec![5];
            s.matrix = Some((6, 16, vec![[0; 3]; 6 * 16]));
            (Sim::new(s), 0x02A5)
        }
        // Goliathus mousepad (reported on Reddit): one zone, no polling rate.
        "goliathus" => {
            let mut s = SimState::new("SIMGOLIATH01");
            s.txid = 0x3F;
            s.leds = vec![0];
            s.matrix = Some((1, 1, vec![[0; 3]]));
            (Sim::new(s), 0x0C01)
        }
        // Firefly V2 Pro mousepad: a strip of 17 LEDs.
        "firefly" => {
            let mut s = SimState::new("SIMFIREFLY01");
            s.txid = 0x3F;
            s.leds = vec![0];
            s.matrix = Some((1, 17, vec![[0; 3]; 17]));
            (Sim::new(s), 0x0C08)
        }
        // Blade 15 Advanced (Early 2021), issue #7: older lighting commands, keyboard and lid logo.
        "blade" => {
            let mut s = SimState::new("SIMBLADE0001");
            s.standard = true;
            s.leds = vec![5, 4];
            s.matrix = Some((6, 16, vec![[0; 3]; 6 * 16]));
            // Smart Dimming on, going dark when idle, as in issue #20.
            s.smart_dimming = Some((1, 2));
            (Sim::new(s), 0x026D)
        }
        // Base Station V2 Chroma, issue #6.
        "base-station" => {
            let mut s = SimState::new("SIMBASEST001");
            s.leds = vec![0];
            s.matrix = Some((1, 8, vec![[0; 3]; 8]));
            (Sim::new(s), 0x0F20)
        }
        _ => return None,
    })
}

/// Every key in [`crate::buttons::KEYS`] typing itself, in the live and saved slot, on both
/// layers. How a keyboard that takes 02:0D might answer; no real one has been seen yet.
fn with_keys(s: &mut SimState) {
    for slot in [0, 1] {
        for layer in [0, 1] {
            for &(id, _, _, usage) in crate::buttons::KEYS {
                s.keys.insert((slot, id, layer), (2, [0, usage, 0, 0, 0]));
            }
        }
    }
}

/// Factory button assignments in the live slot and the saved slot, on both layers.
fn with_buttons(s: &mut SimState, factory: &[(u8, u8, [u8; 5])]) {
    for slot in [0, 1] {
        for layer in [0, 1] {
            for &(id, function, data) in factory {
                s.buttons.insert((slot, id, layer), (function, data));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::Rgb;
    use crate::profile::{Profile, Zone};

    fn open(sim: &Sim, pid: u16) -> Device {
        sim.open(pid, Sim::spec_for(pid)).expect("simulator answers")
    }

    #[test]
    fn discovery_finds_each_txid() {
        for (sim, pid, txid) in [
            (Sim::mouse(), MOUSE_PID, 0x1F),
            (Sim::wireless_mouse(), WIRELESS_MOUSE_PID, 0x3F),
            (Sim::keyboard(), KEYBOARD_PID, 0xFF),
        ] {
            let dev = open(&sim, pid);
            assert_eq!(dev.txid, txid);
            assert!(dev.serial().unwrap().starts_with("SIM"));
        }
    }

    #[test]
    fn unknown_txid_is_not_a_device() {
        let sim = Sim::mouse();
        sim.state().txid = 0x42;
        assert!(sim.open(MOUSE_PID, None).is_none());
    }

    #[test]
    fn busy_replies_are_retried() {
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        sim.state().busy_reads = 5;
        assert_eq!(dev.firmware().unwrap(), (1, 2));
        sim.state().busy_reads = 100;
        assert!(matches!(dev.firmware(), Err(Error::NoReply(_))));
    }

    #[test]
    fn bad_checksum_is_rejected() {
        let sim = Sim::mouse();
        let mut req = Report::request(0x1F, cmd::FIRMWARE.get(), 2, &[]);
        req.0[88] ^= 0xFF;
        let mut data = vec![0u8];
        data.extend_from_slice(&req.0);
        sim.send_feature_report(&data).unwrap();
        let mut buf = [0u8; REPORT_LEN + 1];
        sim.get_feature_report(&mut buf).unwrap();
        assert_eq!(buf[1], 3, "status Failed");
        assert!(sim.state().log.is_empty());
    }

    #[test]
    fn mouse_settings_round_trip() {
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        assert_eq!(dev.set_dpi(1234, 1234).unwrap(), (1250, 1250));
        assert_eq!(dev.dpi().unwrap(), (1250, 1250));
        dev.set_polling_rate(500).unwrap();
        assert_eq!(dev.polling_rate().unwrap(), 500);
        assert!(dev.set_polling_rate(333).is_err());
        assert_eq!(dev.battery().unwrap(), None);

        dev.set_static_color(4, Rgb(0x12, 0x34, 0x56)).unwrap();
        dev.set_brightness(4, 50).unwrap();
        dev.lights_off(10).unwrap();
        let s = sim.state();
        assert_eq!(s.effects[&4], [cmd::EFFECT_STATIC, 0, 0, 1, 0x12, 0x34, 0x56]);
        assert_eq!(s.brightness[&4], 127);
        assert_eq!(s.effects[&10][0], cmd::EFFECT_OFF);
    }

    #[test]
    fn buttons_remap_reset_and_follow_profiles() {
        use crate::buttons::{Action, Layer};
        use crate::profile::DeviceProfiles;
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        let found: Vec<u8> = dev.buttons().iter().map(|b| b.0).collect();
        assert_eq!(found, [1, 2, 3, 4, 5, 14, 15, 52, 96]);
        assert_eq!(dev.button(15, Layer::Normal).unwrap(), Action::DpiClutch { x: 400, y: 400 });

        let key_a: Action = "key:a".parse().unwrap();
        assert!(dev.set_button(5, Layer::Normal, key_a).unwrap());
        assert!(!dev.set_button(5, Layer::Normal, key_a).unwrap(), "unchanged buttons are not rewritten");
        assert_eq!(dev.button(5, Layer::Normal).unwrap(), key_a);
        assert_eq!(sim.state().buttons[&(1, 5, 0)].0, 1, "saved slot untouched");
        dev.reset_button(5, Layer::Normal).unwrap();
        assert_eq!(dev.button(5, Layer::Normal).unwrap(), Action::Mouse(5));
        assert!(dev.set_button(77, Layer::Normal, key_a).is_err(), "unknown button");

        // Switching to a profile without the remap puts the button back.
        let mut p = DeviceProfiles::default();
        p.create("Gaming").unwrap();
        p.profiles.get_mut("Gaming").unwrap().buttons.insert("side-forward/hypershift".into(), "turbo:left@10".into());
        assert!(p.apply("Gaming", &dev).iter().all(|a| a.result.is_ok()));
        assert_eq!(dev.button(5, Layer::HyperShift).unwrap(), Action::TurboMouse { button: 1, rate: 10 });
        assert!(p.apply("Default", &dev).iter().all(|a| a.result.is_ok()));
        assert_eq!(dev.button(5, Layer::HyperShift).unwrap(), Action::Mouse(5));
    }

    #[test]
    fn buttons_lost_in_sleep_are_noticed_and_sent_again() {
        use crate::buttons::{Action, Layer};
        use crate::profile::DeviceProfiles;
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        let mut p = DeviceProfiles::default();
        assert!(!p.buttons_lost("Default", &dev), "nothing remapped, nothing to lose");
        p.active_mut().buttons.insert("side-forward".into(), "key:ctrl+tab".into());
        assert!(p.apply("Default", &dev).iter().all(|a| a.result.is_ok()));
        assert!(!p.buttons_lost("Default", &dev));

        // Waking from sleep, the mouse fills the live slot from its saved one (#45).
        {
            let mut s = sim.state();
            let saved: Vec<_> = s.buttons.iter().filter(|(k, _)| k.0 == 1).map(|(k, v)| ((0, k.1, k.2), *v)).collect();
            s.buttons.extend(saved);
        }
        assert!(p.buttons_lost("Default", &dev));
        assert!(p.apply_buttons("Default", &dev).iter().all(|a| a.result.is_ok()));
        assert_eq!(dev.button(5, Layer::Normal).unwrap(), "key:ctrl+tab".parse::<Action>().unwrap());
        assert!(!p.buttons_lost("Default", &dev));
    }

    #[test]
    fn dpi_stages_round_trip_and_move_the_mouse() {
        use crate::features::DpiStages;
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        let read = dev.dpi_stages().unwrap();
        assert_eq!(read, DpiStages { active: 2, stages: vec![(400, 400), (800, 800), (1000, 1000), (1300, 1300), (6400, 6400)] });

        let want = DpiStages { active: 3, stages: vec![(450, 450), (1234, 1234), (30000, 26050)] };
        let sent = dev.set_dpi_stages(&want).unwrap();
        assert_eq!(sent.stages, [(450, 450), (1250, 1250), (26000, 26000)], "snapped to the 26000 max");
        assert_eq!(dev.dpi_stages().unwrap(), sent);
        assert_eq!(dev.dpi().unwrap(), (26000, 26000), "mouse moved to the active stage");

        for bad in [
            DpiStages { active: 1, stages: vec![] },
            DpiStages { active: 1, stages: vec![(800, 800); 6] },
            DpiStages { active: 0, stages: vec![(800, 800)] },
            DpiStages { active: 2, stages: vec![(800, 800)] },
        ] {
            assert!(matches!(dev.set_dpi_stages(&bad), Err(Error::InvalidArg(_))), "{bad:?}");
        }
        assert_eq!(dev.dpi_stages().unwrap(), sent, "rejected stages never reach the mouse");
    }

    #[test]
    fn dpi_stages_follow_profiles() {
        use crate::features::DpiStages;
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        let mut store = crate::Store::default();
        let stages = DpiStages { active: 1, stages: vec![(500, 500), (1500, 1500)] };
        // A store with no path can't save; record by hand what actions::set_dpi_stages would.
        let p = store.for_device(&dev).active_mut();
        p.dpi_stages = Some(stages.clone());
        let failures: Vec<_> = p.apply(&dev).into_iter().filter(|a| a.result.is_err()).collect();
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(dev.dpi_stages().unwrap(), stages);
        assert_eq!(dev.dpi().unwrap(), (500, 500));
    }

    #[test]
    fn sleep_timer_limits_and_switch() {
        // Wired mouse: no sleep timer.
        let wired = open(&Sim::mouse(), MOUSE_PID);
        assert_eq!(wired.sleep_timer().unwrap(), None);
        assert!(wired.set_sleep_minutes(5).is_err());

        // Wireless mouse without the switch: 1-15 minutes.
        let sim = Sim::wireless_mouse();
        let dev = open(&sim, WIRELESS_MOUSE_PID);
        let t = dev.sleep_timer().unwrap().unwrap();
        assert_eq!((t.minutes, t.max_minutes, t.auto_sleep), (5, 15, None));
        dev.set_sleep_minutes(15).unwrap();
        assert_eq!(sim.state().idle_seconds, Some(900));
        assert!(matches!(dev.set_sleep_minutes(16), Err(Error::InvalidArg(_))));
        assert!(matches!(dev.set_sleep_minutes(0), Err(Error::InvalidArg(_))));
        assert!(dev.set_auto_sleep(false).is_err(), "no switch on this mouse");

        // Naga with the switch: up to 60 minutes, and sleeping can be turned off.
        let sim = Sim::naga();
        let dev = open(&sim, NAGA_PID);
        let t = dev.sleep_timer().unwrap().unwrap();
        assert_eq!((t.minutes, t.max_minutes, t.auto_sleep), (3, 60, Some(true)));
        dev.set_sleep_minutes(45).unwrap();
        dev.set_auto_sleep(false).unwrap();
        assert_eq!(dev.sleep_timer().unwrap().unwrap().auto_sleep, Some(false));
        assert_eq!(sim.state().idle_seconds, Some(2700));
    }

    #[test]
    fn effects_are_sent_and_follow_profiles() {
        use crate::effects::Effect;
        use crate::profile::DeviceProfiles;
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        dev.set_effect(4, &"wave:left@5".parse().unwrap()).unwrap();
        assert_eq!(sim.state().effects[&4], [4, 2, 15, 0]);
        dev.set_effect(1, &"breathing:FF0000,0000FF".parse().unwrap()).unwrap();
        assert_eq!(sim.state().effects[&1], [2, 2, 0, 2, 0xFF, 0, 0, 0, 0, 0xFF]);
        assert!(dev.set_effect(7, &Effect::Spectrum).is_err(), "unknown zone");

        let mut p = DeviceProfiles::default();
        p.create("Party").unwrap();
        let zone = p.profiles.get_mut("Party").unwrap().lighting.entry("logo".into()).or_default();
        zone.color = Some(Rgb(0, 0xFF, 0));
        zone.effect = Some(Effect::Spectrum);
        assert!(p.apply("Party", &dev).iter().all(|a| a.result.is_ok()));
        assert_eq!(sim.state().effects[&4], [3, 0, 0, 0], "the effect wins over the static color");

        let old = open(&Sim::old_keyboard(), OLD_KEYBOARD_PID);
        // Its grid runs effects (older_keyboard_uses_the_03_command_set); a single LED doesn't.
        assert!(matches!(old.set_effect(4, &Effect::Spectrum), Err(Error::Unsupported(_))));
    }

    #[test]
    fn macros_are_stored_listed_read_and_deleted() {
        use crate::buttons::{Action, Layer, MacroPlay};
        use crate::macros::Script;
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        assert!(dev.macros().unwrap().is_empty());
        let free = dev.macro_storage().unwrap().free_bytes;

        // Long enough to need several 72-byte data reports.
        let text = (0..30).map(|i| format!("tap:{} wait:{}", (b'a' + i % 26) as char, 20 + i)).collect::<Vec<_>>().join(" ");
        let script: Script = text.parse().unwrap();
        let first = dev.create_macro("Long one", &script.0).unwrap();
        let second = dev.create_macro("Copy ✓", &"tap:ctrl+c".parse::<Script>().unwrap().0).unwrap();
        assert_eq!((first, second), (1, 2), "lowest free ids");

        let list = dev.macros().unwrap();
        assert_eq!(list.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Long one", "Copy ✓"]);
        assert_eq!(dev.macro_steps(first).unwrap(), script.0);
        assert_eq!(Script(dev.macro_steps(second).unwrap()).to_string(), "tap:ctrl+c");
        assert!(dev.macro_storage().unwrap().free_bytes < free);

        // A button plays it.
        let play = Action::Macro { id: second, play: MacroPlay::WhileHeld };
        assert!(dev.set_button(5, Layer::Normal, play).unwrap());
        assert_eq!(dev.button(5, Layer::Normal).unwrap(), play);

        dev.delete_macro(first).unwrap();
        assert_eq!(dev.macro_ids().unwrap(), [second]);
        assert!(dev.macro_storage().unwrap().recyclable_bytes > 0);
        assert_eq!(dev.create_macro("Reuses id 1", &script.0).unwrap(), 1);
        assert!(dev.create_macro("  ", &script.0).is_err(), "needs a name");
    }

    #[test]
    fn macro_too_big_for_memory_is_refused_before_writing() {
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        let huge = vec![crate::macros::Step::Delay(10); 50_000];
        assert!(matches!(dev.create_macro("Huge", &huge), Err(Error::InvalidArg(_))));
        assert!(sim.state().macros.as_ref().unwrap().is_empty());
        let kb = open(&Sim::keyboard(), KEYBOARD_PID);
        assert!(kb.macro_storage().is_err(), "devices without macro memory say so");
    }

    #[test]
    fn last_left_click_is_kept_and_everything_can_be_restored() {
        use crate::buttons::{Action, Layer};
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        let dir = std::env::temp_dir().join(format!("secbird-restore-{}", std::process::id()));
        let mut store = crate::Store::load(&dir.join("profiles.json")).unwrap();
        let middle: Action = "middle".parse().unwrap();
        let err = crate::actions::set_button(&dev, &mut store, "left", Layer::Normal, Some(middle)).unwrap_err();
        assert!(err.to_string().contains("only left click"), "{err}");
        assert_eq!(dev.button(1, Layer::Normal).unwrap(), Action::Mouse(1));

        // With another left click it's allowed; HyperShift is never blocked.
        let left: Action = "left".parse().unwrap();
        crate::actions::set_button(&dev, &mut store, "side-back", Layer::Normal, Some(left)).unwrap();
        crate::actions::set_button(&dev, &mut store, "left", Layer::Normal, Some(middle)).unwrap();
        crate::actions::set_button(&dev, &mut store, "left", Layer::HyperShift, Some(middle)).unwrap();
        assert_eq!(store.for_device(&dev).active().buttons.len(), 3);

        assert_eq!(crate::actions::restore_all_buttons(&dev, &mut store).unwrap(), 3);
        assert_eq!(dev.button(1, Layer::Normal).unwrap(), Action::Mouse(1));
        assert_eq!(dev.button(4, Layer::Normal).unwrap(), Action::Mouse(4));
        assert!(store.for_device(&dev).active().buttons.is_empty(), "remaps forgotten, not re-applied later");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn every_preset_opens_with_its_real_spec() {
        let db = crate::db::Db::builtin().unwrap();
        for name in PRESETS {
            let (sim, pid) = preset(name).unwrap();
            let spec = Sim::spec_for(pid).or_else(|| db.get(pid).cloned());
            assert!(spec.is_some(), "{name}: no spec");
            let mut dev = sim.open(pid, spec).unwrap_or_else(|| panic!("{name} doesn't answer"));
            dev.identify_docked_mouse(&db);
            for led in dev.spec.clone().unwrap().leds {
                dev.set_static_color(led.id, Rgb(1, 2, 3)).unwrap_or_else(|e| panic!("{name} {}: {e}", led.name));
            }
            if let Some(m) = dev.matrix() {
                let frame = vec![Rgb(9, 9, 9); m.rows as usize * m.cols as usize];
                dev.show_frame(&frame).unwrap_or_else(|e| panic!("{name} keys: {e}"));
            }
        }
        let (sim, pid) = preset("dock").unwrap();
        let mut dock = sim.open(pid, db.get(pid).cloned()).unwrap();
        dock.identify_docked_mouse(&db);
        assert_eq!(dock.name(), "Basilisk V3 Pro on Mouse Dock Pro");
        assert_eq!(dock.polling_rate().unwrap(), 1000);
    }

    #[test]
    fn naga_side_grid_is_listed_and_remappable() {
        use crate::buttons::{button_id, Action, Layer};
        let sim = Sim::mouse();
        // Factory grid keys 1-9, 0, -, = (Synapse's SPAD_NUM_1-12 defaults).
        let usages = [0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E];
        for (i, usage) in usages.into_iter().enumerate() {
            for slot in [0, 1] {
                for layer in [0, 1] {
                    sim.state().buttons.insert((slot, 64 + i as u8, layer), (2, [0, usage, 0, 0, 0]));
                }
            }
        }
        // A Basilisk's buttons with a grid added, opened as a Naga (which gets side plates).
        let dev = open(&sim, NAGA_PID);
        let ids: Vec<u8> = dev.buttons().iter().map(|b| b.0).collect();
        assert_eq!(ids, [1, 2, 3, 4, 5, 14, 15, 52, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 96]);
        let labels: Vec<String> = dev.buttons().iter().filter(|b| b.0 >= 64 && b.0 <= 75).map(|b| b.1.to_string()).collect();
        assert_eq!(labels, ["key:1", "key:2", "key:3", "key:4", "key:5", "key:6", "key:7", "key:8", "key:9", "key:0", "key:minus", "key:equal"]);

        let grid12 = button_id("grid-12").unwrap();
        assert_eq!(grid12, 75);
        let f5: Action = "key:f5".parse().unwrap();
        assert!(dev.set_button(grid12, Layer::HyperShift, f5).unwrap());
        assert_eq!(dev.button(grid12, Layer::HyperShift).unwrap(), f5);
        dev.reset_button(grid12, Layer::HyperShift).unwrap();
        assert_eq!(dev.button(grid12, Layer::HyperShift).unwrap().to_string(), "key:equal");
    }

    /// A Mouse Dock Pro answering for a docked Basilisk V3 Pro, shaped like issue #6's report.
    fn docked_basilisk() -> (Sim, Device, crate::db::Db) {
        let mut s = SimState::new("PM2400000076");
        s.dpi = Some((950, 950));
        s.battery = Some(0xF3);
        s.polling_hs = Some(0x08);
        s.leds = vec![1, 4, 10];
        s.paired = vec![(true, 0x00AB)];
        let sim = Sim::new(s);
        let db = crate::db::Db::builtin().unwrap();
        let dev = sim.open(0x00A4, db.get(0x00A4).cloned()).unwrap();
        (sim, dev, db)
    }

    #[test]
    fn mouse_dock_pro_controls_the_docked_mouse() {
        let (_sim, mut dev, db) = docked_basilisk();
        dev.identify_docked_mouse(&db);
        assert_eq!(dev.name(), "Basilisk V3 Pro on Mouse Dock Pro");
        assert_eq!(dev.key(), "00A4:PM2400000076", "profiles stay with the dock");
        assert_eq!(dev.battery().unwrap(), Some(95));
        assert_eq!(dev.polling_rate().unwrap(), 1000);
        dev.set_polling_rate(500).unwrap();
        assert_eq!(dev.polling_rate().unwrap(), 500);
        assert_eq!(dev.set_dpi(31000, 31000).unwrap(), (30000, 30000));
        let leds: Vec<u8> = dev.spec.as_ref().unwrap().leds.iter().map(|l| l.id).collect();
        assert_eq!(leds, [4, 1, 10]);
    }

    #[test]
    fn the_dock_light_is_its_own_zone_on_the_dock_txid() {
        let (sim, mut dev, db) = docked_basilisk();
        sim.state().dock_light = Some(vec![13, 1]);
        dev.identify_receiver(&db);
        let led = dev.led_id("dock").unwrap();
        assert_eq!(led, crate::device::DOCK_LED);

        dev.set_static_color(led, Rgb(255, 0, 0)).unwrap();
        assert_eq!(sim.state().dock_light.as_deref(), Some(&[cmd::EFFECT_STATIC, 0, 0, 1, 255, 0, 0][..]));
        assert!(!sim.state().effects.contains_key(&led), "nothing sent to the mouse");
        dev.set_effect(led, &crate::effects::Effect::Spectrum).unwrap();
        assert_eq!(sim.state().dock_light.as_ref().unwrap()[0], 3);

        // Battery: the mouse is at 95%, so the dock goes green.
        dev.set_effect(led, &crate::effects::Effect::Battery).unwrap();
        assert_eq!(sim.state().dock_light.as_ref().unwrap()[4..7], [0, 255, 0]);
        sim.state().battery = Some(0x30); // 19%
        dev.set_effect(led, &crate::effects::Effect::Battery).unwrap();
        assert_eq!(sim.state().dock_light.as_ref().unwrap()[4..7], [255, 0x28, 0]);

        // The mouse's own lights still go to the mouse.
        dev.set_static_color(dev.led_id("logo").unwrap(), Rgb(1, 2, 3)).unwrap();
        assert_eq!(sim.state().effects[&4][4..7], [1, 2, 3]);
    }

    #[test]
    fn report_asks_the_dock_itself_with_its_own_txid() {
        let (_sim, mut dev, db) = docked_basilisk();
        dev.identify_docked_mouse(&db);
        let r = crate::report::probe(&dev, true);
        assert_eq!(r.leds, [1, 4, 10], "the docked mouse, on the usual txid");
        // The simulator only answers txid 1F, like a dock that ignores 0xFF would.
        assert!(!r.dock.is_empty() && r.dock.iter().all(|p| p.status == "no reply"));
        assert!(r.dock_leds.is_empty());
    }

    #[test]
    fn laptop_keyboard_dimming_reads_sets_and_reads_back() {
        use crate::Dimming;
        let (sim, pid) = preset("blade").unwrap();
        let db = crate::db::Db::builtin().unwrap();
        let dev = sim.open(pid, db.get(pid).cloned()).unwrap();
        assert_eq!(dev.dimming().unwrap(), Some(Dimming::Dark));
        dev.set_dimming(Dimming::StaysLit).unwrap();
        assert_eq!(dev.dimming().unwrap(), Some(Dimming::StaysLit));
        assert_eq!(sim.state().smart_dimming, Some((0, 2)), "turning it off keeps the level");
        dev.set_dimming(Dimming::Half).unwrap();
        assert_eq!(sim.state().smart_dimming, Some((1, 0)));
        for d in Dimming::ALL {
            assert_eq!(d.to_string().parse::<Dimming>().unwrap(), d);
        }
        // Only laptops have it.
        let mouse = open(&Sim::mouse(), MOUSE_PID);
        assert_eq!(mouse.dimming().unwrap(), None);
    }

    #[test]
    fn hyperpolling_dongle_names_its_mouse_and_writes_both_polling_selectors() {
        let (sim, pid) = preset("hyperpolling").unwrap();
        let db = crate::db::Db::builtin().unwrap();
        let mut dev = sim.open(pid, db.get(pid).cloned()).unwrap();
        dev.identify_receiver(&db);
        assert_eq!(dev.name(), "DeathAdder V3 Pro (via HyperPolling)");
        assert_eq!(dev.key(), "00B3:PM2327H26600383", "profiles stay with the dongle");
        assert_eq!(dev.battery().unwrap(), Some(95));
        assert_eq!(dev.polling_rate().unwrap(), 2000);

        // Both the live (0x00) and the stored (0x01) selector, or the device comes back
        // up at its old rate.
        sim.state().log.clear();
        dev.set_polling_rate(8000).unwrap();
        assert_eq!(sim.state().log.iter().filter(|c| **c == cmd::POLLING_HS).count(), 2);
        assert_eq!(dev.polling_rate().unwrap(), 8000);
    }

    #[test]
    fn hyperpolling_dongle_keeps_its_entry_when_no_mouse_is_paired() {
        let (sim, pid) = preset("hyperpolling").unwrap();
        let db = crate::db::Db::builtin().unwrap();
        let mut dev = sim.open(pid, db.get(pid).cloned()).unwrap();
        for paired in [vec![], vec![(false, 0x00B7)], vec![(true, 0x00FF)], vec![(true, 0x02A7)]] {
            sim.state().paired = paired.clone();
            dev.identify_receiver(&db);
            assert_eq!(dev.key(), "00B3:PM2327H26600383", "the key never changes");
            assert_eq!(dev.name(), "HyperPolling Wireless Dongle", "{paired:?}");
        }
    }

    #[test]
    fn mouse_dock_pro_keeps_generic_entry_when_mouse_is_unknown() {
        for paired in [vec![], vec![(false, 0x00AB)], vec![(true, 0x00FF)], vec![(true, 0x0099)]] {
            let (sim, mut dev, db) = docked_basilisk();
            sim.state().paired = paired.clone();
            dev.identify_docked_mouse(&db);
            assert_eq!(dev.name(), "Mouse on Mouse Dock Pro", "{paired:?}");
            assert_eq!(dev.battery().unwrap(), Some(95));
            assert_eq!(dev.polling_rate().unwrap(), 1000);
        }
    }

    #[test]
    fn docked_basilisk_v4_pro_is_named_and_shows_only_its_buttons() {
        use crate::buttons::BUTTONS;
        let (sim, mut dev, db) = docked_basilisk();
        {
            // Like #16: the dock answers for every button id, Naga plates included.
            let mut s = sim.state();
            s.paired = vec![(true, 0x00E3)];
            for &(id, _, _) in BUTTONS {
                s.buttons.insert((0, id, 0), (0, [0; 5]));
                s.buttons.insert((1, id, 0), (0, [0; 5]));
            }
        }
        dev.identify_docked_mouse(&db);
        assert_eq!(dev.name(), "Basilisk V4 Pro on Mouse Dock Pro");
        let ids: Vec<u8> = dev.buttons().iter().map(|b| b.0).collect();
        assert!(ids.contains(&1) && ids.contains(&106));
        assert!(!ids.iter().any(|id| (64..=86).contains(id)), "no Naga side plates: {ids:?}");

        // When the mouse lists its buttons, that list wins, unnamed ids included.
        sim.state().button_list = Some(vec![1, 2, 0x34, 15, 120]);
        sim.state().buttons.insert((0, 120, 0), (33, [0x79, 0xC4, 0x73, 0xCE, 0x11]));
        let ids: Vec<u8> = dev.buttons().iter().map(|b| b.0).collect();
        assert_eq!(ids, [1, 2, 15, 0x34, 120]);
        assert_eq!(crate::buttons::button_name(120), "button-120");
        assert_eq!(crate::buttons::button_id("button-120"), Some(120));

        // A listed button that doesn't answer a read is left out, and reported.
        sim.state().button_list = Some(vec![1, 2, 106]);
        sim.state().buttons.remove(&(0, 106, 0));
        let (found, unread) = dev.button_list();
        assert_eq!(found.iter().map(|b| b.0).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(unread, [106]);
    }

    #[test]
    fn wireless_reports_battery() {
        let dev = open(&Sim::wireless_mouse(), WIRELESS_MOUSE_PID);
        assert_eq!(dev.battery().unwrap(), Some(75));
        assert!(dev.charging().unwrap());
    }

    #[test]
    fn unknown_led_and_unsupported_commands_fail() {
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        assert!(matches!(dev.set_brightness(7, 50), Err(Error::Device { status: Status::Failed, .. })));
        let kb = open(&Sim::keyboard(), KEYBOARD_PID);
        assert!(matches!(kb.dpi(), Err(Error::Device { status: Status::NotSupported, .. })));
    }

    #[test]
    fn keyboard_keys_remap_reset_and_follow_profiles() {
        use crate::buttons::{parse_setting_key, Action, Input, Layer};
        let sim = Sim::keyboard();
        let dev = open(&sim, KEYBOARD_PID);
        assert_eq!(dev.keys().len(), crate::buttons::KEYS.len());
        assert!(dev.buttons().is_empty(), "keys aren't mouse buttons");
        let caps = Input::Key(30);
        assert_eq!(parse_setting_key("key-capslock").unwrap(), (caps, Layer::Normal));
        assert_eq!(parse_setting_key("key-left/hypershift").unwrap(), (Input::Key(79), Layer::HyperShift));
        assert_eq!(parse_setting_key("left").unwrap(), (Input::Button(1), Layer::Normal), "the mouse button");
        assert_eq!(Input::Key(200).name(), "key-id200");
        assert_eq!(Input::parse("key-id200"), Some(Input::Key(200)));

        let esc = Action::Key { modifiers: 0, usage: 0x29 };
        let dir = std::env::temp_dir().join(format!("secbird-keys-{}", std::process::id()));
        let mut store = crate::Store::load(&dir.join("profiles.json")).unwrap();
        crate::actions::set_button(&dev, &mut store, "key-capslock", Layer::Normal, Some(esc)).unwrap();
        assert_eq!(sim.state().keys[&(0, 30, 0)].1[1], 0x29, "live slot");
        assert_eq!(sim.state().keys[&(1, 30, 0)].1[1], 0x39, "saved slot untouched");
        assert!(sim.state().log.contains(&cmd::KEY), "sent as a key, not a button");
        assert_eq!(store.for_device(&dev).active().buttons["key-capslock"], "key:esc");

        // A profile switch puts the key back; switching again sends it again.
        let p = store.for_device(&dev);
        p.profiles.insert("Plain".into(), Profile::default());
        p.apply("Plain", &dev);
        assert_eq!(dev.input(caps, Layer::Normal).unwrap(), Action::Key { modifiers: 0, usage: 0x39 });
        p.apply("Default", &dev);
        assert_eq!(dev.input(caps, Layer::Normal).unwrap(), esc);

        assert_eq!(crate::actions::restore_all_buttons(&dev, &mut store).unwrap(), 1);
        assert_eq!(dev.input(caps, Layer::Normal).unwrap(), Action::Key { modifiers: 0, usage: 0x39 });
    }

    #[test]
    fn keyboards_without_key_commands_list_no_keys() {
        let sim = Sim::old_keyboard();
        let dev = open(&sim, OLD_KEYBOARD_PID);
        assert!(dev.keys().is_empty());
        let mouse = Sim::mouse();
        assert!(open(&mouse, MOUSE_PID).keys().is_empty(), "mice have no keys");
    }

    #[test]
    fn keyboard_shows_a_full_frame() {
        let sim = Sim::keyboard();
        let dev = open(&sim, KEYBOARD_PID);
        let m = dev.matrix().unwrap();
        let frame: Vec<Rgb> = (0..m.rows as usize * m.cols as usize)
            .map(|i| Rgb((i / m.cols as usize) as u8 * 40, (i % m.cols as usize) as u8 * 10, 7))
            .collect();
        dev.show_frame(&frame).unwrap();

        let s = sim.state();
        assert_eq!(s.key(0, 0), Some([0, 0, 7]));
        assert_eq!(s.key(5, 21), Some([200, 210, 7]));
        assert_eq!(s.effects[&0][0], cmd::EFFECT_CUSTOM_FRAME);
        // 6 rows, then the effect, live only (nothing saved on the device).
        assert_eq!(s.log.iter().filter(|c| **c == cmd::MATRIX_FRAME).count(), 6);
        assert_eq!(s.log.iter().filter(|c| **c == cmd::MATRIX_EFFECT).count(), 1);
    }

    #[test]
    fn save_to_mouse_writes_the_slot_keeps_a_backup_and_puts_it_back() {
        use crate::buttons::{Action, Layer};
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        let path = std::env::temp_dir().join(format!("secbird-onboard-{}.json", std::process::id()));
        let mut store = crate::Store::load(&path).unwrap();
        let factory = dev.slot_button(1, 5, Layer::Normal).unwrap();
        let f13 = Action::Key { modifiers: 0, usage: 0x68 };
        crate::actions::set_button(&dev, &mut store, "side-forward", Layer::Normal, Some(f13)).unwrap();
        assert_eq!(dev.slot_button(1, 5, Layer::Normal).unwrap(), factory, "a remap is live only");

        assert_eq!(crate::actions::save_buttons_to_mouse(&dev, &mut store).unwrap(), 1);
        assert_eq!(dev.slot_button(1, 5, Layer::Normal).unwrap(), f13, "saved in slot 1");
        let save = store.for_device(&dev).onboard.clone().unwrap();
        assert_eq!(save.before["side-forward"], factory.to_string(), "the backup holds the factory setting");

        // Waking from sleep refills the live slot from the saved one: nothing is lost now.
        {
            let mut s = sim.state();
            let saved: Vec<_> = s.buttons.iter().filter(|(k, _)| k.0 == 1).map(|(k, v)| ((0, k.1, k.2), *v)).collect();
            s.buttons.extend(saved);
        }
        assert!(!store.for_device(&dev).buttons_lost("Default", &dev));

        // A profile without the remap goes back to the factory setting, not the saved remap.
        store.for_device(&dev).create("Game").unwrap();
        store.for_device(&dev).profiles.get_mut("Game").unwrap().buttons.clear();
        store.for_device(&dev).apply("Game", &dev);
        assert_eq!(dev.button(5, Layer::Normal).unwrap(), factory);

        // Saving a profile that doesn't remap it puts the slot back for that button.
        store.for_device(&dev).active = "Game".into();
        crate::actions::save_buttons_to_mouse(&dev, &mut store).unwrap();
        assert_eq!(dev.slot_button(1, 5, Layer::Normal).unwrap(), factory);

        // Put back, and Restore all, leave the slot as the factory had it.
        store.for_device(&dev).active = "Default".into();
        crate::actions::save_buttons_to_mouse(&dev, &mut store).unwrap();
        assert_eq!(crate::actions::restore_all_buttons(&dev, &mut store).unwrap(), 1);
        assert_eq!(dev.slot_button(1, 5, Layer::Normal).unwrap(), factory);
        assert_eq!(dev.button(5, Layer::Normal).unwrap(), factory);
        assert!(store.for_device(&dev).onboard.is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn gaming_mode_blocks_keys_and_is_kept_in_the_profile() {
        use crate::GameMode;
        let sim = Sim::keyboard();
        let dev = open(&sim, KEYBOARD_PID);
        assert_eq!(dev.game_mode().unwrap(), Some(GameMode::default()));
        let path = std::env::temp_dir().join(format!("secbird-gaming-{}.json", std::process::id()));
        let mut store = crate::Store::load(&path).unwrap();
        let want: GameMode = "win,alt-tab".parse().unwrap();
        crate::actions::set_game_mode(&dev, &mut store, want).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(sim.state().game_mode, Some(1 | 4 | 8), "Synapse's bits");
        assert_eq!(dev.game_mode().unwrap(), Some(want));
        assert_eq!(store.for_device(&dev).active_mut().game_mode.as_deref(), Some("win,alt-tab"));

        dev.set_game_mode(GameMode::default()).unwrap();
        assert_eq!(sim.state().game_mode, Some(0), "off is all bits clear");
        assert_eq!(GameMode::default().to_string(), "off");
        assert_eq!("off".parse::<GameMode>().unwrap(), GameMode::default());
        assert!("ctrl".parse::<GameMode>().is_err());

        let mouse = open(&Sim::mouse(), MOUSE_PID);
        assert_eq!(mouse.game_mode().unwrap(), None, "not a keyboard");
    }

    #[test]
    fn live_brightness_isnt_saved_and_reads_answer_the_saved_level() {
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        dev.set_brightness(1, 80).unwrap();
        dev.set_brightness_live(1, 0).unwrap();
        assert_eq!(sim.state().live_brightness[&1], 0, "dark now");
        assert_eq!(dev.brightness(1).unwrap(), 80, "the saved level, as a real Basilisk V3 answers");
        dev.set_brightness_live(1, dev.brightness(1).unwrap()).unwrap();
        assert_eq!(sim.state().live_brightness[&1], 204);
    }

    #[test]
    fn per_key_colors_come_back_after_the_keyboard_drops_them() {
        use crate::profile::DeviceProfiles;
        let sim = Sim::keyboard();
        let dev = open(&sim, KEYBOARD_PID);
        let mut p = DeviceProfiles::default();
        assert!(p.refresh_frame("Default", &dev).is_none(), "no per-key colors, nothing to send");
        p.active_mut().keys = vec![Rgb(9, 8, 7); 6 * 22];
        p.apply("Default", &dev);

        // Quick adjustment mode (#45) leaves the keys dark.
        sim.state().matrix.as_mut().unwrap().2.fill([0; 3]);
        assert_eq!(sim.state().key(3, 10), Some([0, 0, 0]));
        assert!(p.refresh_frame("Default", &dev).unwrap().is_ok());
        assert_eq!(sim.state().key(3, 10), Some([9, 8, 7]));

        p.lighting_off = true;
        assert!(p.refresh_frame("Default", &dev).is_none(), "lighting left to another app");
    }

    #[test]
    fn finding_a_light_shows_only_that_one_and_saves_nothing() {
        let sim = Sim::keyboard();
        let dev = open(&sim, KEYBOARD_PID);
        let mut store = crate::Store::default();
        crate::actions::show_light(&dev, &mut store, 0, 21).unwrap();
        let s = sim.state();
        assert_eq!(s.key(0, 21), Some([255, 255, 255]));
        assert_eq!(s.key(0, 20), Some([0, 0, 0]));
        assert!(store.for_device(&dev).active_mut().keys.is_empty());
        assert!(crate::actions::show_light(&dev, &mut store, 6, 0).is_err());
    }

    #[test]
    fn frame_size_is_checked() {
        let dev = open(&Sim::keyboard(), KEYBOARD_PID);
        assert!(matches!(dev.show_frame(&[Rgb(1, 2, 3); 10]), Err(Error::InvalidArg(_))));
        assert!(matches!(dev.set_row(6, 0, &[Rgb(1, 2, 3)]), Err(Error::InvalidArg(_))));
        assert!(matches!(dev.set_row(0, 20, &[Rgb(1, 2, 3); 3]), Err(Error::InvalidArg(_))));
        let mouse = open(&Sim::mouse(), MOUSE_PID);
        assert!(matches!(mouse.show_frame(&[]), Err(Error::Unsupported(_))));
    }

    #[test]
    fn wide_rows_are_split_into_packets() {
        let mut s = SimState::new("SIMWIDE00001");
        s.leds = vec![5];
        s.matrix = Some((1, 30, vec![[0; 3]; 30]));
        let sim = Sim::new(s);
        let mut spec = Sim::spec_for(KEYBOARD_PID).unwrap();
        spec.matrix = Some(crate::db::Matrix { rows: 1, cols: 30, first: 1 });
        let dev = sim.open(KEYBOARD_PID, Some(spec)).unwrap();
        let row: Vec<Rgb> = (0..30).map(|i| Rgb(i, i, i)).collect();
        dev.set_row(0, 0, &row).unwrap();
        let s = sim.state();
        assert_eq!(s.log.iter().filter(|c| **c == cmd::MATRIX_FRAME).count(), 2);
        assert_eq!(s.key(0, 29), Some([29, 29, 29]));
        assert_eq!(s.key(0, 24), Some([24, 24, 24]));
    }

    #[test]
    fn profile_applies_to_simulated_mouse() {
        let sim = Sim::mouse();
        let dev = open(&sim, MOUSE_PID);
        let mut p = Profile { dpi: Some((3200, 3200)), polling: Some(125), ..Default::default() };
        p.lighting.insert("logo".into(), Zone { color: Some(Rgb(255, 0, 0)), off: false, brightness: Some(100), effect: None });
        p.lighting.insert("underglow".into(), Zone { color: None, off: true, brightness: None, effect: None });
        let results = p.apply(&dev);
        assert!(results.iter().all(|r| r.result.is_ok()), "{:?}", results.iter().map(|r| &r.result).collect::<Vec<_>>());
        let s = sim.state();
        assert_eq!(s.dpi, Some((3200, 3200)));
        assert_eq!(s.polling_code, 0x08);
        assert_eq!(s.effects[&4][4..7], [255, 0, 0]);
        assert_eq!(s.effects[&10][0], cmd::EFFECT_OFF);
    }

    #[test]
    fn lighting_goes_to_the_active_slot() {
        let sim = Sim::keyboard();
        sim.state().active_slot = Some(2);
        let dev = open(&sim, KEYBOARD_PID);
        let led = dev.led_id("backlight").unwrap();
        dev.set_static_color(led, Rgb(0, 255, 0)).unwrap();
        dev.set_brightness(led, 100).unwrap();
        let s = sim.state();
        assert_eq!(s.effects[&5][4..7], [0, 255, 0]);
        assert_eq!(s.brightness[&5], 255);
    }

    #[test]
    fn auto_sleep_off_is_saved_too() {
        let sim = Sim::naga();
        sim.state().auto_sleep_saved = Some(true);
        let dev = open(&sim, NAGA_PID);
        dev.set_auto_sleep(false).unwrap();
        let s = sim.state();
        assert_eq!((s.auto_sleep, s.auto_sleep_saved), (Some(false), Some(false)));
    }

    #[test]
    fn keyboard_backlight_zone_works() {
        let sim = Sim::keyboard();
        let dev = open(&sim, KEYBOARD_PID);
        let led = dev.led_id("backlight").unwrap();
        dev.set_static_color(led, Rgb(0, 255, 0)).unwrap();
        dev.set_brightness(led, 100).unwrap();
        assert_eq!(sim.state().brightness[&5], 255);
    }

    #[test]
    fn painted_keys_are_saved_and_replace_zone_color() {
        use crate::{actions, Store};
        let path = std::env::temp_dir().join(format!("secbird-test-{}.json", std::process::id()));
        let mut store = Store::load(&path).unwrap();
        let sim = Sim::keyboard();
        let dev = open(&sim, KEYBOARD_PID);

        actions::set_color(&dev, &mut store, "backlight", Rgb(0, 0, 255)).unwrap();
        actions::set_brightness(&dev, &mut store, "backlight", 60).unwrap();
        let mut keys = vec![Rgb(0, 0, 0); 6 * 22];
        keys[23] = Rgb(255, 0, 0);
        actions::set_keys(&dev, &mut store, keys.clone()).unwrap();

        let saved = Store::load(&path).unwrap().for_device(&dev).active().clone();
        assert_eq!(saved.keys, keys);
        assert_eq!(saved.lighting["backlight"].color, None);
        assert_eq!(saved.lighting["backlight"].brightness, Some(60));
        assert_eq!(sim.state().key(1, 1), Some([255, 0, 0]));

        // Re-applying on a fresh device repaints the keys.
        let fresh = Sim::keyboard();
        let results = saved.apply(&open(&fresh, KEYBOARD_PID));
        assert!(results.iter().all(|r| r.result.is_ok()));
        assert_eq!(fresh.state().key(1, 1), Some([255, 0, 0]));

        // A whole-zone color clears the painted keys.
        actions::set_color(&dev, &mut store, "backlight", Rgb(9, 9, 9)).unwrap();
        assert!(Store::load(&path).unwrap().for_device(&dev).active().keys.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn older_keyboard_uses_the_03_command_set() {
        let sim = Sim::old_keyboard();
        let dev = open(&sim, OLD_KEYBOARD_PID);
        dev.set_static_color(5, Rgb(1, 2, 3)).unwrap();
        assert_eq!(sim.state().grid_effect, [cmd::GRID_STATIC, 1, 2, 3]);
        dev.lights_off(5).unwrap();
        assert_eq!(sim.state().grid_effect, [cmd::GRID_OFF]);

        // Effects on the whole grid, as openrazer sends them to the BlackWidow Chroma V2 (#49).
        let effect = |text: &str| {
            dev.set_effect(5, &text.parse().unwrap()).unwrap();
            sim.state().grid_effect.clone()
        };
        assert_eq!(effect("spectrum"), [0x04]);
        assert_eq!(effect("wave:left"), [0x01, 0x02]);
        assert_eq!(effect("reactive:0A0B0C@3"), [0x02, 3, 0x0A, 0x0B, 0x0C]);
        assert_eq!(effect("breathing:random"), [0x03, 0x03, 0, 0, 0, 0, 0, 0]);
        assert_eq!(effect("breathing:010203,040506"), [0x03, 0x02, 1, 2, 3, 4, 5, 6]);
        assert_eq!(effect("starlight:random@2"), [0x19]);

        dev.set_static_color(4, Rgb(9, 8, 7)).unwrap();
        dev.set_brightness(4, 100).unwrap();
        {
            let s = sim.state();
            assert_eq!(s.led_rgb[&4], [9, 8, 7]);
            assert_eq!(s.effects[&4], [cmd::CLASSIC_STATIC]);
            assert!(s.led_on[&4]);
            assert_eq!(s.brightness[&4], 255);
        }
        dev.lights_off(4).unwrap();
        assert!(!sim.state().led_on[&4]);

        let mut frame = vec![Rgb(0, 0, 0); 6 * 22];
        frame[22 * 5 + 21] = Rgb(255, 255, 255);
        dev.show_frame(&frame).unwrap();
        let s = sim.state();
        assert_eq!(s.key(5, 21), Some([255, 255, 255]));
        assert_eq!(s.grid_effect, [cmd::GRID_CUSTOM_FRAME, cmd::VARSTORE]);
        assert!(s.log.iter().all(|c| c.class != 0x0F), "no 0F:xx commands sent");
    }

    #[test]
    fn kraken_lighting_is_refused_cleanly() {
        let mut spec = Sim::spec_for(MOUSE_PID).unwrap();
        spec.lighting = crate::db::Lighting::Kraken;
        let dev = Sim::mouse().open(MOUSE_PID, Some(spec)).unwrap();
        assert!(matches!(dev.set_static_color(4, Rgb(1, 1, 1)), Err(Error::Unsupported(_))));
    }

    #[test]
    fn disabled_lighting_sends_no_rgb_commands_and_keeps_other_settings() {
        use crate::{actions, Store};
        let path = std::env::temp_dir().join(format!("secbird-lighting-control-{}.json", std::process::id()));
        let mut store = Store::load(&path).unwrap();
        for (sim, pid) in [(Sim::mouse(), MOUSE_PID), (Sim::keyboard(), KEYBOARD_PID), (Sim::old_keyboard(), OLD_KEYBOARD_PID)] {
            let dev = open(&sim, pid);
            let profiles = store.for_device(&dev);
            let profile = profiles.active_mut();
            profile.polling = Some(500);
            if pid == MOUSE_PID {
                profile.dpi = Some((1600, 1600));
                profile.buttons.insert("side-forward".into(), "key:a".into());
            }
            for led in &dev.spec.as_ref().unwrap().leds {
                profile.lighting.insert(led.name.clone(), Zone {
                    color: Some(Rgb(1, 2, 3)), brightness: Some(25), ..Zone::default()
                });
            }
            if let Some(m) = dev.matrix() {
                profile.keys = vec![Rgb(4, 5, 6); m.rows as usize * m.cols as usize];
            }
            profiles.create("Gaming").unwrap();
            profiles.profiles.get_mut("Gaming").unwrap().lighting.values_mut().for_each(|z| {
                z.off = true;
                z.effect = Some(crate::effects::Effect::Spectrum);
            });
            profiles.set_apps("Gaming", &["game.exe".into()]).unwrap();
            let saved = profiles.profiles.clone();
            sim.state().log.clear();
            actions::set_lighting_control(&dev, &mut store, false).unwrap();
            assert!(sim.state().log.is_empty(), "opting out must not send even an off command");
            store = Store::load(&path).unwrap();
            let profiles = store.for_device(&dev);
            assert!(profiles.lighting_off);
            for foreground in [None, Some("game.exe"), None] {
                let name = profiles.effective(foreground);
                assert!(profiles.apply(name, &dev).iter().all(|a| a.result.is_ok()));
            }
            assert!(sim.state().log.iter().all(|c| c.class != 0x03 && c.class != 0x0F));
            assert_eq!(dev.polling_rate().unwrap(), 500);
            if pid == MOUSE_PID {
                assert_eq!(dev.dpi().unwrap(), (1600, 1600));
                assert_eq!(dev.button(5, crate::buttons::Layer::Normal).unwrap(), "key:a".parse().unwrap());
            }
            sim.state().log.clear();
            let zone = &dev.spec.as_ref().unwrap().leds[0].name;
            for result in [
                actions::set_color(&dev, &mut store, zone, Rgb(9, 9, 9)),
                actions::set_effect(&dev, &mut store, zone, crate::effects::Effect::Spectrum),
                actions::set_brightness(&dev, &mut store, zone, 100),
                actions::lights_off(&dev, &mut store, zone),
                actions::set_keys(&dev, &mut store, vec![Rgb(9, 9, 9); 132]),
            ] {
                assert!(matches!(result, Err(Error::InvalidArg(_))));
            }
            assert!(sim.state().log.is_empty(), "editor and CLI writes must be rejected before USB access");
            assert_eq!(store.for_device(&dev).profiles, saved, "saved lighting must not change");
            actions::set_lighting_control(&dev, &mut store, true).unwrap();
            assert!(sim.state().log.is_empty(), "opting in only changes ownership");
            actions::set_color(&dev, &mut store, zone, Rgb(9, 9, 9)).unwrap();
            assert!(!sim.state().log.is_empty(), "opting back in allows RGB writes");
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn sync_skips_disabled_devices_including_identical_models() {
        use crate::{actions, Store};
        let path = std::env::temp_dir().join(format!("secbird-lighting-sync-{}.json", std::process::id()));
        let mut store = Store::load(&path).unwrap();
        let a = Sim::mouse();
        let b = Sim::mouse();
        b.state().serial = "SIMMOUSE0002".into();
        let keyboard = Sim::keyboard();
        let devices = [open(&a, MOUSE_PID), open(&b, MOUSE_PID), open(&keyboard, KEYBOARD_PID)];
        for dev in [&devices[0], &devices[2]] {
            actions::set_lighting_control(dev, &mut store, false).unwrap();
        }
        a.state().log.clear();
        keyboard.state().log.clear();
        for results in [
            actions::sync_color(&devices, &mut store, Rgb(1, 2, 3)),
            actions::sync_effect(&devices, &mut store, &crate::effects::Effect::Spectrum),
        ] {
            assert_eq!(results.len(), 3, "only the second mouse's zones count");
            assert!(results.iter().all(|r| r.key == devices[1].key() && r.result.is_ok()));
        }
        assert!(a.state().log.is_empty());
        assert!(keyboard.state().log.is_empty());
        assert!(!b.state().log.is_empty());
        assert_eq!(*store.for_device(&devices[0]).active(), Profile::default());
        assert_eq!(*store.for_device(&devices[2]).active(), Profile::default());
        actions::set_lighting_control(&devices[1], &mut store, false).unwrap();
        b.state().log.clear();
        assert!(actions::sync_color(&devices, &mut store, Rgb(9, 9, 9)).is_empty());
        assert!(actions::sync_effect(&devices, &mut store, &crate::effects::Effect::Spectrum).is_empty());
        assert!(b.state().log.is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn sync_colors_every_zone_on_every_device() {
        use crate::{actions, Store};
        let path = std::env::temp_dir().join(format!("secbird-sync-{}.json", std::process::id()));
        let mut store = Store::load(&path).unwrap();
        let sims = [
            (Sim::mouse(), MOUSE_PID),
            (Sim::wireless_mouse(), WIRELESS_MOUSE_PID),
            (Sim::keyboard(), KEYBOARD_PID),
            (Sim::old_keyboard(), OLD_KEYBOARD_PID),
        ];
        let devices: Vec<Device> = sims.iter().map(|(s, p)| open(s, *p)).collect();
        let results = actions::sync_color(&devices, &mut store, Rgb(0x11, 0x22, 0x33));
        assert_eq!(results.len(), 3 + 1 + 1 + 2);
        assert!(results.iter().all(|r| r.result.is_ok()));

        assert_eq!(sims[0].0.state().effects[&10][4..7], [0x11, 0x22, 0x33]);
        assert_eq!(sims[2].0.state().effects[&5][4..7], [0x11, 0x22, 0x33]);
        assert_eq!(sims[3].0.state().grid_effect, [cmd::GRID_STATIC, 0x11, 0x22, 0x33]);
        assert_eq!(sims[3].0.state().led_rgb[&4], [0x11, 0x22, 0x33]);
        assert_eq!(Store::load(&path).unwrap().devices.len(), 4);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn identical_devices_get_separate_keys() {
        let a = Sim::mouse();
        let b = Sim::mouse();
        b.state().serial = "SIMMOUSE0002".into();
        let (da, db) = (open(&a, MOUSE_PID), open(&b, MOUSE_PID));
        assert_eq!(da.key(), "FFF1:SIMMOUSE0001");
        assert_eq!(db.key(), "FFF1:SIMMOUSE0002");
        let mut store = crate::Store::default();
        store.for_device(&da).active_mut().dpi = Some((400, 400));
        assert_eq!(store.for_device(&db).active().dpi, None);
    }
}
