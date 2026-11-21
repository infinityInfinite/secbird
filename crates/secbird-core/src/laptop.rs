//! Blade laptops: performance mode, CPU and GPU boost, battery charge limit, and the fan
//! speeds and temperatures the embedded controller reports.
//!
//! Command bytes from razer-ctl and blade-control (both MIT), checked against a Blade 18
//! (2023) report in #23: its controller answered `0D:82` with `00 01 04` (zone 1, Custom).
//! Only the controller's own presets are written, never a fan speed. Every write is read
//! back, and a laptop that doesn't keep a setting gets an error, not a silent success.

use crate::proto::Command;
use crate::{Device, Error, Kind, Result};

/// Performance mode, per zone: `[1, zone, mode, fan]` (fan 0 = automatic).
pub const PERFORMANCE: Command = Command::new(0x0D, 0x02);
const PERFORMANCE_SIZE: u8 = 4;
/// CPU or GPU boost in Custom mode: `[1, cluster, level]`.
pub const BOOST: Command = Command::new(0x0D, 0x07);
const BOOST_SIZE: u8 = 3;
/// A fan's speed, read only: `[0, zone, rpm / 100]`.
pub const FAN_SPEED: Command = Command::new(0x0D, 0x08);
/// Temperatures, read only: `[2, cpu, gpu]` in whole degrees Celsius.
pub const TEMPERATURES: Command = Command::new(0x0D, 0x05);
/// Battery charge limit: `[0x80 | percent]`, or `0x50` for none.
pub const CHARGE_LIMIT: Command = Command::new(0x07, 0x12);

/// The controller keeps the mode per zone and expects both written: CPU, then GPU.
const ZONES: [u8; 2] = [1, 2];
const FAN_AUTO: u8 = 0;
const CHARGE_LIMIT_ON: u8 = 0x80;
const NO_CHARGE_LIMIT: u8 = 0x50;
/// The lowest charge limit offered (blade-control's floor).
pub const CHARGE_LIMIT_MIN: u8 = 60;

/// What the controller is set to. Battery Saver and Battery Balanced (newer Blades, on
/// battery) read as Silent and Balanced, and the 2020 Blades' Gaming as Performance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerMode {
    Silent,
    Balanced,
    Performance,
    /// CPU and GPU boost chosen by hand.
    Custom,
    /// A mode set elsewhere (Synapse, a key on the laptop) that has no name here.
    Other(u8),
}

impl PowerMode {
    /// The modes that can be chosen.
    pub const ALL: [PowerMode; 4] = [PowerMode::Silent, PowerMode::Balanced, PowerMode::Performance, PowerMode::Custom];

    fn from_byte(b: u8) -> PowerMode {
        match b {
            0 | 6 => PowerMode::Balanced,
            1 | 2 => PowerMode::Performance,
            3 | 5 => PowerMode::Silent,
            4 => PowerMode::Custom,
            other => PowerMode::Other(other),
        }
    }

    /// The bytes to try, in order: Performance is 2, or 1 (Gaming) on the 2020 Blades.
    fn bytes(self) -> &'static [u8] {
        match self {
            PowerMode::Silent => &[5],
            PowerMode::Balanced => &[0],
            PowerMode::Performance => &[2, 1],
            PowerMode::Custom => &[4],
            PowerMode::Other(_) => &[],
        }
    }
}

impl std::fmt::Display for PowerMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PowerMode::Silent => f.write_str("silent"),
            PowerMode::Balanced => f.write_str("balanced"),
            PowerMode::Performance => f.write_str("performance"),
            PowerMode::Custom => f.write_str("custom"),
            PowerMode::Other(b) => write!(f, "other-{b}"),
        }
    }
}

impl std::str::FromStr for PowerMode {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        PowerMode::ALL
            .into_iter()
            .find(|m| m.to_string() == s)
            .ok_or_else(|| Error::InvalidArg(format!("unknown power mode '{s}' (silent, balanced, performance, custom)")))
    }
}

/// Boost levels in Custom mode: CPU low, medium, high, boost; GPU low, medium, high.
pub const CPU_BOOST_MAX: u8 = 3;
pub const GPU_BOOST_MAX: u8 = 2;

/// A performance setting as a profile stores it: `silent`, `balanced`, `performance`, or
/// `custom:<cpu>:<gpu>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Power {
    pub mode: PowerMode,
    /// `(cpu, gpu)` boost, Custom only.
    pub boost: Option<(u8, u8)>,
}

impl std::fmt::Display for Power {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.boost {
            Some((cpu, gpu)) if self.mode == PowerMode::Custom => write!(f, "custom:{cpu}:{gpu}"),
            _ => write!(f, "{}", self.mode),
        }
    }
}

impl std::str::FromStr for Power {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let mut parts = s.split(':');
        let mode: PowerMode = parts.next().unwrap_or_default().parse()?;
        let level = |p: Option<&str>, max: u8, what: &str| -> Result<Option<u8>> {
            match p {
                None => Ok(None),
                Some(v) => match v.parse::<u8>() {
                    Ok(n) if n <= max => Ok(Some(n)),
                    _ => Err(Error::InvalidArg(format!("{what} boost is 0 to {max}, not '{v}'"))),
                },
            }
        };
        let cpu = level(parts.next(), CPU_BOOST_MAX, "CPU")?;
        let gpu = level(parts.next(), GPU_BOOST_MAX, "GPU")?;
        if parts.next().is_some() || (mode != PowerMode::Custom && (cpu.is_some() || gpu.is_some())) {
            return Err(Error::InvalidArg(format!("'{s}': only custom takes boost levels, as custom:<cpu>:<gpu>")));
        }
        let boost = match (cpu, gpu) {
            (Some(c), Some(g)) => Some((c, g)),
            (None, None) => None,
            _ => return Err(Error::InvalidArg(format!("'{s}': give both levels, custom:<cpu>:<gpu>"))),
        };
        Ok(Power { mode, boost })
    }
}

/// Fan speeds and temperatures, as the controller reports them. Each is `None` when the
/// laptop doesn't answer for it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sensors {
    /// RPM of the CPU and GPU fans.
    pub fans: Vec<u32>,
    /// Degrees Celsius near the CPU and the GPU (a controller sensor, lower than the die).
    pub cpu_temp: Option<u8>,
    pub gpu_temp: Option<u8>,
}

/// The controller answered that it doesn't take this command.
fn refused(e: &Error) -> bool {
    matches!(e, Error::Device { .. } | Error::Unsupported(_))
}

impl Device {
    fn is_laptop(&self) -> bool {
        self.spec.as_ref().is_some_and(|s| s.kind == Kind::Laptop)
    }

    fn zone_mode(&self, zone: u8) -> Result<u8> {
        let r = self.get_echo(PERFORMANCE, PERFORMANCE_SIZE, &[0, zone, 0, 0], 2)?;
        Ok(r.args()[2])
    }

    /// The performance mode, `None` for anything that isn't a laptop or a laptop whose
    /// controller doesn't answer. The two zones can disagree for a moment while the mode
    /// changes (a key on the laptop, Synapse), so a disagreement is read once more.
    pub fn power_mode(&self) -> Result<Option<PowerMode>> {
        if !self.is_laptop() {
            return Ok(None);
        }
        let mut last = None;
        for _ in 0..2 {
            let (cpu, gpu) = match (self.zone_mode(ZONES[0]), self.zone_mode(ZONES[1])) {
                (Ok(c), Ok(g)) => (c, g),
                (Err(e), _) if refused(&e) => return Ok(None),
                (Err(e), _) | (_, Err(e)) => return Err(e),
            };
            if cpu == gpu {
                return Ok(Some(PowerMode::from_byte(cpu)));
            }
            last = Some(cpu);
        }
        Ok(last.map(PowerMode::Other))
    }

    /// The boost levels `(cpu, gpu)`. Only meaningful in Custom mode.
    pub fn boost(&self) -> Result<(u8, u8)> {
        let cpu = self.get_echo(BOOST, BOOST_SIZE, &[0, 1, 0], 2)?.args()[2];
        let gpu = self.get_echo(BOOST, BOOST_SIZE, &[0, 2, 0], 2)?.args()[2];
        Ok((cpu, gpu))
    }

    /// What the laptop runs now, boost included in Custom.
    pub fn power(&self) -> Result<Option<Power>> {
        let Some(mode) = self.power_mode()? else { return Ok(None) };
        let boost = if mode == PowerMode::Custom { self.boost().ok() } else { None };
        Ok(Some(Power { mode, boost }))
    }

    /// Set the performance mode on both zones, with the fans automatic, and read it back.
    /// For Custom, also the boost levels when given.
    pub fn set_power(&self, want: Power) -> Result<()> {
        if !self.is_laptop() {
            return Err(Error::Unsupported("performance modes on anything but a Blade laptop".into()));
        }
        let mut kept = false;
        for &byte in want.mode.bytes() {
            for zone in ZONES {
                self.transact(PERFORMANCE, PERFORMANCE_SIZE, &[1, zone, byte, FAN_AUTO])?;
            }
            if self.power_mode()? == Some(want.mode) {
                kept = true;
                break;
            }
        }
        if !kept {
            let now = self.power_mode().ok().flatten();
            return Err(Error::Unsupported(format!(
                "this laptop didn't keep {} (it reads {})",
                want.mode,
                now.map_or("nothing".into(), |m| m.to_string())
            )));
        }
        if let (PowerMode::Custom, Some((cpu, gpu))) = (want.mode, want.boost) {
            self.set_boost(cpu, gpu)?;
        }
        Ok(())
    }

    /// Set the CPU and GPU boost (Custom mode only) and read them back.
    pub fn set_boost(&self, cpu: u8, gpu: u8) -> Result<()> {
        if cpu > CPU_BOOST_MAX || gpu > GPU_BOOST_MAX {
            return Err(Error::InvalidArg(format!("boost is CPU 0-{CPU_BOOST_MAX}, GPU 0-{GPU_BOOST_MAX}")));
        }
        // Outside Custom the controller ignores or misapplies boost: never sent there.
        if self.power_mode()? != Some(PowerMode::Custom) {
            return Err(Error::Unsupported("boost levels outside Custom mode".into()));
        }
        self.transact(BOOST, BOOST_SIZE, &[1, 1, cpu])?;
        self.transact(BOOST, BOOST_SIZE, &[1, 2, gpu])?;
        let got = self.boost()?;
        if got != (cpu, gpu) {
            return Err(Error::Unsupported(format!("this laptop didn't keep boost CPU {cpu}, GPU {gpu} (it reads {got:?})")));
        }
        Ok(())
    }

    /// The battery charge limit in percent, 100 for none. `None` for anything that isn't a
    /// laptop, or a laptop that can't report it (then it isn't offered: it couldn't be
    /// checked after setting).
    pub fn charge_limit(&self) -> Result<Option<u8>> {
        if !self.is_laptop() {
            return Ok(None);
        }
        let b = match self.get(CHARGE_LIMIT, 1, &[0]) {
            Ok(r) => r.args()[0],
            Err(e) if refused(&e) => return Ok(None),
            Err(e) => return Err(e),
        };
        Ok(match b {
            b if b & CHARGE_LIMIT_ON != 0 && (CHARGE_LIMIT_MIN..100).contains(&(b & 0x7F)) => Some(b & 0x7F),
            b if b & CHARGE_LIMIT_ON == 0 => Some(100),
            // Anything else is no answer we understand.
            _ => None,
        })
    }

    /// Set the charge limit (`CHARGE_LIMIT_MIN`-99, or 100 for none) and read it back.
    pub fn set_charge_limit(&self, percent: u8) -> Result<()> {
        if !(CHARGE_LIMIT_MIN..=100).contains(&percent) {
            return Err(Error::InvalidArg(format!("the charge limit is {CHARGE_LIMIT_MIN} to 100 %, not {percent}")));
        }
        if self.charge_limit()?.is_none() {
            return Err(Error::Unsupported("a charge limit on this laptop (it doesn't report one)".into()));
        }
        let byte = if percent == 100 { NO_CHARGE_LIMIT } else { CHARGE_LIMIT_ON | percent };
        self.transact(CHARGE_LIMIT, 1, &[byte])?;
        let got = self.charge_limit()?;
        if got != Some(percent) {
            let reads = match got {
                Some(100) => "no limit".to_string(),
                Some(p) => format!("{p} %"),
                None => "nothing it understands".to_string(),
            };
            return Err(Error::Unsupported(format!("this laptop didn't keep the charge limit at {percent} % (it reads {reads})")));
        }
        Ok(())
    }

    /// Fan speeds and temperatures. Read only; what doesn't answer is left out.
    pub fn laptop_sensors(&self) -> Option<Sensors> {
        if !self.is_laptop() {
            return None;
        }
        let fans = ZONES
            .iter()
            .filter_map(|&z| self.get_echo(FAN_SPEED, 3, &[0, z, 0], 2).ok().map(|r| r.args()[2] as u32 * 100))
            .collect();
        // A controller sensor: anything outside 10-125 °C is no reading.
        let believable = |c: u8| (10..=125).contains(&c).then_some(c);
        let (cpu_temp, gpu_temp) = match self.get(TEMPERATURES, 3, &[0]) {
            Ok(r) => (believable(r.args()[1]), believable(r.args()[2])),
            Err(_) => (None, None),
        };
        Some(Sensors { fans, cpu_temp, gpu_temp })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_settings_round_trip_through_text() {
        for text in ["silent", "balanced", "performance", "custom", "custom:3:2", "custom:0:0"] {
            assert_eq!(text.parse::<Power>().unwrap().to_string(), text);
        }
        for bad in ["fast", "custom:4:0", "custom:0:3", "custom:1", "balanced:1:1", "custom:1:1:1", ""] {
            assert!(bad.parse::<Power>().is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn controller_bytes_read_as_the_modes_synapse_shows() {
        assert_eq!(PowerMode::from_byte(0), PowerMode::Balanced);
        assert_eq!(PowerMode::from_byte(1), PowerMode::Performance); // Gaming, 2020 Blades
        assert_eq!(PowerMode::from_byte(3), PowerMode::Silent); // Battery Saver
        assert_eq!(PowerMode::from_byte(6), PowerMode::Balanced); // Battery Balanced
        assert_eq!(PowerMode::from_byte(9), PowerMode::Other(9));
    }
}
