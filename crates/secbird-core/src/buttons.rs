//! Button remapping on mice that keep button assignments on board.
//!
//! Each button has a function per layer (normal, and HyperShift while the HyperShift
//! button is held). Layouts come from Synapse's button-assignment encoders; key and
//! mouse-button assignments are verified on a Basilisk V3, the others are not yet.
//!
//! Settings are written to the live slot ([`cmd::BUTTON_LIVE_SLOT`]); the saved slots
//! 1-5 keep whatever Synapse or the factory put there, and "reset" copies a button back
//! from the active saved slot.

use crate::commands as cmd;
use crate::device::Device;
use crate::error::{Error, Result};
use std::fmt;
use std::str::FromStr;

/// Buttons a Razer mouse may have: (firmware id, name used in profiles and the CLI, label).
pub const BUTTONS: &[(u8, &str, &str)] = &[
    (1, "left", "Left click"),
    (2, "right", "Right click"),
    (3, "wheel", "Wheel click"),
    (4, "side-back", "Side button (back)"),
    (5, "side-forward", "Side button (forward)"),
    (9, "scroll-up", "Scroll up"),
    (10, "scroll-down", "Scroll down"),
    (11, "dpi-up", "DPI up button"),
    (12, "dpi-down", "DPI down button"),
    (14, "profile", "Profile button"),
    (15, "sniper", "Sniper button"),
    (52, "tilt-left", "Wheel tilt left"),
    (53, "tilt-right", "Wheel tilt right"),
    (57, "hypershift", "HyperShift button"),
    // Naga side plates (Synapse SPAD_NUM_1-12 and HPAD_NUM_1-7). Factory keys 1-9, 0, -, =.
    (64, "grid-1", "Side grid 1"),
    (65, "grid-2", "Side grid 2"),
    (66, "grid-3", "Side grid 3"),
    (67, "grid-4", "Side grid 4"),
    (68, "grid-5", "Side grid 5"),
    (69, "grid-6", "Side grid 6"),
    (70, "grid-7", "Side grid 7"),
    (71, "grid-8", "Side grid 8"),
    (72, "grid-9", "Side grid 9"),
    (73, "grid-10", "Side grid 10"),
    (74, "grid-11", "Side grid 11"),
    (75, "grid-12", "Side grid 12"),
    (80, "hex-1", "6-button side 1"),
    (81, "hex-2", "6-button side 2"),
    (82, "hex-3", "6-button side 3"),
    (83, "hex-4", "6-button side 4"),
    (84, "hex-5", "6-button side 5"),
    (85, "hex-6", "6-button side 6"),
    (86, "hex-7", "6-button side 7"),
    (96, "dpi", "DPI button"),
    (97, "dpi-2", "Second DPI button"),
    (104, "scroll-left", "Scroll left"),
    (105, "scroll-right", "Scroll right"),
    (106, "wheel-mode", "Wheel mode button"),
    // Basilisk V4 Pro: the extra side button, Synapse's "AI Prompt Master" (#16, #54). Named
    // top-extra until 0.5; profiles saved with that name still find it.
    (128, "side-extra", "Extra side button"),
];

/// The id of a button name; buttons not in [`BUTTONS`] are named `button-<id>`.
pub fn button_id(name: &str) -> Option<u8> {
    let name = if name == "top-extra" { "side-extra" } else { name };
    BUTTONS.iter().find(|b| b.1 == name).map(|b| b.0).or_else(|| name.strip_prefix("button-")?.parse().ok())
}

pub fn button_name(id: u8) -> String {
    BUTTONS.iter().find(|b| b.0 == id).map_or_else(|| format!("button-{id}"), |b| b.1.to_string())
}

pub fn button_label(id: u8) -> String {
    BUTTONS.iter().find(|b| b.0 == id).map_or_else(|| format!("Button {id}"), |b| b.2.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Normal = 0,
    HyperShift = 1,
}

/// Keyboard keys: (position id, name used in profiles and the CLI, label, USB usage). The
/// ids are the IBM PC key position numbers Synapse uses for keyboards (its `analogKeyID`
/// table); the usage is what the key types out of the box.
pub const KEYS: &[(u8, &str, &str, u8)] = &[
    (1, "grave", "` (left of 1)", 0x35),
    (2, "1", "1", 0x1E),
    (3, "2", "2", 0x1F),
    (4, "3", "3", 0x20),
    (5, "4", "4", 0x21),
    (6, "5", "5", 0x22),
    (7, "6", "6", 0x23),
    (8, "7", "7", 0x24),
    (9, "8", "8", 0x25),
    (10, "9", "9", 0x26),
    (11, "0", "0", 0x27),
    (12, "minus", "-", 0x2D),
    (13, "equal", "=", 0x2E),
    (14, "yen", "Yen (Japanese)", 0x89),
    (15, "backspace", "Backspace", 0x2A),
    (16, "tab", "Tab", 0x2B),
    (17, "q", "Q", 0x14),
    (18, "w", "W", 0x1A),
    (19, "e", "E", 0x08),
    (20, "r", "R", 0x15),
    (21, "t", "T", 0x17),
    (22, "y", "Y", 0x1C),
    (23, "u", "U", 0x18),
    (24, "i", "I", 0x0C),
    (25, "o", "O", 0x12),
    (26, "p", "P", 0x13),
    (27, "lbracket", "[", 0x2F),
    (28, "rbracket", "]", 0x30),
    (29, "backslash", "\\", 0x31),
    (30, "capslock", "Caps Lock", 0x39),
    (31, "a", "A", 0x04),
    (32, "s", "S", 0x16),
    (33, "d", "D", 0x07),
    (34, "f", "F", 0x09),
    (35, "g", "G", 0x0A),
    (36, "h", "H", 0x0B),
    (37, "j", "J", 0x0D),
    (38, "k", "K", 0x0E),
    (39, "l", "L", 0x0F),
    (40, "semicolon", ";", 0x33),
    (41, "quote", "'", 0x34),
    (42, "iso-hash", "# (ISO, left of Enter)", 0x32),
    (43, "enter", "Enter", 0x28),
    (44, "lshift", "Left Shift", 0xE1),
    (45, "iso-backslash", "\\ (ISO, right of Left Shift)", 0x64),
    (46, "z", "Z", 0x1D),
    (47, "x", "X", 0x1B),
    (48, "c", "C", 0x06),
    (49, "v", "V", 0x19),
    (50, "b", "B", 0x05),
    (51, "n", "N", 0x11),
    (52, "m", "M", 0x10),
    (53, "comma", ",", 0x36),
    (54, "period", ".", 0x37),
    (55, "slash", "/", 0x38),
    (56, "ro", "Ro (Japanese)", 0x87),
    (57, "rshift", "Right Shift", 0xE5),
    (58, "lctrl", "Left Ctrl", 0xE0),
    (60, "lalt", "Left Alt", 0xE2),
    (61, "space", "Space", 0x2C),
    (62, "ralt", "Right Alt", 0xE6),
    (64, "rctrl", "Right Ctrl", 0xE4),
    (75, "insert", "Insert", 0x49),
    (76, "delete", "Delete", 0x4C),
    (79, "left", "Left arrow", 0x50),
    (80, "home", "Home", 0x4A),
    (81, "end", "End", 0x4D),
    (83, "up", "Up arrow", 0x52),
    (84, "down", "Down arrow", 0x51),
    (85, "pageup", "Page Up", 0x4B),
    (86, "pagedown", "Page Down", 0x4E),
    (89, "right", "Right arrow", 0x4F),
    (90, "numlock", "Num Lock", 0x53),
    (91, "num7", "Numpad 7", 0x5F),
    (92, "num4", "Numpad 4", 0x5C),
    (93, "num1", "Numpad 1", 0x59),
    (95, "num-slash", "Numpad /", 0x54),
    (96, "num8", "Numpad 8", 0x60),
    (97, "num5", "Numpad 5", 0x5D),
    (98, "num2", "Numpad 2", 0x5A),
    (99, "num0", "Numpad 0", 0x62),
    (100, "num-star", "Numpad *", 0x55),
    (101, "num9", "Numpad 9", 0x61),
    (102, "num6", "Numpad 6", 0x5E),
    (103, "num3", "Numpad 3", 0x5B),
    (104, "num-period", "Numpad .", 0x63),
    (105, "num-minus", "Numpad -", 0x56),
    (106, "num-plus", "Numpad +", 0x57),
    (108, "num-enter", "Numpad Enter", 0x58),
    (110, "esc", "Esc", 0x29),
    (112, "f1", "F1", 0x3A),
    (113, "f2", "F2", 0x3B),
    (114, "f3", "F3", 0x3C),
    (115, "f4", "F4", 0x3D),
    (116, "f5", "F5", 0x3E),
    (117, "f6", "F6", 0x3F),
    (118, "f7", "F7", 0x40),
    (119, "f8", "F8", 0x41),
    (120, "f9", "F9", 0x42),
    (121, "f10", "F10", 0x43),
    (122, "f11", "F11", 0x44),
    (123, "f12", "F12", 0x45),
    (124, "printscreen", "Print Screen", 0x46),
    (125, "scrolllock", "Scroll Lock", 0x47),
    (126, "pause", "Pause", 0x48),
    (127, "lwin", "Windows", 0xE3),
    (129, "menu", "Menu", 0x65),
    (150, "muhenkan", "Muhenkan (Japanese)", 0x8B),
    (151, "kana", "Katakana/Hiragana (Japanese)", 0x88),
    (154, "henkan", "Henkan (Japanese)", 0x8A),
];

/// Profile and CLI names of keys carry this prefix, so `key-left` (the arrow) and `left`
/// (the mouse button) stay apart.
const KEY_PREFIX: &str = "key-";

/// A key's id from its name without the `key-` prefix (`capslock`, or `id57`).
pub fn key_id(name: &str) -> Option<u8> {
    KEYS.iter().find(|k| k.1 == name).map(|k| k.0).or_else(|| name.strip_prefix("id")?.parse().ok())
}

/// A remappable input: a mouse button or a keyboard key. Each has its own id space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Button(u8),
    Key(u8),
}

impl Input {
    /// `side-forward`, `key-capslock`; unnamed ids are `button-<id>` and `key-id<id>`.
    pub fn parse(name: &str) -> Option<Input> {
        match name.strip_prefix(KEY_PREFIX) {
            Some(key) => key_id(key).map(Input::Key),
            None => button_id(name).map(Input::Button),
        }
    }

    pub fn name(self) -> String {
        match self {
            Input::Button(id) => button_name(id),
            Input::Key(id) => {
                let name = KEYS.iter().find(|k| k.0 == id).map_or_else(|| format!("id{id}"), |k| k.1.to_string());
                format!("{KEY_PREFIX}{name}")
            }
        }
    }

    pub fn label(self) -> String {
        match self {
            Input::Button(id) => button_label(id),
            Input::Key(id) => KEYS.iter().find(|k| k.0 == id).map_or_else(|| format!("Key {id}"), |k| k.2.to_string()),
        }
    }

    /// The assignment command, its size and the id it carries.
    fn command(self) -> (crate::proto::Command, u8, u8) {
        match self {
            Input::Button(id) => (cmd::BUTTON, cmd::BUTTON_SIZE, id),
            Input::Key(id) => (cmd::KEY, cmd::KEY_SIZE, id),
        }
    }
}

/// Profile key of a button or key on a layer: `side-forward`, `key-capslock/hypershift`.
pub fn setting_key(button: &str, layer: Layer) -> String {
    match layer {
        Layer::Normal => button.to_string(),
        Layer::HyperShift => format!("{button}/hypershift"),
    }
}

/// Inverse of [`setting_key`]: `(input, layer)`.
pub fn parse_setting_key(key: &str) -> Result<(Input, Layer)> {
    let (name, layer) = match key.strip_suffix("/hypershift") {
        Some(name) => (name, Layer::HyperShift),
        None => (key, Layer::Normal),
    };
    let input = Input::parse(name).ok_or_else(|| Error::InvalidArg(format!("unknown button or key '{name}'")))?;
    Ok((input, layer))
}

/// What a button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Disabled,
    /// A mouse button code: 1 left, 2 right, 3 middle, 4 back, 5 forward,
    /// 9/10 scroll up/down, 0x68/0x69 scroll left/right.
    Mouse(u8),
    DoubleClick,
    Key { modifiers: u8, usage: u8 },
    /// Repeats while held, `rate` presses per second.
    TurboMouse { button: u8, rate: u8 },
    TurboKey { modifiers: u8, usage: u8, rate: u8 },
    DpiUp,
    DpiDown,
    DpiCycleUp,
    DpiCycleDown,
    /// DPI while held (sniper).
    DpiClutch { x: u16, y: u16 },
    ProfileUp,
    ProfileDown,
    ProfileCycleUp,
    ProfileCycleDown,
    ProfileSlot(u8),
    /// Hold for the HyperShift layer.
    HyperShift,
    /// A USB consumer-control usage (volume, media).
    Media(u16),
    /// Scroll wheel mode: 1 switches free-spin/notched.
    ScrollMode(u8),
    /// Play an on-device macro (see [`crate::macros`]).
    Macro { id: u16, play: MacroPlay },
    /// Anything else read from a device, kept so it can be written back unchanged.
    Raw { function: u8, data: [u8; 5] },
}

/// How a button plays its macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroPlay {
    /// This many times per press.
    Times(u8),
    /// Over and over while the button is held.
    WhileHeld,
    /// Press to start repeating, press again to stop.
    Toggle,
}

// Function ids (Synapse's `Vh` enum).
const FN_OFF: u8 = 0;
const FN_BUTTON: u8 = 1;
const FN_KEY: u8 = 2;
const FN_MACRO_TIMES: u8 = 3;
const FN_MACRO_HELD: u8 = 4;
const FN_MACRO_TOGGLE: u8 = 5;
const FN_DPI: u8 = 6;
const FN_PROFILE: u8 = 7;
const FN_MEDIA: u8 = 10;
const FN_DOUBLE_CLICK: u8 = 11;
const FN_HYPERSHIFT: u8 = 12;
const FN_TURBO_KEY: u8 = 13;
const FN_TURBO_BUTTON: u8 = 14;
const FN_SCROLL_MODE: u8 = 18;

/// Synapse's default turbo rate.
pub const DEFAULT_TURBO_RATE: u8 = 7;

fn interval_ms(rate: u8) -> [u8; 2] {
    (1000 / u16::from(rate.max(1))).to_be_bytes()
}

fn rate_from(hi: u8, lo: u8) -> u8 {
    let ms = u16::from_be_bytes([hi, lo]).max(1);
    (1000 / ms).clamp(1, 255) as u8
}

impl Action {
    /// `(function, data size, data)` as sent in the assignment command.
    pub fn encode(&self) -> (u8, u8, [u8; 5]) {
        let mut d = [0u8; 5];
        let (function, size) = match *self {
            Action::Disabled => (FN_OFF, 0),
            Action::Mouse(b) => {
                d[0] = b;
                (FN_BUTTON, 1)
            }
            Action::DoubleClick => {
                d[0] = 1;
                (FN_DOUBLE_CLICK, 1)
            }
            Action::Key { modifiers, usage } => {
                d[..2].copy_from_slice(&[modifiers, usage]);
                (FN_KEY, 2)
            }
            Action::TurboMouse { button, rate } => {
                d[0] = button;
                d[1..3].copy_from_slice(&interval_ms(rate));
                (FN_TURBO_BUTTON, 3)
            }
            Action::TurboKey { modifiers, usage, rate } => {
                d[..2].copy_from_slice(&[modifiers, usage]);
                d[2..4].copy_from_slice(&interval_ms(rate));
                (FN_TURBO_KEY, 4)
            }
            Action::DpiUp => one(&mut d, FN_DPI, 1),
            Action::DpiDown => one(&mut d, FN_DPI, 2),
            Action::DpiCycleUp => one(&mut d, FN_DPI, 6),
            Action::DpiCycleDown => one(&mut d, FN_DPI, 7),
            Action::DpiClutch { x, y } => {
                let ([xh, xl], [yh, yl]) = (x.to_be_bytes(), y.to_be_bytes());
                d = [5, xh, xl, yh, yl];
                (FN_DPI, 5)
            }
            Action::ProfileUp => one(&mut d, FN_PROFILE, 1),
            Action::ProfileDown => one(&mut d, FN_PROFILE, 2),
            Action::ProfileCycleUp => one(&mut d, FN_PROFILE, 4),
            Action::ProfileCycleDown => one(&mut d, FN_PROFILE, 5),
            Action::ProfileSlot(slot) => {
                d[..2].copy_from_slice(&[3, slot]);
                (FN_PROFILE, 2)
            }
            Action::HyperShift => one(&mut d, FN_HYPERSHIFT, 1),
            Action::Media(usage) => {
                d[..2].copy_from_slice(&usage.to_be_bytes());
                (FN_MEDIA, 2)
            }
            Action::ScrollMode(mode) => one(&mut d, FN_SCROLL_MODE, mode),
            Action::Macro { id, play } => {
                d[..2].copy_from_slice(&id.to_be_bytes());
                match play {
                    MacroPlay::Times(n) => {
                        d[2] = n;
                        (FN_MACRO_TIMES, 3)
                    }
                    MacroPlay::WhileHeld => (FN_MACRO_HELD, 2),
                    MacroPlay::Toggle => (FN_MACRO_TOGGLE, 2),
                }
            }
            Action::Raw { function, data } => {
                d = data;
                let size = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1) as u8;
                (function, size)
            }
        };
        (function, size, d)
    }

    pub fn decode(function: u8, d: [u8; 5]) -> Action {
        match (function, d[0]) {
            (FN_OFF, _) => Action::Disabled,
            (FN_BUTTON, b) => Action::Mouse(b),
            (FN_DOUBLE_CLICK, _) => Action::DoubleClick,
            (FN_KEY, _) => Action::Key { modifiers: d[0], usage: d[1] },
            (FN_TURBO_BUTTON, b) => Action::TurboMouse { button: b, rate: rate_from(d[1], d[2]) },
            (FN_TURBO_KEY, _) => Action::TurboKey { modifiers: d[0], usage: d[1], rate: rate_from(d[2], d[3]) },
            (FN_DPI, 1) => Action::DpiUp,
            (FN_DPI, 2) => Action::DpiDown,
            (FN_DPI, 5) => {
                Action::DpiClutch { x: u16::from_be_bytes([d[1], d[2]]), y: u16::from_be_bytes([d[3], d[4]]) }
            }
            (FN_DPI, 6) => Action::DpiCycleUp,
            (FN_DPI, 7) => Action::DpiCycleDown,
            (FN_PROFILE, 1) => Action::ProfileUp,
            (FN_PROFILE, 2) => Action::ProfileDown,
            (FN_PROFILE, 3) => Action::ProfileSlot(d[1]),
            (FN_PROFILE, 4) => Action::ProfileCycleUp,
            (FN_PROFILE, 5) => Action::ProfileCycleDown,
            (FN_HYPERSHIFT, _) => Action::HyperShift,
            (FN_MEDIA, _) => Action::Media(u16::from_be_bytes([d[0], d[1]])),
            (FN_SCROLL_MODE, m) => Action::ScrollMode(m),
            (FN_MACRO_TIMES, _) => Action::Macro { id: u16::from_be_bytes([d[0], d[1]]), play: MacroPlay::Times(d[2].max(1)) },
            (FN_MACRO_HELD, _) => Action::Macro { id: u16::from_be_bytes([d[0], d[1]]), play: MacroPlay::WhileHeld },
            (FN_MACRO_TOGGLE, _) => Action::Macro { id: u16::from_be_bytes([d[0], d[1]]), play: MacroPlay::Toggle },
            _ => Action::Raw { function, data: d },
        }
    }
}

fn one(d: &mut [u8; 5], function: u8, value: u8) -> (u8, u8) {
    d[0] = value;
    (function, 1)
}

const MOUSE_NAMES: &[(u8, &str)] = &[
    (1, "left"),
    (2, "right"),
    (3, "middle"),
    (4, "back"),
    (5, "forward"),
    (9, "scroll-up"),
    (10, "scroll-down"),
    (0x68, "scroll-left"),
    (0x69, "scroll-right"),
];

const MEDIA_NAMES: &[(u16, &str)] = &[
    (0xE9, "volume-up"),
    (0xEA, "volume-down"),
    (0xE2, "mute"),
    (0xCD, "play-pause"),
    (0xB5, "next-track"),
    (0xB6, "previous-track"),
    (0xB7, "stop"),
];

/// USB keyboard modifier bits.
pub(crate) const MODIFIERS: &[(u8, &str)] = &[
    (0x01, "ctrl"),
    (0x02, "shift"),
    (0x04, "alt"),
    (0x08, "win"),
    (0x10, "rctrl"),
    (0x20, "rshift"),
    (0x40, "ralt"),
    (0x80, "rwin"),
];

/// USB HID keyboard usages by name.
pub(crate) fn key_usage(name: &str) -> Option<u8> {
    let b = name.as_bytes();
    match name {
        _ if b.len() == 1 && b[0].is_ascii_lowercase() => Some(0x04 + (b[0] - b'a')),
        "0" => Some(0x27),
        _ if b.len() == 1 && (b'1'..=b'9').contains(&b[0]) => Some(0x1E + (b[0] - b'1')),
        _ if name.starts_with('f') && name.len() > 1 => match name[1..].parse::<u8>().ok()? {
            n @ 1..=12 => Some(0x3A + n - 1),
            n @ 13..=24 => Some(0x68 + n - 13),
            _ => None,
        },
        _ => KEY_NAMES.iter().find(|k| k.1 == name).map(|k| k.0),
    }
}

const KEY_NAMES: &[(u8, &str)] = &[
    (0x28, "enter"),
    (0x29, "esc"),
    (0x2A, "backspace"),
    (0x2B, "tab"),
    (0x2C, "space"),
    (0x2D, "minus"),
    (0x2E, "equal"),
    (0x2F, "lbracket"),
    (0x30, "rbracket"),
    (0x31, "backslash"),
    (0x33, "semicolon"),
    (0x34, "quote"),
    (0x35, "grave"),
    (0x36, "comma"),
    (0x37, "period"),
    (0x38, "slash"),
    (0x39, "capslock"),
    (0x46, "printscreen"),
    (0x47, "scrolllock"),
    (0x48, "pause"),
    (0x49, "insert"),
    (0x4A, "home"),
    (0x4B, "pageup"),
    (0x4C, "delete"),
    (0x4D, "end"),
    (0x4E, "pagedown"),
    (0x4F, "right"),
    (0x50, "left"),
    (0x51, "down"),
    (0x52, "up"),
    (0x53, "numlock"),
    (0x54, "num-slash"),
    (0x55, "num-star"),
    (0x56, "num-minus"),
    (0x57, "num-plus"),
    (0x58, "num-enter"),
    (0x59, "num1"),
    (0x5A, "num2"),
    (0x5B, "num3"),
    (0x5C, "num4"),
    (0x5D, "num5"),
    (0x5E, "num6"),
    (0x5F, "num7"),
    (0x60, "num8"),
    (0x61, "num9"),
    (0x62, "num0"),
    (0x63, "num-period"),
    (0x65, "menu"),
];

pub(crate) fn key_name(usage: u8) -> String {
    match usage {
        0x04..=0x1D => ((b'a' + usage - 0x04) as char).to_string(),
        0x1E..=0x26 => ((b'1' + usage - 0x1E) as char).to_string(),
        0x27 => "0".into(),
        0x3A..=0x45 => format!("f{}", usage - 0x3A + 1),
        0x68..=0x73 => format!("f{}", usage - 0x68 + 13),
        // A modifier sent as a key (Synapse stores a lone Ctrl or Alt this way, #16).
        0xE0..=0xE7 => MODIFIERS[(usage - 0xE0) as usize].1.to_string(),
        _ => KEY_NAMES.iter().find(|k| k.0 == usage).map_or(format!("0x{usage:02x}"), |k| k.1.to_string()),
    }
}

/// `ctrl+shift+a` -> (modifier bits, usage). A modifier on its own (`ctrl`) is sent
/// as that modifier with no key, as Synapse does.
pub(crate) fn parse_combo(s: &str) -> Result<(u8, u8)> {
    let bad = || Error::InvalidArg(format!("unknown key '{s}' (e.g. a, f5, ctrl+c, space, volume keys are media:...)"));
    let mut modifiers = 0;
    let mut usage = None;
    for part in s.split('+').map(str::trim) {
        if let Some(m) = MODIFIERS.iter().find(|m| m.1 == part) {
            modifiers |= m.0;
        } else if usage.is_none() {
            usage = Some(match part.strip_prefix("0x") {
                Some(hex) => u8::from_str_radix(hex, 16).map_err(|_| bad())?,
                None => key_usage(part).ok_or_else(bad)?,
            });
        } else {
            return Err(bad());
        }
    }
    if usage.is_none() && modifiers == 0 {
        return Err(bad());
    }
    Ok((modifiers, usage.unwrap_or(0)))
}

fn combo_name(modifiers: u8, usage: u8) -> String {
    let mut parts: Vec<String> = MODIFIERS.iter().filter(|m| modifiers & m.0 != 0).map(|m| m.1.to_string()).collect();
    if usage != 0 {
        parts.push(key_name(usage));
    }
    parts.join("+")
}

fn mouse_name(b: u8) -> String {
    MOUSE_NAMES.iter().find(|m| m.0 == b).map_or(format!("mouse:0x{b:02x}"), |m| m.1.to_string())
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Action::Disabled => write!(f, "disabled"),
            Action::Mouse(b) => write!(f, "{}", mouse_name(b)),
            Action::DoubleClick => write!(f, "double-click"),
            Action::Key { modifiers, usage } => write!(f, "key:{}", combo_name(modifiers, usage)),
            Action::TurboMouse { button, rate } => write!(f, "turbo:{}@{rate}", mouse_name(button)),
            Action::TurboKey { modifiers, usage, rate } => write!(f, "turbo:key:{}@{rate}", combo_name(modifiers, usage)),
            Action::DpiUp => write!(f, "dpi-up"),
            Action::DpiDown => write!(f, "dpi-down"),
            Action::DpiCycleUp => write!(f, "dpi-cycle-up"),
            Action::DpiCycleDown => write!(f, "dpi-cycle-down"),
            Action::DpiClutch { x, y } if x == y => write!(f, "dpi-clutch:{x}"),
            Action::DpiClutch { x, y } => write!(f, "dpi-clutch:{x}x{y}"),
            Action::ProfileUp => write!(f, "profile-up"),
            Action::ProfileDown => write!(f, "profile-down"),
            Action::ProfileCycleUp => write!(f, "profile-cycle-up"),
            Action::ProfileCycleDown => write!(f, "profile-cycle-down"),
            Action::ProfileSlot(n) => write!(f, "profile:{n}"),
            Action::HyperShift => write!(f, "hypershift"),
            Action::Media(u) => match MEDIA_NAMES.iter().find(|m| m.0 == u) {
                Some(m) => write!(f, "media:{}", m.1),
                None => write!(f, "media:0x{u:04x}"),
            },
            Action::ScrollMode(m) => write!(f, "scroll-mode:{m}"),
            Action::Macro { id, play: MacroPlay::Times(1) } => write!(f, "macro:{id}"),
            Action::Macro { id, play: MacroPlay::Times(n) } => write!(f, "macro:{id}@{n}"),
            Action::Macro { id, play: MacroPlay::WhileHeld } => write!(f, "macro:{id}@hold"),
            Action::Macro { id, play: MacroPlay::Toggle } => write!(f, "macro:{id}@toggle"),
            Action::Raw { function, data } => {
                write!(f, "raw:{function}:")?;
                data.iter().try_for_each(|b| write!(f, "{b:02x}"))
            }
        }
    }
}

impl FromStr for Action {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        let bad = || Error::InvalidArg(format!("unknown action '{s}' (see `secbird button --help`)"));
        let num = |v: &str| v.parse::<u16>().map_err(|_| bad());
        if let Some(m) = MOUSE_NAMES.iter().find(|m| m.1 == s) {
            return Ok(Action::Mouse(m.0));
        }
        Ok(match s.as_str() {
            "disabled" | "off" | "none" => Action::Disabled,
            "double-click" => Action::DoubleClick,
            "dpi-up" => Action::DpiUp,
            "dpi-down" => Action::DpiDown,
            "dpi-cycle-up" => Action::DpiCycleUp,
            "dpi-cycle-down" => Action::DpiCycleDown,
            "profile-up" => Action::ProfileUp,
            "profile-down" => Action::ProfileDown,
            "profile-cycle-up" => Action::ProfileCycleUp,
            "profile-cycle-down" => Action::ProfileCycleDown,
            "hypershift" => Action::HyperShift,
            _ => {
                let (kind, arg) = s.split_once(':').ok_or_else(bad)?;
                match kind {
                    "key" => {
                        let (modifiers, usage) = parse_combo(arg)?;
                        Action::Key { modifiers, usage }
                    }
                    "turbo" => {
                        let (inner, rate) = match arg.rsplit_once('@') {
                            Some((inner, rate)) => (inner, rate.parse::<u8>().map_err(|_| bad())?),
                            None => (arg, DEFAULT_TURBO_RATE),
                        };
                        if !(1..=50).contains(&rate) {
                            return Err(Error::InvalidArg("turbo rate must be 1-50 presses per second".into()));
                        }
                        match inner.parse::<Action>()? {
                            Action::Mouse(button) => Action::TurboMouse { button, rate },
                            Action::Key { modifiers, usage } => Action::TurboKey { modifiers, usage, rate },
                            _ => return Err(Error::InvalidArg("turbo works with a mouse button or key:...".into())),
                        }
                    }
                    "dpi-clutch" => {
                        let (x, y) = match arg.split_once('x') {
                            Some((x, y)) => (num(x)?, num(y)?),
                            None => (num(arg)?, num(arg)?),
                        };
                        if !(100..=30000).contains(&x) || !(100..=30000).contains(&y) {
                            return Err(Error::InvalidArg(format!("clutch DPI {x}x{y} out of range")));
                        }
                        Action::DpiClutch { x, y }
                    }
                    "profile" => Action::ProfileSlot(arg.parse().map_err(|_| bad())?),
                    "media" => match MEDIA_NAMES.iter().find(|m| m.1 == arg) {
                        Some(m) => Action::Media(m.0),
                        None => Action::Media(
                            u16::from_str_radix(arg.strip_prefix("0x").ok_or_else(bad)?, 16).map_err(|_| bad())?,
                        ),
                    },
                    "scroll-mode" => Action::ScrollMode(arg.parse().map_err(|_| bad())?),
                    "macro" => {
                        let (id, play) = arg.split_once('@').unwrap_or((arg, "1"));
                        let play = match play {
                            "hold" => MacroPlay::WhileHeld,
                            "toggle" => MacroPlay::Toggle,
                            n => match n.parse::<u8>() {
                                Ok(n @ 1..=255) => MacroPlay::Times(n),
                                _ => return Err(Error::InvalidArg("macro plays 1-255 times, @hold or @toggle".into())),
                            },
                        };
                        Action::Macro { id: id.parse().map_err(|_| bad())?, play }
                    }
                    "mouse" => Action::Mouse(
                        u8::from_str_radix(arg.strip_prefix("0x").unwrap_or(arg), 16).map_err(|_| bad())?,
                    ),
                    "raw" => {
                        let (function, hex) = arg.split_once(':').ok_or_else(bad)?;
                        let mut data = [0u8; 5];
                        for (i, chunk) in hex.as_bytes().chunks(2).enumerate().take(5) {
                            let pair = std::str::from_utf8(chunk).map_err(|_| bad())?;
                            data[i] = u8::from_str_radix(pair, 16).map_err(|_| bad())?;
                        }
                        Action::Raw { function: function.parse().map_err(|_| bad())?, data }
                    }
                    _ => return Err(bad()),
                }
            }
        })
    }
}

impl Device {
    fn read_input(&self, slot: u8, input: Input, layer: Layer) -> Result<Action> {
        let (command, size, id) = input.command();
        let r = self.get(command, size, &[slot, id, layer as u8])?;
        let a = r.args();
        Ok(Action::decode(a[3], [a[5], a[6], a[7], a[8], a[9]]))
    }

    /// What a button or key does right now (the live slot).
    pub fn input(&self, input: Input, layer: Layer) -> Result<Action> {
        self.read_input(cmd::BUTTON_LIVE_SLOT, input, layer)
    }

    /// What a button or key does in the active saved slot: Synapse's or the factory setting.
    pub fn saved_input(&self, input: Input, layer: Layer) -> Result<Action> {
        let slot = self.get(cmd::ACTIVE_SLOT, 1, &[]).map(|r| r.args()[0]).unwrap_or(1).max(1);
        self.read_input(slot, input, layer)
    }

    /// Set a button or key in the live slot. Returns whether anything was sent: an
    /// unchanged one is left alone.
    pub fn set_input(&self, input: Input, layer: Layer, action: Action) -> Result<bool> {
        if self.input(input, layer).ok() == Some(action) {
            return Ok(false);
        }
        let (command, size, id) = input.command();
        let (function, data_size, data) = action.encode();
        let mut args = vec![cmd::BUTTON_LIVE_SLOT, id, layer as u8, function, data_size];
        args.extend_from_slice(&data);
        self.transact(command, size, &args)?;
        Ok(true)
    }

    /// What a button or key does in on-board slot `slot` (1-5).
    pub fn slot_input(&self, slot: u8, input: Input, layer: Layer) -> Result<Action> {
        self.read_input(slot, input, layer)
    }

    /// Write a button or key into on-board slot `slot` (1-5), which the device keeps
    /// without any software ("Save to mouse"). A flash write: only on request, never on a
    /// profile switch.
    pub fn set_slot_input(&self, slot: u8, input: Input, layer: Layer, action: Action) -> Result<()> {
        if !(1..=5).contains(&slot) {
            return Err(Error::InvalidArg(format!("on-board slot {slot}: use 1 to 5")));
        }
        if self.slot_input(slot, input, layer).ok() == Some(action) {
            return Ok(());
        }
        let (command, size, id) = input.command();
        let (function, data_size, data) = action.encode();
        let mut args = vec![slot, id, layer as u8, function, data_size];
        args.extend_from_slice(&data);
        self.transact_slow(command, size, &args)?;
        Ok(())
    }

    /// Put a button or key back to its saved-slot setting.
    pub fn reset_input(&self, input: Input, layer: Layer) -> Result<bool> {
        let saved = self.saved_input(input, layer)?;
        self.set_input(input, layer, saved)
    }

    /// What a button does right now (the live slot).
    pub fn button(&self, id: u8, layer: Layer) -> Result<Action> {
        self.input(Input::Button(id), layer)
    }

    /// What a button does in the active saved slot: Synapse's or the factory setting.
    pub fn saved_button(&self, id: u8, layer: Layer) -> Result<Action> {
        self.saved_input(Input::Button(id), layer)
    }

    /// See [`Device::set_input`].
    pub fn set_button(&self, id: u8, layer: Layer, action: Action) -> Result<bool> {
        self.set_input(Input::Button(id), layer, action)
    }

    /// What a button does in on-board slot `slot` (1-5).
    pub fn slot_button(&self, slot: u8, id: u8, layer: Layer) -> Result<Action> {
        self.slot_input(slot, Input::Button(id), layer)
    }

    /// See [`Device::set_slot_input`].
    pub fn set_slot_button(&self, slot: u8, id: u8, layer: Layer, action: Action) -> Result<()> {
        self.set_slot_input(slot, Input::Button(id), layer, action)
    }

    /// Put a button back to its saved-slot setting.
    pub fn reset_button(&self, id: u8, layer: Layer) -> Result<bool> {
        self.reset_input(Input::Button(id), layer)
    }

    /// Keys this keyboard lets us remap, with their current (live, normal-layer) action:
    /// every key in [`KEYS`] that answers a read. Empty for anything that isn't a keyboard,
    /// keypad or laptop, or that doesn't answer key reads (Synapse remaps keyboards in
    /// software, so many may not). Asks about each key; takes a moment.
    pub fn keys(&self) -> Vec<(u8, Action)> {
        use crate::Kind;
        if !matches!(self.spec.as_ref().map(|s| s.kind), Some(Kind::Keyboard | Kind::Keypad | Kind::Laptop)) {
            return Vec::new();
        }
        // One key that every layout has: if A doesn't answer, nothing will.
        if self.input(Input::Key(31), Layer::Normal).is_err() {
            return Vec::new();
        }
        KEYS.iter().filter_map(|k| self.input(Input::Key(k.0), Layer::Normal).ok().map(|a| (k.0, a))).collect()
    }

    /// Button ids the mouse says it has (Synapse's "Get Button ID list"), in [`BUTTONS`]
    /// order with any ids we have no name for after them. `None` if it doesn't answer.
    /// Verified on a Basilisk V3: 13 ids, exactly its buttons.
    pub fn button_ids(&self) -> Option<Vec<u8>> {
        let reply = self.get(cmd::BUTTON_IDS, cmd::BUTTON_IDS_SIZE, &[]).ok()?;
        let args = reply.args();
        let listed = args.get(1..1 + *args.first()? as usize)?;
        let listed: Vec<u8> = listed.iter().copied().filter(|&id| id != 0).collect();
        if listed.is_empty() {
            return None;
        }
        let mut ids: Vec<u8> = BUTTONS.iter().map(|b| b.0).filter(|id| listed.contains(id)).collect();
        ids.extend(listed.iter().filter(|id| !BUTTONS.iter().any(|b| b.0 == **id)));
        Some(ids)
    }

    /// Buttons this device has, with their current (live, normal-layer) action. Empty for
    /// devices without on-board button assignments. Uses the mouse's own button list, or
    /// asks about each known button id when it has none (a mouse behind a receiver may
    /// answer for buttons it doesn't have). Takes a moment; call it when the buttons are
    /// about to be shown.
    pub fn buttons(&self) -> Vec<(u8, Action)> {
        self.button_list().0
    }

    /// [`Device::buttons`], and the ids the mouse lists that don't answer a read: buttons it
    /// has but won't let software read or change. Only from the mouse's own list; empty
    /// when it has none.
    pub fn button_list(&self) -> (Vec<(u8, Action)>, Vec<u8>) {
        if !matches!(self.spec.as_ref().map(|s| s.kind), Some(crate::Kind::Mouse)) {
            return (Vec::new(), Vec::new());
        }
        let listed = self.button_ids();
        let from_list = listed.is_some();
        let ids = listed.unwrap_or_else(|| {
            // Without the mouse's list, only a Naga (or a docked mouse we can't name) gets
            // the side-plate buttons.
            let name = self.spec.as_ref().map_or("", |s| s.name.as_str());
            let plates = name.contains("Naga") || name.starts_with("Mouse on");
            BUTTONS.iter().map(|b| b.0).filter(|id| plates || !(64..=86).contains(id)).collect()
        });
        let (mut found, mut unread) = (Vec::new(), Vec::new());
        for id in ids {
            match self.button(id, Layer::Normal) {
                Ok(a) => found.push((id, a)),
                Err(_) if from_list => unread.push(id),
                Err(_) => {}
            }
        }
        (found, unread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_round_trip_through_text_and_bytes() {
        for text in [
            "disabled",
            "left",
            "forward",
            "scroll-left",
            "double-click",
            "key:a",
            "key:ctrl+shift+f13",
            "key:ctrl",
            "key:space",
            "key:num1",
            "key:ctrl+num-plus",
            "key:menu",
            "turbo:left@15",
            "turbo:key:e@10",
            "dpi-up",
            "dpi-cycle-down",
            "dpi-clutch:400",
            "dpi-clutch:400x800",
            "profile-cycle-up",
            "profile:3",
            "hypershift",
            "media:volume-up",
            "scroll-mode:1",
            "macro:7",
            "macro:12@3",
            "macro:300@hold",
            "macro:5@toggle",
        ] {
            let a: Action = text.parse().unwrap();
            assert_eq!(a.to_string(), text, "text round trip");
            let (function, _, data) = a.encode();
            assert_eq!(Action::decode(function, data), a, "byte round trip for {text}");
        }
    }

    #[test]
    fn encodings_match_what_the_basilisk_v3_reports() {
        // Read back from real hardware (docs/research-button-remapping.md).
        assert_eq!(Action::decode(6, [5, 0x01, 0x90, 0x01, 0x90]), Action::DpiClutch { x: 400, y: 400 });
        assert_eq!(Action::decode(14, [0x68, 0, 0x14, 0, 0]), Action::TurboMouse { button: 0x68, rate: 50 });
        assert_eq!(Action::decode(7, [4, 0, 0, 0, 0]), Action::ProfileCycleUp);
        // Written to real hardware: side button typed "a".
        assert_eq!("key:a".parse::<Action>().unwrap().encode(), (2, 2, [0, 4, 0, 0, 0]));
        assert_eq!("forward".parse::<Action>().unwrap().encode(), (1, 1, [5, 0, 0, 0, 0]));
        // Synapse stores a lone Alt as the key E2, not as a modifier bit (#16).
        assert_eq!(Action::decode(2, [0, 0xE2, 0, 0, 0]).to_string(), "key:alt");
        assert_eq!(button_label(128), "Extra side button");
        assert_eq!(button_id("top-extra"), Some(128));
    }

    #[test]
    fn bad_input_is_rejected() {
        for text in ["", "key:", "key:a+b", "turbo:dpi-up", "turbo:left@0", "dpi-clutch:5", "jump"] {
            assert!(text.parse::<Action>().is_err(), "{text}");
        }
    }

    #[test]
    fn setting_keys() {
        assert_eq!(setting_key("sniper", Layer::HyperShift), "sniper/hypershift");
        assert_eq!(parse_setting_key("sniper/hypershift").unwrap(), (Input::Button(15), Layer::HyperShift));
        assert_eq!(parse_setting_key("side-forward").unwrap(), (Input::Button(5), Layer::Normal));
        assert!(parse_setting_key("elbow").is_err());
    }
}
