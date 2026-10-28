//! Scroll wheel settings on mice with a HyperScroll wheel: the wheel mode (tactile,
//! free-spin, precision tactile), which modes the wheel-mode button cycles through, scroll
//! acceleration and Smart-Reel. Synapse's Scroll Wheel page.

use crate::commands as cmd;
use crate::device::Device;
use crate::error::{Error, Result};
use crate::proto::Command;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// How the wheel turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScrollMode {
    /// Notched steps.
    Tactile,
    FreeSpin,
    /// Finer notched steps (Synapse's "Precision Tactile", "Micro Tactile" in its code).
    PrecisionTactile,
}

impl ScrollMode {
    pub const ALL: [ScrollMode; 3] = [ScrollMode::Tactile, ScrollMode::FreeSpin, ScrollMode::PrecisionTactile];

    fn code(self) -> u8 {
        self as u8
    }

    fn from_code(code: u8) -> Option<ScrollMode> {
        ScrollMode::ALL.into_iter().find(|m| m.code() == code)
    }

    fn bit(self) -> u8 {
        1 << self.code()
    }
}

impl fmt::Display for ScrollMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ScrollMode::Tactile => "tactile",
            ScrollMode::FreeSpin => "free-spin",
            ScrollMode::PrecisionTactile => "precision-tactile",
        })
    }
}

impl FromStr for ScrollMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "precision" => Ok(ScrollMode::PrecisionTactile),
            "free" => Ok(ScrollMode::FreeSpin),
            _ => ScrollMode::ALL
                .into_iter()
                .find(|m| m.to_string() == s)
                .ok_or_else(|| Error::InvalidArg(format!("'{s}': use tactile, free-spin or precision-tactile"))),
        }
    }
}

/// Scroll acceleration or Smart-Reel: off, or on at a level 0-4, the five stops of
/// Synapse's slider (low, medium and high are 0, 2 and 4). The Naga V3 Pro refuses 5 and up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assist {
    Off,
    On(u8),
}

impl Assist {
    pub const MAX_LEVEL: u8 = 4;
    const NAMES: [(&'static str, u8); 3] = [("low", 0), ("medium", 2), ("high", 4)];

    fn level(self) -> Option<u8> {
        match self {
            Assist::Off => None,
            Assist::On(level) => Some(level),
        }
    }

    /// From the on/off and level replies.
    fn from_reply(on: u8, level: u8) -> Assist {
        match on {
            0 => Assist::Off,
            _ => Assist::On(level.min(Self::MAX_LEVEL)),
        }
    }
}

impl fmt::Display for Assist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Assist::Off => f.write_str("off"),
            Assist::On(level) => match Self::NAMES.iter().find(|n| n.1 == level) {
                Some((name, _)) => f.write_str(name),
                None => write!(f, "{level}"),
            },
        }
    }
}

impl FromStr for Assist {
    type Err = Error;

    /// `off`, `low`, `medium`, `high`, or a level `0`-`4`.
    fn from_str(s: &str) -> Result<Self> {
        if s == "off" {
            return Ok(Assist::Off);
        }
        let level = match Self::NAMES.iter().find(|n| n.0 == s) {
            Some(&(_, level)) => Some(level),
            None => s.parse::<u8>().ok().filter(|l| *l <= Self::MAX_LEVEL),
        };
        level.map(Assist::On).ok_or_else(|| Error::InvalidArg(format!("'{s}': use off, low, medium, high or a level 0-4")))
    }
}

impl Serialize for Assist {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Assist {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

/// What the wheel is set to now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScrollWheel {
    pub mode: ScrollMode,
    /// Whether the wheel has precision tactile (the newer mode command).
    pub precision: bool,
    /// Modes the wheel-mode button skips. Only on wheels with precision tactile.
    pub disabled: Vec<ScrollMode>,
    pub acceleration: Assist,
    pub smart_reel: Assist,
}

/// Modes as text: `none`, or a comma-separated list like `free-spin,precision-tactile`.
pub fn parse_modes(s: &str) -> Result<Vec<ScrollMode>> {
    if s == "none" || s.is_empty() {
        return Ok(Vec::new());
    }
    let mut modes = Vec::new();
    for m in s.split(',') {
        let m: ScrollMode = m.trim().parse()?;
        if !modes.contains(&m) {
            modes.push(m);
        }
    }
    Ok(modes)
}

pub fn modes_text(modes: &[ScrollMode]) -> String {
    if modes.is_empty() {
        return "none".into();
    }
    modes.iter().map(ScrollMode::to_string).collect::<Vec<_>>().join(",")
}

impl Device {
    fn scroll_get(&self, command: Command) -> Result<u8> {
        let r = self.get_echo(command, cmd::SCROLL_SIZE, &[cmd::NOSTORE], 1)?;
        Ok(r.args()[1])
    }

    /// Live (store 0) first; the active saved slot too, best effort, so the setting stays
    /// after the mouse rebuilds the live values at power-on (as for auto sleep, #37).
    fn scroll_set(&self, command: Command, value: u8) -> Result<()> {
        self.transact_echo(command, cmd::SCROLL_SIZE, &[cmd::NOSTORE, value], 2)?;
        let _ = self.transact_echo(command, cmd::SCROLL_SIZE, &[self.saved_slot(), value], 2);
        Ok(())
    }

    /// The scroll wheel's settings. `None` for a device without a mode-switching wheel.
    pub fn scroll_wheel(&self) -> Result<Option<ScrollWheel>> {
        if !matches!(self.spec.as_ref().map(|s| s.kind), Some(crate::Kind::Mouse)) {
            return Ok(None);
        }
        let (mode, precision) = match self.scroll_get(cmd::SCROLL_MODE.get()) {
            Ok(code) => (code, true),
            Err(_) => match self.scroll_get(cmd::SCROLL_MODE_OLD.get()) {
                Ok(code) => (code, false),
                Err(_) => return Ok(None),
            },
        };
        let mode = ScrollMode::from_code(mode).unwrap_or(ScrollMode::Tactile);
        let disabled = match precision {
            true => match self.scroll_get(cmd::SCROLL_MODES_ENABLED.get()) {
                Ok(bits) => ScrollMode::ALL.into_iter().filter(|m| bits & m.bit() == 0).collect(),
                Err(_) => Vec::new(),
            },
            false => Vec::new(),
        };
        let assist = |on: Command, level: Command| {
            let on = self.scroll_get(on.get()).unwrap_or(0);
            let level = self.scroll_get(level.get()).unwrap_or(2);
            Assist::from_reply(on, level)
        };
        let acceleration = assist(cmd::SCROLL_ACCELERATION, cmd::SCROLL_ACCELERATION_LEVEL);
        let smart_reel = assist(cmd::SMART_REEL, cmd::SMART_REEL_LEVEL);
        Ok(Some(ScrollWheel { mode, precision, disabled, acceleration, smart_reel }))
    }

    fn require_scroll_wheel(&self) -> Result<ScrollWheel> {
        self.scroll_wheel()?.ok_or_else(|| Error::Unsupported(format!("{} has no scroll wheel modes", self.name())))
    }

    /// Switch the wheel mode. A mode the wheel-mode button skips is put back in its cycle,
    /// as Synapse does.
    pub fn set_scroll_mode(&self, mode: ScrollMode) -> Result<()> {
        let wheel = self.require_scroll_wheel()?;
        if !wheel.precision {
            if mode == ScrollMode::PrecisionTactile {
                return Err(Error::Unsupported("this wheel has tactile and free-spin only".into()));
            }
            return self.scroll_set(cmd::SCROLL_MODE_OLD, mode.code());
        }
        if wheel.disabled.contains(&mode) {
            let disabled: Vec<ScrollMode> = wheel.disabled.into_iter().filter(|m| *m != mode).collect();
            self.write_scroll_disabled(&disabled)?;
        }
        self.scroll_set(cmd::SCROLL_MODE, mode.code())
    }

    /// Which modes the wheel-mode button skips: at most two, and not the mode in use.
    pub fn set_scroll_disabled(&self, disabled: &[ScrollMode]) -> Result<()> {
        let wheel = self.require_scroll_wheel()?;
        if !wheel.precision {
            return Err(Error::Unsupported("this wheel's modes can't be turned off".into()));
        }
        if disabled.len() > 2 {
            return Err(Error::InvalidArg("at most two modes can be turned off".into()));
        }
        if disabled.contains(&wheel.mode) {
            return Err(Error::InvalidArg(format!("the wheel is in {} now: switch to another mode first", wheel.mode)));
        }
        self.write_scroll_disabled(disabled)
    }

    fn write_scroll_disabled(&self, disabled: &[ScrollMode]) -> Result<()> {
        let bits = ScrollMode::ALL.into_iter().filter(|m| !disabled.contains(m)).fold(0, |b, m| b | m.bit());
        self.scroll_set(cmd::SCROLL_MODES_ENABLED, bits)
    }

    /// Scroll acceleration in free-spin: off, or on at a level.
    pub fn set_scroll_acceleration(&self, a: Assist) -> Result<()> {
        self.set_assist(cmd::SCROLL_ACCELERATION, cmd::SCROLL_ACCELERATION_LEVEL, a)
    }

    /// Smart-Reel: free-spin while the wheel is flicked, back to notched when it stops.
    pub fn set_smart_reel(&self, a: Assist) -> Result<()> {
        self.set_assist(cmd::SMART_REEL, cmd::SMART_REEL_LEVEL, a)
    }

    fn set_assist(&self, on: Command, level: Command, a: Assist) -> Result<()> {
        if let Some(level_value) = a.level() {
            self.scroll_set(level, level_value)?;
        }
        self.scroll_set(on, a.level().is_some() as u8)
    }
}

/// A profile's scroll wheel settings; unset ones are left as the mouse has them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrollSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ScrollMode>,
    /// Modes the wheel-mode button skips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<Vec<ScrollMode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceleration: Option<Assist>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_reel: Option<Assist>,
}

impl ScrollSettings {
    pub fn is_empty(&self) -> bool {
        *self == ScrollSettings::default()
    }

    /// `wheel` as read back, with these settings, just sent, on top. The Naga V3 Pro
    /// answers the old mode for about a second after a switch, while the wheel moves.
    pub fn overlay(&self, mut wheel: ScrollWheel) -> ScrollWheel {
        if let Some(disabled) = &self.disabled {
            wheel.disabled = disabled.clone();
        }
        if let Some(mode) = self.mode {
            wheel.mode = mode;
            wheel.disabled.retain(|m| *m != mode);
        }
        wheel.acceleration = self.acceleration.unwrap_or(wheel.acceleration);
        wheel.smart_reel = self.smart_reel.unwrap_or(wheel.smart_reel);
        wheel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips() {
        for m in ScrollMode::ALL {
            assert_eq!(m.to_string().parse::<ScrollMode>().unwrap(), m);
        }
        for text in ["off", "low", "1", "medium", "3", "high"] {
            assert_eq!(text.parse::<Assist>().unwrap().to_string(), text);
        }
        assert_eq!("4".parse::<Assist>().unwrap(), Assist::On(4));
        assert!("5".parse::<Assist>().is_err());
        assert_eq!(serde_json::to_string(&Assist::On(2)).unwrap(), "\"medium\"");
        assert_eq!(serde_json::from_str::<Assist>("\"3\"").unwrap(), Assist::On(3));
        assert_eq!("precision".parse::<ScrollMode>().unwrap(), ScrollMode::PrecisionTactile);
        assert_eq!(parse_modes("free-spin, tactile").unwrap(), [ScrollMode::FreeSpin, ScrollMode::Tactile]);
        assert_eq!(modes_text(&parse_modes("none").unwrap()), "none");
        assert!(parse_modes("wobbly").is_err());
    }

    #[test]
    fn what_was_sent_wins_over_a_late_read_back() {
        // Read right after switching to free-spin: still tactile, free-spin skipped.
        let read = ScrollWheel {
            mode: ScrollMode::Tactile,
            precision: true,
            disabled: vec![ScrollMode::FreeSpin],
            acceleration: Assist::Off,
            smart_reel: Assist::On(0),
        };
        let sent = ScrollSettings { mode: Some(ScrollMode::FreeSpin), ..Default::default() };
        let shown = sent.overlay(read.clone());
        assert_eq!(shown.mode, ScrollMode::FreeSpin);
        assert!(shown.disabled.is_empty(), "the mode switched to is back in the cycle");
        assert_eq!(shown.smart_reel, Assist::On(0), "settings not sent stay as read");
        assert_eq!(ScrollSettings::default().overlay(read.clone()), read);
    }

    #[test]
    fn replies_match_what_the_naga_v3_pro_showed() {
        // Synapse: acceleration and Smart-Reel off, both sliders at medium; reads 00 / 02.
        assert_eq!(Assist::from_reply(0, 2), Assist::Off);
        assert_eq!(Assist::from_reply(1, 2).to_string(), "medium");
        assert_eq!(Assist::On(2).level(), Some(2));
        assert_eq!(ScrollMode::from_code(2), Some(ScrollMode::PrecisionTactile));
        // All three modes in the cycle reads 7.
        assert_eq!(ScrollMode::ALL.iter().fold(0, |b, m| b | m.bit()), 7);
    }
}
