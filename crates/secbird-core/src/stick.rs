//! A thumbstick as keys (#21). The Tartarus V2 Pro's stick, in Synapse's True Analog mode,
//! shows up as a game controller. Secretarybird turns where it points into held keys (WASD
//! out of the box), with three things Synapse's 4-way and 8-way modes can't do:
//!
//! - **Rotation**: any angle for "forward", not just Synapse's Default or Rotated (90°).
//! - **Diagonal width**: how wide the zones are where two neighbouring keys are held at once
//!   (W+A when pushed up-left). 0° is a clean 4-way, 45° an even 8-way.
//! - **Dead zone**: how far the stick moves before any key goes down, apart from the angle.
//!
//! Only the maths and the settings live here, so they are tested everywhere; reading the
//! stick and pressing keys is the tray's job, on Windows.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// The four directions, in the order of [`Stick::keys`] and [`Reading::held`].
pub const DIRECTIONS: [&str; 4] = ["up", "right", "down", "left"];

pub const DEADZONE_RANGE: std::ops::RangeInclusive<u8> = 5..=90;
/// Above 80° a held key's zone (with [`HOLD_DEGREES`]) would reach its opposite.
pub const DIAGONAL_RANGE: std::ops::RangeInclusive<u8> = 0..=80;

/// Once down, a key stays down this many degrees past its zone and this much closer to the
/// centre than the dead zone, so it doesn't flicker when the stick rests on a boundary.
pub const HOLD_DEGREES: f32 = 3.0;
pub const HOLD_PUSH: f32 = 0.04;

/// A profile's thumbstick settings. No settings (`None` in the profile) means the stick is
/// left alone and stays a game controller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stick {
    /// Degrees clockwise from the stick's own up to the direction that presses the up key
    /// (0-359). Point the stick where forward should be and use the angle it shows.
    #[serde(default)]
    pub rotation: u16,
    /// Percent of full travel before any key goes down.
    #[serde(default = "default_deadzone")]
    pub deadzone: u8,
    /// Width in degrees of each zone where two neighbouring keys are held.
    #[serde(default = "default_diagonal")]
    pub diagonal: u8,
    /// Keys for up, right, down and left (names as for `key:` actions: `w`, `space`,
    /// `lshift`, `up`). Empty for none.
    #[serde(default = "default_keys")]
    pub keys: [String; 4],
}

fn default_deadzone() -> u8 {
    30
}

fn default_diagonal() -> u8 {
    45
}

fn default_keys() -> [String; 4] {
    ["w", "d", "s", "a"].map(String::from)
}

impl Default for Stick {
    fn default() -> Self {
        Self { rotation: 0, deadzone: default_deadzone(), diagonal: default_diagonal(), keys: default_keys() }
    }
}

/// Where the stick is and which keys that holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Reading {
    /// Degrees clockwise from the stick's own up (0-360).
    pub raw: f32,
    /// The same after rotation: 0 is the direction of the up key.
    pub angle: f32,
    /// 0 at rest to 1 pushed all the way.
    pub push: f32,
    /// Up, right, down, left.
    pub held: [bool; 4],
}

/// Degrees between two compass angles, 0-180.
fn apart(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

impl Stick {
    /// Checked and tidied: rotation within 0-359, known key names in lowercase.
    pub fn checked(mut self) -> Result<Self> {
        self.rotation %= 360;
        if !DEADZONE_RANGE.contains(&self.deadzone) {
            return Err(Error::InvalidArg(format!(
                "dead zone {}% (use {}-{})",
                self.deadzone,
                DEADZONE_RANGE.start(),
                DEADZONE_RANGE.end()
            )));
        }
        if !DIAGONAL_RANGE.contains(&self.diagonal) {
            return Err(Error::InvalidArg(format!(
                "diagonal width {}° (use {}-{})",
                self.diagonal,
                DIAGONAL_RANGE.start(),
                DIAGONAL_RANGE.end()
            )));
        }
        for (key, dir) in self.keys.iter_mut().zip(DIRECTIONS) {
            *key = key.trim().to_lowercase();
            if !key.is_empty() && scan_code(key).is_none() {
                return Err(Error::InvalidArg(format!("unknown key '{key}' for {dir}")));
            }
        }
        Ok(self)
    }

    /// Which keys a stick position holds. `x` and `y` run from -1 to 1, right and up
    /// positive; `before` is what was held at the last reading.
    pub fn read(&self, x: f32, y: f32, before: [bool; 4]) -> Reading {
        let push = x.hypot(y).min(1.0);
        let raw = x.atan2(y).to_degrees().rem_euclid(360.0);
        let angle = (raw - f32::from(self.rotation)).rem_euclid(360.0);
        let half = (90.0 + f32::from(self.diagonal)) / 2.0;
        let deadzone = f32::from(self.deadzone) / 100.0;
        let mut held = [false; 4];
        for (i, h) in held.iter_mut().enumerate() {
            let (width, need) = match before[i] {
                true => (half + HOLD_DEGREES, deadzone - HOLD_PUSH),
                false => (half, deadzone),
            };
            *h = push > need && apart(angle, 90.0 * i as f32) <= width && !self.keys[i].is_empty();
        }
        Reading { raw, angle, push, held }
    }

    /// Short description for the CLI and the tray.
    pub fn summary(&self) -> String {
        let keys: Vec<&str> = self.keys.iter().map(|k| if k.is_empty() { "-" } else { k.as_str() }).collect();
        format!(
            "keys {} · forward at {}° · dead zone {}% · diagonals {}°",
            keys.join("/"),
            self.rotation,
            self.deadzone,
            self.diagonal
        )
    }
}

impl std::str::FromStr for Stick {
    type Err = Error;

    /// `rotation=315 deadzone=25 diagonal=50 keys=w,d,s,a`, any subset, on top of the
    /// defaults. Used by the CLI.
    fn from_str(s: &str) -> Result<Self> {
        let mut stick = Stick::default();
        stick.apply_text(s)?;
        stick.checked()
    }
}

impl Stick {
    /// Change the settings named in `s` (see [`Stick::from_str`]), keeping the others.
    pub fn apply_text(&mut self, s: &str) -> Result<()> {
        let number = |name: &str, v: &str| v.trim_end_matches(['°', '%']).parse::<u16>().map_err(|_| Error::InvalidArg(format!("{name}={v}")));
        for part in s.split_whitespace() {
            let (name, value) = part
                .split_once('=')
                .ok_or_else(|| Error::InvalidArg(format!("'{part}': use name=value (rotation, deadzone, diagonal, keys)")))?;
            match name {
                "rotation" => self.rotation = number(name, value)? % 360,
                "deadzone" => self.deadzone = number(name, value)?.min(255) as u8,
                "diagonal" => self.diagonal = number(name, value)?.min(255) as u8,
                "keys" => {
                    let list: Vec<&str> = value.split(',').collect();
                    if list.len() != 4 {
                        return Err(Error::InvalidArg("keys: four names for up,right,down,left (e.g. w,d,s,a; leave one empty for none)".into()));
                    }
                    for (key, v) in self.keys.iter_mut().zip(list) {
                        *key = v.to_string();
                    }
                }
                _ => return Err(Error::InvalidArg(format!("unknown stick setting '{name}'"))),
            }
        }
        Ok(())
    }
}

/// The keypad's stick modes, as Synapse sets them (docs/tartarus-v2-pro.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The stick is a game controller (Razer's driver makes it one).
    Analog,
    FourWay,
    /// Directions; the stick's position comes in the keypad's own report, in driver mode.
    EightWay,
}

const REPORT_MODE: crate::proto::Command = crate::proto::Command::new(0x02, 0x38);
const MAPPING_MODE: crate::proto::Command = crate::proto::Command::new(0x02, 0x39);
/// Synapse "Set Thumbstick Orientation Options" `[profile, option]`: option 1 turns the stick a
/// quarter turn clockwise (firmware 1.2), 0 is as it comes. Logged from Synapse as
/// `02 45 01 01` and seen in USB captures; read back with 02:C5 `[profile]`.
const ORIENTATION: crate::proto::Command = crate::proto::Command::new(0x02, 0x45);

impl crate::Device {
    /// Set the stick's mode the way Synapse does: report mode 02:38, then mapping mode
    /// 02:39 for profile 1. Tested on the keypad (with Synapse closed).
    pub fn set_stick_mode(&self, mode: Mode) -> Result<()> {
        let (report, mapping) = match mode {
            Mode::Analog => (0x00, 0x00),
            Mode::FourWay => (0x01, 0x01),
            Mode::EightWay => (0x01, 0x02),
        };
        self.transact(REPORT_MODE, 1, &[report])?;
        self.transact(MAPPING_MODE, 2, &[0x01, mapping])?;
        Ok(())
    }

    /// Synapse's Thumbstick Orientation for profile 1: `true` turns the stick a quarter turn
    /// clockwise in the keypad itself (02:45 `[01, 01]`), `false` leaves it as it comes
    /// (`[01, 00]`). The keypad keeps it.
    pub fn set_stick_rotated(&self, rotated: bool) -> Result<()> {
        self.transact(ORIENTATION, 2, &[0x01, u8::from(rotated)]).map(|_| ())
    }

    /// Whether the keypad's own quarter turn is on (02:C5 `[01]`).
    pub fn stick_rotated(&self) -> Result<bool> {
        Ok(self.get(ORIENTATION, 2, &[0x01])?.args()[1] == 1)
    }

    /// The stick's mode, `None` for a combination Synapse doesn't set.
    pub fn stick_mode(&self) -> Result<Option<Mode>> {
        let report = self.get(REPORT_MODE, 1, &[])?.args()[0];
        let mapping = self.get(MAPPING_MODE, 2, &[0x01])?.args()[1];
        Ok(match (report, mapping) {
            (0, 0) => Some(Mode::Analog),
            (1, 1) => Some(Mode::FourWay),
            (1, 2) => Some(Mode::EightWay),
            _ => None,
        })
    }

    /// Driver mode (00:04 `[03 00]`), which Synapse sets when it starts. The keypad only sends
    /// its stick's position in it; its other keys keep typing (tested on the keypad).
    pub fn set_driver_mode(&self) -> Result<()> {
        use crate::commands::{DEVICE_MODE, DEVICE_MODE_SIZE};
        self.transact(DEVICE_MODE, DEVICE_MODE_SIZE, &[0x03, 0x00]).map(|_| ())
    }

    /// Normal mode (00:04 `[00 00]`), which Synapse sets when it closes and the keypad is in
    /// after plugging in. The keypad uses its own keymap (key remaps) only in this mode.
    pub fn set_normal_mode(&self) -> Result<()> {
        use crate::commands::{DEVICE_MODE, DEVICE_MODE_SIZE};
        self.transact(DEVICE_MODE, DEVICE_MODE_SIZE, &[0x00, 0x00]).map(|_| ())
    }
}

/// The stick's position from the keypad's own analog report, which it sends on interface 1
/// in 4-way and 8-way mode (Synapse's `analogKeyboardV2` report, id 0x0B), even with every
/// direction disabled, when Windows sees no controller and no keys from the stick. After
/// the id come 3-byte entries `[input, value hi, value lo]` for each input away from rest,
/// values signed; inputs 0xFB and 0xFC are the stick (found with `secbird listen`, #21:
/// pushing up raises 0xFC, pushing right raises 0xFB, confirmed by turning the stick on the
/// keypad). Returns -1..1 with right and up
/// positive, or `None` for another report.
pub fn raw_report(report: &[u8]) -> Option<(f32, f32)> {
    let (&id, entries) = report.split_first()?;
    if id != 0x0B {
        return None;
    }
    let (mut x, mut y) = (0.0, 0.0);
    for e in entries.chunks_exact(3) {
        let v = f32::from(i16::from_be_bytes([e[1], e[2]])) / 32767.0;
        match e[0] {
            0xFB => x = v,
            0xFC => y = v,
            _ => {}
        }
    }
    Some((x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0)))
}

/// Turns readings into key presses: which keys are down now, and what to press or release
/// when the stick moves or the settings change. `send(code, down)` does the pressing.
#[derive(Debug, Default)]
pub struct Presser {
    /// Scan codes this has pressed and not released.
    down: Vec<u16>,
    before: [bool; 4],
}

impl Presser {
    pub fn update(&mut self, stick: &Stick, x: f32, y: f32, send: &mut impl FnMut(u16, bool)) -> Reading {
        let reading = stick.read(x, y, self.before);
        self.before = reading.held;
        let mut want: Vec<u16> = Vec::new();
        for (held, key) in reading.held.iter().zip(&stick.keys) {
            if let Some(code) = held.then(|| scan_code(key)).flatten() {
                if !want.contains(&code) {
                    want.push(code);
                }
            }
        }
        self.set(want, send);
        reading
    }

    /// Let go of every key this holds (stick off, PC locked, device gone).
    pub fn release(&mut self, send: &mut impl FnMut(u16, bool)) {
        self.before = [false; 4];
        self.set(Vec::new(), send);
    }

    pub fn holding(&self) -> bool {
        !self.down.is_empty()
    }

    fn set(&mut self, want: Vec<u16>, send: &mut impl FnMut(u16, bool)) {
        for &code in self.down.iter().filter(|c| !want.contains(c)) {
            send(code, false);
        }
        for &code in want.iter().filter(|c| !self.down.contains(c)) {
            send(code, true);
        }
        self.down = want;
    }
}

/// The PC (set 1) scan code a key name types, for sending it the way a keyboard would;
/// extended keys carry 0xE0 in the high byte. Games read scan codes, not characters.
pub fn scan_code(name: &str) -> Option<u16> {
    let name = match name {
        "shift" => "lshift",
        "ctrl" => "lctrl",
        "alt" => "lalt",
        "win" => "lwin",
        other => other,
    };
    let usage = crate::buttons::KEYS.iter().find(|k| k.1 == name).map(|k| k.3)?;
    usage_scan_code(usage)
}

pub fn usage_scan_code(usage: u8) -> Option<u16> {
    const LETTERS: [u16; 26] = [
        0x1E, 0x30, 0x2E, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18, 0x19, 0x10, 0x13,
        0x1F, 0x14, 0x16, 0x2F, 0x11, 0x2D, 0x15, 0x2C,
    ];
    Some(match usage {
        0x04..=0x1D => LETTERS[(usage - 0x04) as usize],
        0x1E..=0x27 => 0x02 + u16::from(usage - 0x1E), // 1-9, 0
        0x28 => 0x1C,
        0x29 => 0x01,
        0x2A => 0x0E,
        0x2B => 0x0F,
        0x2C => 0x39,
        0x2D => 0x0C,
        0x2E => 0x0D,
        0x2F => 0x1A,
        0x30 => 0x1B,
        0x31 | 0x32 => 0x2B,
        0x33 => 0x27,
        0x34 => 0x28,
        0x35 => 0x29,
        0x36 => 0x33,
        0x37 => 0x34,
        0x38 => 0x35,
        0x39 => 0x3A,
        0x3A..=0x43 => 0x3B + u16::from(usage - 0x3A), // F1-F10
        0x44 => 0x57,
        0x45 => 0x58,
        0x46 => 0xE037,
        0x47 => 0x46,
        0x49 => 0xE052,
        0x4A => 0xE047,
        0x4B => 0xE049,
        0x4C => 0xE053,
        0x4D => 0xE04F,
        0x4E => 0xE051,
        0x4F => 0xE04D,
        0x50 => 0xE04B,
        0x51 => 0xE050,
        0x52 => 0xE048,
        0x53 => 0x45,
        0x54 => 0xE035,
        0x55 => 0x37,
        0x56 => 0x4A,
        0x57 => 0x4E,
        0x58 => 0xE01C,
        0x59 => 0x4F,
        0x5A => 0x50,
        0x5B => 0x51,
        0x5C => 0x4B,
        0x5D => 0x4C,
        0x5E => 0x4D,
        0x5F => 0x47,
        0x60 => 0x48,
        0x61 => 0x49,
        0x62 => 0x52,
        0x63 => 0x53,
        0x64 => 0x56,
        0x65 => 0xE05D,
        0xE0 => 0x1D,
        0xE1 => 0x2A,
        0xE2 => 0x38,
        0xE3 => 0xE05B,
        0xE4 => 0xE01D,
        0xE5 => 0x36,
        0xE6 => 0xE038,
        // Pause sends a sequence, not one code; Japanese keys vary by layout.
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full push towards a compass angle.
    fn at(stick: &Stick, degrees: f32) -> [bool; 4] {
        let r = degrees.to_radians();
        stick.read(r.sin(), r.cos(), [false; 4]).held
    }

    /// Half-way between two axes, at full push.
    const H: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const W: [bool; 4] = [true, false, false, false];
    const WD: [bool; 4] = [true, true, false, false];
    const D: [bool; 4] = [false, true, false, false];
    const NONE: [bool; 4] = [false; 4];

    #[test]
    fn default_is_an_even_eight_way_wasd() {
        let s = Stick::default();
        assert_eq!(at(&s, 0.0), W);
        assert_eq!(at(&s, 20.0), W);
        assert_eq!(at(&s, 25.0), WD);
        assert_eq!(at(&s, 45.0), WD);
        assert_eq!(at(&s, 65.0), WD);
        assert_eq!(at(&s, 70.0), D);
        assert_eq!(at(&s, 225.0), [false, false, true, true]);
    }

    #[test]
    fn diagonal_zero_is_four_way() {
        let s = Stick { diagonal: 0, ..Stick::default() };
        assert_eq!(at(&s, 44.0), W);
        assert_eq!(at(&s, 46.0), D);
    }

    #[test]
    fn rotation_moves_forward() {
        // The stick's own up-left (315°) is where forward should be.
        let s = Stick { rotation: 315, ..Stick::default() };
        assert_eq!(at(&s, 315.0), W);
        assert_eq!(at(&s, 45.0), D);
        assert_eq!(at(&s, 0.0), WD);
        let r = s.read(-H, H, NONE);
        assert!(r.angle < 0.5 || r.angle > 359.5, "{r:?}");
        assert!((r.raw - 315.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn dead_zone_and_hold() {
        let s = Stick::default();
        assert_eq!(s.read(0.0, 0.25, NONE).held, NONE);
        assert_eq!(s.read(0.0, 0.35, NONE).held, W);
        // Once down, a key stays down a little inside the dead zone and past its zone.
        assert_eq!(s.read(0.0, 0.28, W).held, W);
        assert_eq!(s.read(0.0, 0.28, NONE).held, NONE);
        let r = 20f32.to_radians();
        assert_eq!(s.read(r.sin(), r.cos(), WD).held, WD);
        assert_eq!(s.read(r.sin(), r.cos(), W).held, W);
    }

    #[test]
    fn opposite_keys_never_together() {
        for diagonal in [0, 45, 80] {
            let s = Stick { diagonal, ..Stick::default() };
            for tenth in 0..3600 {
                let r = (tenth as f32 / 10.0).to_radians();
                let h = s.read(r.sin(), r.cos(), [true; 4]).held;
                assert!(!(h[0] && h[2]) && !(h[1] && h[3]), "diagonal {diagonal} at {}: {h:?}", tenth as f32 / 10.0);
            }
        }
    }

    #[test]
    fn empty_key_holds_nothing() {
        let mut s = Stick::default();
        s.keys[1] = String::new();
        assert_eq!(at(&s, 90.0), NONE);
        assert_eq!(at(&s, 45.0), W);
    }

    #[test]
    fn settings_from_json_and_text() {
        let s: Stick = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Stick::default());
        let s: Stick = "rotation=405 deadzone=25% diagonal=50 keys=W,d,s,a".parse().unwrap();
        assert_eq!((s.rotation, s.deadzone, s.diagonal), (45, 25, 50));
        assert_eq!(s.keys[0], "w");
        assert!("deadzone=99".parse::<Stick>().is_err());
        assert!("diagonal=85".parse::<Stick>().is_err());
        assert!("keys=w,d,s,nope".parse::<Stick>().is_err());
        assert!("keys=w,d".parse::<Stick>().is_err());
        assert!("speed=3".parse::<Stick>().is_err());
        assert_eq!("keys=up,,down,".parse::<Stick>().unwrap().keys, ["up", "", "down", ""].map(String::from));
    }

    #[test]
    fn presser_sends_each_change_once() {
        let s = Stick::default();
        let mut p = Presser::default();
        let mut sent = Vec::new();
        let mut send = |code, down| sent.push((code, down));
        p.update(&s, 0.0, 1.0, &mut send); // up: W down
        p.update(&s, 0.0, 1.0, &mut send); // still up: nothing
        p.update(&s, H, H, &mut send); // up-right: D down too
        p.update(&s, 1.0, 0.0, &mut send); // right: W up
        p.update(&s, 0.0, 0.0, &mut send); // centre: D up
        assert_eq!(sent, [(0x11, true), (0x20, true), (0x11, false), (0x20, false)]);
        assert!(!p.holding());
    }

    #[test]
    fn presser_follows_new_keys_and_releases() {
        let mut p = Presser::default();
        let mut sent = Vec::new();
        let mut send = |code, down| sent.push((code, down));
        p.update(&Stick::default(), 0.0, 1.0, &mut send);
        let arrows: Stick = "keys=up,right,down,left".parse().unwrap();
        p.update(&arrows, 0.0, 1.0, &mut send); // same push, new key: W up, Up arrow down
        p.release(&mut send);
        assert_eq!(sent, [(0x11, true), (0x11, false), (0xE048, true), (0xE048, false)]);
        // Two directions on one key: held once, let go once.
        let same: Stick = "keys=w,w,s,a".parse().unwrap();
        sent_clear(&mut p, &same);
    }

    fn sent_clear(p: &mut Presser, s: &Stick) {
        let mut sent = Vec::new();
        let mut send = |code, down| sent.push((code, down));
        p.update(s, H, H, &mut send);
        p.update(s, 1.0, 0.0, &mut send);
        p.update(s, 0.0, 0.0, &mut send);
        assert_eq!(sent, [(0x11, true), (0x11, false)]);
    }

    #[test]
    fn raw_reports_from_the_keypad() {
        // Lines of `secbird listen` on a Tartarus V2 Pro in 8-way mode (zeros trimmed there).
        let mut r = [0u8; 48];
        r[..7].copy_from_slice(&[0x0B, 0xFB, 0xFB, 0x3E, 0xFC, 0x07, 0xB1]);
        let (x, y) = raw_report(&r).unwrap();
        assert!((x + 1218.0 / 32767.0).abs() < 1e-4 && (y - 1969.0 / 32767.0).abs() < 1e-4, "{x} {y}");
        // At rest: no entries.
        assert_eq!(raw_report(&[0x0B, 0, 0, 0]), Some((0.0, 0.0)));
        // Full push up and right.
        let (x, y) = raw_report(&[0x0B, 0xFC, 0x7F, 0xFF, 0xFB, 0x7F, 0xFF]).unwrap();
        assert!((x - 1.0).abs() < 1e-4 && (y - 1.0).abs() < 1e-4);
        // Other inputs (analog keys) are left out; other reports aren't the stick.
        assert_eq!(raw_report(&[0x0B, 0x10, 0x40, 0x00]), Some((0.0, 0.0)));
        assert_eq!(raw_report(&[0x05, 0x02, 0x03]), None);
        assert_eq!(raw_report(&[]), None);
    }

    #[test]
    fn scan_codes() {
        assert_eq!(scan_code("w"), Some(0x11));
        assert_eq!(scan_code("a"), Some(0x1E));
        assert_eq!(scan_code("0"), Some(0x0B));
        assert_eq!(scan_code("space"), Some(0x39));
        assert_eq!(scan_code("shift"), Some(0x2A));
        assert_eq!(scan_code("up"), Some(0xE048));
        assert_eq!(scan_code("f11"), Some(0x57));
        assert_eq!(scan_code("num8"), Some(0x48));
        assert_eq!(scan_code("pause"), None);
        assert_eq!(scan_code("nope"), None);
        // Every key name we accept from the key list has a code, apart from the noted few.
        for (_, name, _, _) in crate::buttons::KEYS {
            let skip = ["pause", "yen", "ro", "muhenkan", "kana", "henkan"].contains(name);
            assert!(skip || scan_code(name).is_some(), "{name}");
        }
    }
}
