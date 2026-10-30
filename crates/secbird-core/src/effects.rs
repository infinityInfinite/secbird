//! Built-in lighting effects the device runs by itself (`0F:02`), as Synapse sends them.
//!
//! Packet: `[store, led, effect, flags, rate, colour count, r, g, b, ...]`. Flags and rate
//! per effect come from Synapse's effect-settings converter (`main.js`, module 76665).

use crate::commands as cmd;
use crate::db::Lighting;
use crate::device::Device;
use crate::error::{Error, Result};
use crate::features::Rgb;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Fades in and out: one or two colours, or random colours when empty.
    Breathing(Vec<Rgb>),
    /// Cycles through all colours.
    Spectrum,
    /// Colours flow across the device; `speed` 1 (slow) to 5 (fast).
    Wave { reverse: bool, speed: u8 },
    /// Lights up when used, then fades; `duration` 1 (short) to 3 (long).
    Reactive { color: Rgb, duration: u8 },
    /// Twinkling: one or two colours, or random when empty; `duration` 1 to 3.
    Starlight { colors: Vec<Rgb>, duration: u8 },
    /// The wireless mouse's battery as a colour, green to red. Docks refuse Synapse's
    /// Battery Level effect (14, #39), so it is a static colour the tray keeps up to date.
    Battery,
}

// Effect ids (Synapse's `RP` enum).
const BREATHING: u8 = 2;
const SPECTRUM: u8 = 3;
const WAVE: u8 = 4;
const REACTIVE: u8 = 5;
const STARLIGHT: u8 = 7;

/// Wave speed 1-5 to Synapse's rate byte (lower is faster).
const WAVE_RATES: [u8; 5] = [75, 55, 40, 25, 15];

impl Effect {
    /// `(effect id, flags, rate, colours)`.
    fn params(&self) -> (u8, u8, u8, Vec<Rgb>) {
        match self {
            Effect::Breathing(colors) => (BREATHING, colors.len() as u8, 0, colors.clone()),
            Effect::Spectrum => (SPECTRUM, 0, 0, Vec::new()),
            Effect::Wave { reverse, speed } => {
                (WAVE, if *reverse { 2 } else { 1 }, WAVE_RATES[(*speed).clamp(1, 5) as usize - 1], Vec::new())
            }
            Effect::Reactive { color, duration } => (REACTIVE, 0, *duration, vec![*color]),
            Effect::Starlight { colors, duration } => (STARLIGHT, colors.len() as u8, *duration, colors.clone()),
            // Sent as a static colour instead (see Device::set_effect).
            Effect::Battery => (cmd::EFFECT_STATIC, 0, 0, Vec::new()),
        }
    }

    /// Colour for a battery level, as the dock's charging status in Synapse: full, high,
    /// mid, low, critical.
    pub fn battery_color(percent: u8) -> Rgb {
        match percent {
            80.. => Rgb(0x00, 0xFF, 0x00),
            50..=79 => Rgb(0x80, 0xFF, 0x00),
            25..=49 => Rgb(0xFF, 0x96, 0x00),
            11..=24 => Rgb(0xFF, 0x28, 0x00),
            _ => Rgb(0xFF, 0x00, 0x00),
        }
    }

    /// The effect's name, as in its text form.
    pub fn kind(&self) -> &'static str {
        match self {
            Effect::Breathing(_) => "breathing",
            Effect::Spectrum => "spectrum",
            Effect::Wave { .. } => "wave",
            Effect::Reactive { .. } => "reactive",
            Effect::Starlight { .. } => "starlight",
            Effect::Battery => "battery",
        }
    }

    fn check(&self) -> Result<()> {
        let bad = |what: &str| Err(Error::InvalidArg(what.into()));
        match self {
            Effect::Breathing(c) | Effect::Starlight { colors: c, .. } if c.len() > 2 => bad("at most two colours"),
            Effect::Wave { speed, .. } if !(1..=5).contains(speed) => bad("wave speed is 1-5"),
            Effect::Reactive { duration, .. } | Effect::Starlight { duration, .. } if !(1..=3).contains(duration) => {
                bad("duration is 1 (short) to 3 (long)")
            }
            _ => Ok(()),
        }
    }
}

impl Device {
    /// Whether this model can run the requested effect. Some mice acknowledge
    /// unsupported effect writes but display nothing or a static color.
    pub fn supports_effect(&self, effect: &Effect) -> bool {
        self.spec.as_ref().is_none_or(|s| {
            !s.unsupported_effects.iter().any(|kind| kind == effect.kind())
        })
    }

    /// Run a built-in effect on one lighting zone.
    pub fn set_effect(&self, led: u8, effect: &Effect) -> Result<()> {
        effect.check()?;
        if !self.supports_effect(effect) {
            return Err(Error::Unsupported(format!("the {} effect on {}", effect.kind(), self.name())));
        }
        if *effect == Effect::Battery {
            let percent = self.battery()?.ok_or_else(|| Error::Unsupported(format!("{} has no battery", self.name())))?;
            return self.set_static_color(led, Effect::battery_color(percent));
        }
        if self.lighting_for(led) == Lighting::Standard && self.is_grid_backlight(led) {
            return self.set_grid_effect(effect);
        }
        if self.lighting_for(led) != Lighting::Extended {
            return Err(Error::Unsupported(format!("lighting effects on {} (older lighting commands)", self.name())));
        }
        let (id, flags, rate, colors) = effect.params();
        let mut args = vec![self.store_for(led), led, id, flags, rate, colors.len() as u8];
        args.extend(colors.iter().flat_map(|c| [c.0, c.1, c.2]));
        match self.transact_led(led, cmd::MATRIX_EFFECT, args.len() as u8, &args) {
            // Some firmware runs only a few effects itself (the Basilisk V3: spectrum and wave);
            // Synapse animates the rest from the PC.
            Err(Error::Device { status: crate::proto::Status::NotSupported, .. }) => Err(Error::Unsupported(format!(
                "{} can't run {} by itself",
                self.name(),
                effect.kind()
            ))),
            other => other.map(|_| ()),
        }
    }
}

impl Device {
    /// An effect on an older keyboard's whole grid with `03:0A` (the BlackWidow Chroma V2,
    /// #49). Bytes and sizes as openrazer sends them; wave speed has no byte here.
    fn set_grid_effect(&self, effect: &Effect) -> Result<()> {
        let rgb = |c: &Rgb| [c.0, c.1, c.2];
        let (size, args): (u8, Vec<u8>) = match effect {
            Effect::Spectrum => (1, vec![0x04]),
            Effect::Wave { reverse, .. } => (2, vec![0x01, if *reverse { 0x02 } else { 0x01 }]),
            Effect::Reactive { color, duration } => (5, [&[0x02, *duration][..], &rgb(color)].concat()),
            Effect::Breathing(colors) => {
                let kind = match colors.len() {
                    0 => 0x03,
                    n => n as u8,
                };
                (8, [vec![0x03, kind], colors.iter().flat_map(rgb).collect()].concat())
            }
            Effect::Starlight { colors, duration } => {
                let kind = match colors.len() {
                    0 => 0x03,
                    n => n as u8,
                };
                (1, [vec![0x19, kind, *duration], colors.iter().flat_map(rgb).collect()].concat())
            }
            Effect::Battery => unreachable!("sent as a static colour"),
        };
        self.transact(cmd::GRID_EFFECT, size, &args)?;
        Ok(())
    }
}

fn colors_text(colors: &[Rgb]) -> String {
    if colors.is_empty() {
        "random".into()
    } else {
        colors.iter().map(Rgb::to_string).collect::<Vec<_>>().join(",")
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Effect::Breathing(c) => write!(f, "breathing:{}", colors_text(c)),
            Effect::Spectrum => write!(f, "spectrum"),
            Effect::Wave { reverse, speed } => write!(f, "wave:{}@{speed}", if *reverse { "left" } else { "right" }),
            Effect::Reactive { color, duration } => write!(f, "reactive:{color}@{duration}"),
            Effect::Starlight { colors, duration } => write!(f, "starlight:{}@{duration}", colors_text(colors)),
            Effect::Battery => write!(f, "battery"),
        }
    }
}

impl FromStr for Effect {
    type Err = Error;

    /// `spectrum`, `breathing:FF0000,0000FF`, `breathing:random`, `wave:right@3`,
    /// `reactive:00FF00@2`, `starlight:random@2`. Speeds and durations are optional.
    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        let bad = || {
            Error::InvalidArg(format!(
                "unknown effect '{s}' (spectrum, breathing:RRGGBB[,RRGGBB]|random, wave:left|right[@1-5], \
                 reactive:RRGGBB[@1-3], starlight:RRGGBB[,RRGGBB]|random[@1-3], battery)"
            ))
        };
        let (kind, rest) = s.split_once(':').unwrap_or((&s, ""));
        let (arg, level) = match rest.split_once('@') {
            Some((a, n)) => (a, Some(n.parse::<u8>().map_err(|_| bad())?)),
            None => (rest, None),
        };
        let colors = |a: &str| -> Result<Vec<Rgb>> {
            match a {
                "random" | "" => Ok(Vec::new()),
                list => list.split(',').map(|c| c.parse()).collect(),
            }
        };
        let effect = match kind {
            "spectrum" => Effect::Spectrum,
            "battery" => Effect::Battery,
            "breathing" => Effect::Breathing(colors(arg)?),
            "wave" => Effect::Wave {
                reverse: match arg {
                    "left" => true,
                    "right" | "" => false,
                    _ => return Err(bad()),
                },
                speed: level.unwrap_or(3),
            },
            "reactive" => Effect::Reactive { color: arg.parse()?, duration: level.unwrap_or(2) },
            "starlight" => Effect::Starlight { colors: colors(arg)?, duration: level.unwrap_or(2) },
            _ => return Err(bad()),
        };
        effect.check()?;
        Ok(effect)
    }
}

// Profiles store effects as their text, which stays readable in profiles.json.
impl Serialize for Effect {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Effect {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_unsupported_effects_never_reach_hardware() {
        let sim = crate::sim::Sim::mouse();
        let mut spec = crate::sim::Sim::spec_for(crate::sim::MOUSE_PID).unwrap();
        spec.unsupported_effects = vec!["reactive".into(), "starlight".into()];
        let dev = sim.open(crate::sim::MOUSE_PID, Some(spec)).unwrap();
        let before = sim.state().log.len();
        for unsupported in [
            Effect::Reactive { color: Rgb(0xFF, 0, 0), duration: 2 },
            Effect::Starlight { colors: vec![], duration: 2 },
        ] {
            assert!(!dev.supports_effect(&unsupported));
            assert!(matches!(dev.set_effect(10, &unsupported), Err(Error::Unsupported(_))));
            assert_eq!(sim.state().log.len(), before, "unsupported effect sent HID traffic");
        }
        assert!(dev.supports_effect(&Effect::Spectrum));
        dev.set_effect(10, &Effect::Spectrum).unwrap();
        assert!(sim.state().log.len() > before, "supported effects must still be written");
    }

    #[test]
    fn effects_round_trip_through_text() {
        for text in [
            "spectrum",
            "battery",
            "breathing:FF0000",
            "breathing:FF0000,0000FF",
            "breathing:random",
            "wave:right@3",
            "wave:left@5",
            "reactive:00FF00@1",
            "starlight:random@3",
            "starlight:FFFFFF,FF00FF@2",
        ] {
            let e: Effect = text.parse().unwrap();
            assert_eq!(e.to_string(), text);
        }
        assert_eq!("wave".parse::<Effect>().unwrap(), Effect::Wave { reverse: false, speed: 3 });
        for bad in ["sparkle", "wave:up", "wave:left@9", "reactive:00FF00@4", "breathing:FF0000,00FF00,0000FF", "reactive"] {
            assert!(bad.parse::<Effect>().is_err(), "{bad}");
        }
    }

    #[test]
    fn params_match_synapse() {
        let red = Rgb(0xFF, 0, 0);
        assert_eq!(Effect::Breathing(vec![red]).params(), (2, 1, 0, vec![red]));
        assert_eq!(Effect::Breathing(vec![]).params(), (2, 0, 0, vec![]));
        assert_eq!(Effect::Spectrum.params(), (3, 0, 0, vec![]));
        assert_eq!(Effect::Wave { reverse: false, speed: 1 }.params(), (4, 1, 75, vec![]));
        assert_eq!(Effect::Wave { reverse: true, speed: 5 }.params(), (4, 2, 15, vec![]));
        assert_eq!(Effect::Reactive { color: red, duration: 2 }.params(), (5, 0, 2, vec![red]));
        assert_eq!(Effect::Starlight { colors: vec![], duration: 3 }.params(), (7, 0, 3, vec![]));
    }
}
