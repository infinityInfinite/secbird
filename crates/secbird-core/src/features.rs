//! Typed device settings built on [`Device::transact`].

use crate::commands as cmd;
use crate::commands::BACKLIGHT_LED;
use crate::db::{Lighting, Matrix, Polling};
use crate::device::Device;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl std::fmt::Display for Rgb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

impl std::str::FromStr for Rgb {
    type Err = Error;

    /// `RRGGBB` or `#RRGGBB`.
    fn from_str(s: &str) -> Result<Self> {
        let hex = s.trim_start_matches('#');
        let bad = || Error::InvalidArg(format!("color '{s}' is not RRGGBB"));
        if hex.len() != 6 {
            return Err(bad());
        }
        let v = u32::from_str_radix(hex, 16).map_err(|_| bad())?;
        Ok(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

/// The DPI values the mouse's DPI button cycles through.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DpiStages {
    /// 1-based index of the stage in use.
    pub active: u8,
    /// `(x, y)` per stage, at most [`cmd::MAX_DPI_STAGES`].
    pub stages: Vec<(u16, u16)>,
}

/// What a laptop's keyboard backlight does when the laptop sits idle: Synapse's "Smart
/// Dimming". With it on, the backlight dims or goes dark until the next keypress (#20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimming {
    /// Smart Dimming off: the backlight stays as set.
    StaysLit,
    Half,
    Quarter,
    Dark,
}

impl Dimming {
    pub const ALL: [Dimming; 4] = [Dimming::StaysLit, Dimming::Half, Dimming::Quarter, Dimming::Dark];

    /// Synapse's level byte: 0 half, 1 quarter, 2 off.
    fn level(self) -> Option<u8> {
        match self {
            Dimming::StaysLit => None,
            Dimming::Half => Some(0),
            Dimming::Quarter => Some(1),
            Dimming::Dark => Some(2),
        }
    }

    fn from_reply(enabled: u8, level: u8) -> Dimming {
        match (enabled, level) {
            (0, _) => Dimming::StaysLit,
            (_, 0) => Dimming::Half,
            (_, 1) => Dimming::Quarter,
            _ => Dimming::Dark,
        }
    }
}

impl std::fmt::Display for Dimming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Dimming::StaysLit => "stays-lit",
            Dimming::Half => "half",
            Dimming::Quarter => "quarter",
            Dimming::Dark => "dark",
        })
    }
}

impl std::str::FromStr for Dimming {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Dimming::ALL
            .into_iter()
            .find(|d| d.to_string() == s)
            .ok_or_else(|| Error::InvalidArg(format!("'{s}': use stays-lit, half, quarter or dark")))
    }
}

/// A keyboard's gaming mode: the keys it blocks so a game isn't left by accident. Nothing
/// blocked is gaming mode off. As text: `off`, or the blocked keys, like `win,alt-tab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameMode {
    pub win: bool,
    pub alt_tab: bool,
    pub alt_f4: bool,
}

impl GameMode {
    const KEYS: [(&'static str, u8); 3] = [("win", 4), ("alt-tab", 8), ("alt-f4", 16)];

    pub fn is_off(self) -> bool {
        self == GameMode::default()
    }

    fn flags(self) -> [bool; 3] {
        [self.win, self.alt_tab, self.alt_f4]
    }

    fn bits(self) -> u8 {
        if self.is_off() {
            return 0;
        }
        Self::KEYS.iter().zip(self.flags()).filter(|(_, on)| *on).fold(1, |bits, ((_, bit), _)| bits | bit)
    }

    fn from_bits(bits: u8) -> GameMode {
        let has = |bit: u8| bits & 1 != 0 && bits & bit != 0;
        GameMode { win: has(4), alt_tab: has(8), alt_f4: has(16) }
    }
}

impl std::fmt::Display for GameMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_off() {
            return f.write_str("off");
        }
        let keys: Vec<&str> =
            Self::KEYS.iter().zip(self.flags()).filter(|(_, on)| *on).map(|((name, _), _)| *name).collect();
        f.write_str(&keys.join(","))
    }
}

impl std::str::FromStr for GameMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let mut mode = GameMode::default();
        for key in s.split(',').map(str::trim).filter(|k| !k.is_empty() && *k != "off") {
            match key {
                "win" => mode.win = true,
                "alt-tab" => mode.alt_tab = true,
                "alt-f4" => mode.alt_f4 = true,
                _ => return Err(Error::InvalidArg(format!("'{key}': use off, or win, alt-tab and alt-f4"))),
            }
        }
        Ok(mode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SleepTimer {
    pub minutes: u16,
    pub max_minutes: u16,
    /// `Some` on devices that can turn sleeping off entirely.
    pub auto_sleep: Option<bool>,
}

impl Device {
    pub fn firmware(&self) -> Result<(u8, u8)> {
        let r = self.get(cmd::FIRMWARE, cmd::FIRMWARE_SIZE, &[])?;
        Ok((r.args()[0], r.args()[1]))
    }

    pub fn serial(&self) -> Result<String> {
        let r = self.get(cmd::SERIAL, cmd::SERIAL_SIZE, &[])?;
        let raw = &r.args()[..cmd::SERIAL_SIZE as usize];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Ok(String::from_utf8_lossy(&raw[..end]).trim().to_string())
    }

    pub fn dpi(&self) -> Result<(u16, u16)> {
        let r = self.get(cmd::DPI, cmd::DPI_SIZE, &[cmd::NOSTORE])?;
        let a = r.args();
        Ok((u16::from_be_bytes([a[1], a[2]]), u16::from_be_bytes([a[3], a[4]])))
    }

    /// Set DPI and store it in the device's memory, rounded to the device's step.
    pub fn set_dpi(&self, x: u16, y: u16) -> Result<(u16, u16)> {
        let (x, y) = match self.spec.as_ref().and_then(|s| s.dpi.as_ref()) {
            Some(r) => (snap(x, r.min, r.max, r.step), snap(y, r.min, r.max, r.step)),
            None if (100..=30000).contains(&x) && (100..=30000).contains(&y) => (x, y),
            None => return Err(Error::InvalidArg(format!("DPI {x}x{y} out of range"))),
        };
        let [xh, xl] = x.to_be_bytes();
        let [yh, yl] = y.to_be_bytes();
        self.transact(cmd::DPI, cmd::DPI_SIZE, &[cmd::VARSTORE, xh, xl, yh, yl, 0, 0])?;
        Ok((x, y))
    }

    /// The DPI stages the DPI button cycles through.
    pub fn dpi_stages(&self) -> Result<DpiStages> {
        let r = self.get(cmd::DPI_STAGES, cmd::DPI_STAGES_SIZE, &[cmd::VARSTORE])?;
        let a = r.args();
        let count = (a[2] as usize).min(cmd::MAX_DPI_STAGES);
        let stages = (0..count)
            .map(|i| {
                let s = &a[3 + 7 * i..];
                (u16::from_be_bytes([s[1], s[2]]), u16::from_be_bytes([s[3], s[4]]))
            })
            .collect();
        Ok(DpiStages { active: a[1], stages })
    }

    /// Replace the DPI stages and store them in the device's memory. Values are rounded
    /// to the device's step; returns what was sent. The device also switches to the active
    /// stage's DPI.
    pub fn set_dpi_stages(&self, stages: &DpiStages) -> Result<DpiStages> {
        let n = stages.stages.len();
        if !(1..=cmd::MAX_DPI_STAGES).contains(&n) {
            return Err(Error::InvalidArg(format!("{n} DPI stages; 1 to {} are allowed", cmd::MAX_DPI_STAGES)));
        }
        if !(1..=n as u8).contains(&stages.active) {
            return Err(Error::InvalidArg(format!("active stage {} of {n}", stages.active)));
        }
        let range = self.spec.as_ref().and_then(|s| s.dpi.as_ref());
        let fit = |v: u16| match range {
            Some(r) => Ok(snap(v, r.min, r.max, r.step)),
            None if (100..=30000).contains(&v) => Ok(v),
            None => Err(Error::InvalidArg(format!("DPI {v} out of range"))),
        };
        let mut sent = DpiStages { active: stages.active, stages: Vec::with_capacity(n) };
        let mut args = vec![cmd::VARSTORE, stages.active, n as u8];
        for (i, &(x, y)) in stages.stages.iter().enumerate() {
            let (x, y) = (fit(x)?, fit(y)?);
            sent.stages.push((x, y));
            let ([xh, xl], [yh, yl]) = (x.to_be_bytes(), y.to_be_bytes());
            args.extend([i as u8, xh, xl, yh, yl, 0, 0]);
        }
        self.transact(cmd::DPI_STAGES, args.len() as u8, &args)?;
        Ok(sent)
    }

    /// Minutes of inactivity before the device sleeps, with the longest allowed. `None` for
    /// wired devices.
    pub fn sleep_timer(&self) -> Result<Option<SleepTimer>> {
        if !self.spec.as_ref().is_some_and(|s| s.wireless) {
            return Ok(None);
        }
        let r = self.get(cmd::IDLE_TIME, cmd::IDLE_TIME_SIZE, &[])?;
        let seconds = u16::from_be_bytes([r.args()[0], r.args()[1]]);
        let auto_sleep = self.get(cmd::AUTO_SLEEP, cmd::AUTO_SLEEP_SIZE, &[]).ok().map(|r| r.args()[1] == 1);
        Ok(Some(SleepTimer {
            minutes: seconds.div_ceil(60).max(1),
            max_minutes: if auto_sleep.is_some() { 60 } else { 15 },
            auto_sleep,
        }))
    }

    /// Set the sleep timer in minutes (1-15, or up to 60 on devices with an auto-sleep
    /// switch), then read it back like Synapse does.
    pub fn set_sleep_minutes(&self, minutes: u16) -> Result<()> {
        let timer = self.sleep_timer()?.ok_or_else(|| Error::Unsupported("sleep timer on a wired device".into()))?;
        if !(1..=timer.max_minutes).contains(&minutes) {
            return Err(Error::InvalidArg(format!("sleep after {minutes} min; 1 to {} are allowed", timer.max_minutes)));
        }
        let seconds = minutes * 60;
        self.transact(cmd::IDLE_TIME, cmd::IDLE_TIME_SIZE, &seconds.to_be_bytes())?;
        let r = self.get(cmd::IDLE_TIME, cmd::IDLE_TIME_SIZE, &[])?;
        if u16::from_be_bytes([r.args()[0], r.args()[1]]) != seconds {
            return Err(Error::Unsupported(format!("the device didn't keep a {minutes} min sleep timer")));
        }
        Ok(())
    }

    /// What the keyboard backlight does when the laptop is idle. `None` for anything that
    /// isn't a laptop.
    pub fn dimming(&self) -> Result<Option<Dimming>> {
        if !self.spec.as_ref().is_some_and(|s| s.kind == crate::Kind::Laptop) {
            return Ok(None);
        }
        let r = self.get(cmd::SMART_DIMMING, cmd::SMART_DIMMING_SIZE, &[1])?;
        Ok(Some(Dimming::from_reply(r.args()[1], r.args()[2])))
    }

    /// Set it, then read it back. Turning it off keeps the level the laptop has, as
    /// Synapse does.
    pub fn set_dimming(&self, want: Dimming) -> Result<()> {
        let now = self.get(cmd::SMART_DIMMING, cmd::SMART_DIMMING_SIZE, &[1])?;
        let args = match want.level() {
            Some(level) => [1, 1, level],
            None => [1, 0, now.args()[2]],
        };
        self.transact(cmd::SMART_DIMMING, cmd::SMART_DIMMING_SIZE, &args)?;
        let got = self.dimming()?;
        if got != Some(want) {
            return Err(Error::Unsupported(format!("the laptop didn't keep it (asked for {want}, reads {got:?})")));
        }
        Ok(())
    }

    /// The keys a keyboard or keypad blocks in gaming mode. `None` for other devices; an
    /// error when the keyboard has no gaming mode.
    pub fn game_mode(&self) -> Result<Option<GameMode>> {
        if !self.spec.as_ref().is_some_and(|s| matches!(s.kind, crate::Kind::Keyboard | crate::Kind::Keypad)) {
            return Ok(None);
        }
        let r = self.get(cmd::GAME_MODE, cmd::GAME_MODE_SIZE, &[])?;
        Ok(Some(GameMode::from_bits(r.args()[0])))
    }

    /// Set it, then read it back: a keyboard that doesn't block some of these keys says so.
    pub fn set_game_mode(&self, want: GameMode) -> Result<()> {
        self.transact(cmd::GAME_MODE, cmd::GAME_MODE_SIZE, &[want.bits(), 0, 0, 0])?;
        let got = self.game_mode()?;
        if got != Some(want) {
            let got = got.map_or("nothing".into(), |g| g.to_string());
            return Err(Error::Unsupported(format!("the keyboard didn't keep it (asked for {want}, reads {got})")));
        }
        Ok(())
    }

    /// Turn sleeping when idle on or off, on devices that have the switch. The live value
    /// (store 0) is rebuilt from the saved slot at power-on, so the slot gets it too (#37).
    pub fn set_auto_sleep(&self, enabled: bool) -> Result<()> {
        self.transact(cmd::AUTO_SLEEP, cmd::AUTO_SLEEP_SIZE, &[0, enabled as u8])?;
        // Best effort: a device without saved slots may refuse it, and the live value is set.
        let _ = self.transact(cmd::AUTO_SLEEP, cmd::AUTO_SLEEP_SIZE, &[self.saved_slot(), enabled as u8]);
        Ok(())
    }

    fn polling_kind(&self) -> Polling {
        self.spec.as_ref().and_then(|s| s.polling).unwrap_or(Polling::Standard)
    }

    pub fn polling_rate(&self) -> Result<u32> {
        match self.polling_kind() {
            Polling::Standard => {
                let r = self.get(cmd::POLLING, cmd::POLLING_SIZE, &[])?;
                cmd::polling_hz(r.args()[0])
            }
            Polling::High => {
                let r = self.get(cmd::POLLING_HS, cmd::POLLING_HS_SIZE, &[])?;
                cmd::polling_hs_hz(r.args()[1])
            }
        }
        .ok_or_else(|| Error::Unsupported("unknown polling rate code".into()))
    }

    pub fn set_polling_rate(&self, hz: u32) -> Result<()> {
        let unsupported = || Error::InvalidArg(format!("{hz} Hz is not supported by this device"));
        match self.polling_kind() {
            Polling::Standard => {
                let code = cmd::polling_code(hz).ok_or_else(unsupported)?;
                self.transact(cmd::POLLING, cmd::POLLING_SIZE, &[code])?;
            }
            Polling::High => {
                let code = cmd::polling_hs_code(hz).ok_or_else(unsupported)?;
                self.transact(cmd::POLLING_HS, cmd::POLLING_HS_SIZE, &[0x00, code])?;
                // The HyperPolling dongle keeps a live and a stored selector. Writing only
                // the live one (0x00) leaves the rate the device comes up with unchanged, so
                // both get the new code, as Synapse and OpenRazer do.
                if self.pid == crate::device::HYPERPOLLING_DONGLE {
                    self.transact(cmd::POLLING_HS, cmd::POLLING_HS_SIZE, &[0x01, code])?;
                }
            }
        }
        Ok(())
    }

    /// Battery percentage, or `None` for wired-only devices.
    pub fn battery(&self) -> Result<Option<u8>> {
        if !self.spec.as_ref().is_some_and(|s| s.wireless) {
            return Ok(None);
        }
        let r = self.get(cmd::BATTERY, cmd::POWER_SIZE, &[])?;
        Ok(Some(((r.args()[1] as u32 * 100 + 127) / 255) as u8))
    }

    pub fn charging(&self) -> Result<bool> {
        let r = self.get(cmd::CHARGING, cmd::POWER_SIZE, &[])?;
        Ok(r.args()[1] != 0)
    }

    /// Resolve a lighting zone by name ("logo") or numeric id.
    pub fn led_id(&self, zone: &str) -> Result<u8> {
        if let Some(led) = self.spec.as_ref().and_then(|s| s.leds.iter().find(|l| l.name == zone)) {
            return Ok(led.id);
        }
        zone.parse::<u8>().map_err(|_| Error::InvalidArg(format!("unknown lighting zone '{zone}'")))
    }

    /// The on-board slot that settings are saved to: the active one (`05:84`). The device
    /// shows the active slot, so a write to slot 1 while slot 2 is active reads back fine
    /// but never lights up (#37: a keyboard left on slot 2 by Synapse). Devices without
    /// slots don't answer and get slot 1.
    pub(crate) fn saved_slot(&self) -> u8 {
        match self.get(cmd::ACTIVE_SLOT, 1, &[]) {
            Ok(r) if (1..=5).contains(&r.args()[0]) => r.args()[0],
            _ => cmd::VARSTORE,
        }
    }

    fn lighting(&self) -> Lighting {
        self.spec.as_ref().map_or(Lighting::Extended, |s| s.lighting)
    }

    /// The lighting commands for one LED: a dock's own light takes the newer ones whatever
    /// the docked mouse speaks.
    pub(crate) fn lighting_for(&self, led: u8) -> Lighting {
        if self.is_dock_led(led) { Lighting::Extended } else { self.lighting() }
    }

    /// Where an LED's setting is saved: the active slot ([`Device::saved_slot`]), or slot 1
    /// for a dock's own light, which has none of the mouse's slots (#39 took `[1, 15, ...]`).
    pub(crate) fn store_for(&self, led: u8) -> u8 {
        if self.is_dock_led(led) { cmd::VARSTORE } else { self.saved_slot() }
    }

    /// Old-style grids light the whole device with one effect instead of per-LED commands.
    pub(crate) fn is_grid_backlight(&self, led: u8) -> bool {
        led == BACKLIGHT_LED && self.matrix().is_some()
    }

    fn no_lighting(&self) -> Error {
        Error::Unsupported(format!("lighting on {} uses a protocol Secretarybird doesn't speak yet", self.name()))
    }

    /// Brightness 0-100 %.
    pub fn set_brightness(&self, led: u8, percent: u8) -> Result<()> {
        let level = (percent.min(100) as u32 * 255 / 100) as u8;
        let (command, store) = match self.lighting_for(led) {
            Lighting::Extended => (cmd::MATRIX_BRIGHTNESS, self.store_for(led)),
            Lighting::Standard | Lighting::MouseExtended => (cmd::LED_BRIGHTNESS, cmd::VARSTORE),
            Lighting::Kraken => return Err(self.no_lighting()),
        };
        self.transact_led(led, command, 3, &[store, led, level])?;
        Ok(())
    }

    /// Brightness 0-100 %, as saved for the slot in use.
    pub fn brightness(&self, led: u8) -> Result<u8> {
        let (command, store) = match self.lighting_for(led) {
            Lighting::Extended => (cmd::MATRIX_BRIGHTNESS, self.store_for(led)),
            Lighting::Standard | Lighting::MouseExtended => (cmd::LED_BRIGHTNESS, cmd::VARSTORE),
            Lighting::Kraken => return Err(self.no_lighting()),
        };
        let r = self.transact_led(led, command.get(), 3, &[store, led, 0])?;
        Ok(((r.args()[2] as u32 * 100 + 127) / 255) as u8)
    }

    /// Brightness 0-100 % shown now but not saved, so a power cycle brings back the saved
    /// level (dimming while the PC is locked, #48). Only the newer lighting commands have
    /// a live-only write.
    pub fn set_brightness_live(&self, led: u8, percent: u8) -> Result<()> {
        if self.lighting_for(led) != Lighting::Extended {
            return Err(Error::Unsupported(format!("brightness that isn't saved on {}", self.name())));
        }
        let level = (percent.min(100) as u32 * 255 / 100) as u8;
        self.transact_led(led, cmd::MATRIX_BRIGHTNESS, 3, &[cmd::NOSTORE, led, level])?;
        Ok(())
    }

    pub fn set_static_color(&self, led: u8, Rgb(r, g, b): Rgb) -> Result<()> {
        match self.lighting_for(led) {
            Lighting::Extended => {
                let args = [self.store_for(led), led, cmd::EFFECT_STATIC, 0x00, 0x00, 0x01, r, g, b];
                self.transact_led(led, cmd::MATRIX_EFFECT, args.len() as u8, &args)?;
            }
            Lighting::Standard if self.is_grid_backlight(led) => {
                self.transact(cmd::GRID_EFFECT, 4, &[cmd::GRID_STATIC, r, g, b])?;
            }
            Lighting::Standard => {
                self.transact(cmd::LED_RGB, 5, &[cmd::VARSTORE, led, r, g, b])?;
                self.transact(cmd::LED_EFFECT, 3, &[cmd::VARSTORE, led, cmd::CLASSIC_STATIC])?;
                self.transact(cmd::LED_STATE, 3, &[cmd::VARSTORE, led, 1])?;
            }
            Lighting::MouseExtended => {
                self.transact(cmd::MOUSE_LED_EFFECT, 6, &[cmd::VARSTORE, led, cmd::GRID_STATIC, r, g, b])?;
            }
            Lighting::Kraken => return Err(self.no_lighting()),
        }
        Ok(())
    }

    pub fn lights_off(&self, led: u8) -> Result<()> {
        match self.lighting_for(led) {
            Lighting::Extended => {
                let args = [self.store_for(led), led, cmd::EFFECT_OFF, 0x00, 0x00, 0x00];
                self.transact_led(led, cmd::MATRIX_EFFECT, args.len() as u8, &args)?;
            }
            Lighting::Standard if self.is_grid_backlight(led) => {
                self.transact(cmd::GRID_EFFECT, 1, &[cmd::GRID_OFF])?;
            }
            Lighting::Standard => {
                self.transact(cmd::LED_STATE, 3, &[cmd::VARSTORE, led, 0])?;
            }
            Lighting::MouseExtended => {
                self.transact(cmd::MOUSE_LED_EFFECT, 3, &[cmd::VARSTORE, led, 0x00])?;
            }
            Lighting::Kraken => return Err(self.no_lighting()),
        }
        Ok(())
    }

    /// Per-key lighting grid, for keyboards and keypads.
    pub fn matrix(&self) -> Option<Matrix> {
        self.spec.as_ref().and_then(|s| s.matrix)
    }

    /// Send colors for keys `start..start + colors.len()` of one row. Shown once the
    /// custom-frame effect is active (see [`Device::show_frame`]); not saved on the device.
    pub fn set_row(&self, row: u8, start: u8, colors: &[Rgb]) -> Result<()> {
        let m = self.matrix().ok_or_else(|| Error::Unsupported("no per-key lighting".into()))?;
        let end = start as usize + colors.len();
        if colors.is_empty() || row >= m.rows || end > m.cols as usize {
            return Err(Error::InvalidArg(format!("row {row}, keys {start}..{end} outside {}x{}", m.rows, m.cols)));
        }
        let lighting = self.lighting();
        for (i, chunk) in colors.chunks(cmd::MATRIX_FRAME_MAX_KEYS).enumerate() {
            let first = (start as usize + i * cmd::MATRIX_FRAME_MAX_KEYS) as u8;
            let last = first + chunk.len() as u8 - 1;
            let rgb = chunk.iter().flat_map(|c| [c.0, c.1, c.2]);
            match lighting {
                Lighting::Extended => {
                    let mut args = vec![0x00, 0x00, row, first, last];
                    args.extend(rgb);
                    self.transact(cmd::MATRIX_FRAME, args.len() as u8, &args)?;
                }
                Lighting::Standard => {
                    let mut args = vec![0xFF, row, first, last];
                    args.extend(rgb);
                    self.transact(cmd::GRID_FRAME, cmd::GRID_FRAME_SIZE, &args)?;
                }
                Lighting::MouseExtended | Lighting::Kraken => return Err(self.no_lighting()),
            }
        }
        Ok(())
    }

    /// Show a full frame of per-key colors, row-major (`rows * cols` entries).
    pub fn show_frame(&self, frame: &[Rgb]) -> Result<()> {
        let m = self.matrix().ok_or_else(|| Error::Unsupported("no per-key lighting".into()))?;
        let cols = m.cols as usize;
        if frame.len() != m.rows as usize * cols {
            return Err(Error::InvalidArg(format!(
                "frame has {} colors, expected {} ({}x{})",
                frame.len(),
                m.rows as usize * cols,
                m.rows,
                m.cols
            )));
        }
        for (row, colors) in frame.chunks(cols).enumerate() {
            self.set_row(row as u8, 0, colors)?;
        }
        match self.lighting() {
            // Size 0x0C as Razer sends it; the trailing arguments are zero.
            Lighting::Extended => {
                // Not saved: a saved custom frame left the Huntsman V3 Pro dark after quick
                // adjustment mode anyway (#45), and dark after a power cycle without the tray.
                // The tray sends the frame again instead (Engine::refresh_frames).
                self.transact(cmd::MATRIX_EFFECT, 0x0C, &[cmd::NOSTORE, 0x00, cmd::EFFECT_CUSTOM_FRAME])?;
            }
            _ => {
                self.transact(cmd::GRID_EFFECT, 2, &[cmd::GRID_CUSTOM_FRAME, cmd::VARSTORE])?;
            }
        }
        Ok(())
    }
}

/// Clamp to `[min, max]` and round to the nearest multiple of `step`.
fn snap(v: u16, min: u16, max: u16, step: u16) -> u16 {
    let v = v.clamp(min, max);
    let step = step.max(1);
    let rounded = (v + step / 2) / step * step;
    rounded.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_rounds_and_clamps() {
        assert_eq!(snap(1026, 100, 26000, 50), 1050);
        assert_eq!(snap(1020, 100, 26000, 50), 1000);
        assert_eq!(snap(50, 100, 26000, 50), 100);
        assert_eq!(snap(30000, 100, 26000, 50), 26000);
    }

    #[test]
    fn rgb_parses_hex() {
        assert_eq!("#FF8000".parse::<Rgb>().unwrap(), Rgb(255, 128, 0));
        assert!("12345".parse::<Rgb>().is_err());
    }
}
