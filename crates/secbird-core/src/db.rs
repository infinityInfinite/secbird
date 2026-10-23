//! Device database: what each product supports, keyed by USB product id.
//!
//! The built-in list ships inside the binary. A user file (`devices.json` next to the
//! config) is merged on top, so adding a device is a data change, not a rebuild.

use crate::error::{Error, Result};
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::path::Path;

pub const RAZER_VID: u16 = 0x1532;

const BUILTIN: &str = include_str!("../devices.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mouse,
    Keyboard,
    Headset,
    Mousepad,
    Keypad,
    Laptop,
    Accessory,
    Webcam,
}

/// Which lighting command set a device speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lighting {
    /// `0F:xx` commands (most devices since ~2019).
    #[default]
    Extended,
    /// Older `03:xx` commands: per-LED state/color/effect, `03:0A` grid effects.
    Standard,
    /// `03:0D` per-LED effects (a few Naga models).
    MouseExtended,
    /// Kraken headsets: a different protocol, not supported yet.
    Kraken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Polling {
    /// 125/500/1000 Hz via `00:05`.
    Standard,
    /// Up to 8000 Hz via `00:40`.
    High,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DpiRange {
    pub min: u16,
    pub max: u16,
    pub step: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Led {
    pub name: String,
    pub id: u8,
}

/// Per-key lighting grid (keyboards, keypads): colors are sent row by row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Matrix {
    pub rows: u8,
    pub cols: u8,
    /// Matrix column of the leftmost key. 1 on most boards, where column 0 is the macro keys;
    /// 0 on boards without them (DeathStalker V2, #41).
    #[serde(default = "first_col")]
    pub first: u8,
}

fn first_col() -> u8 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceSpec {
    #[serde(deserialize_with = "hex_u16")]
    pub pid: u16,
    pub name: String,
    pub kind: Kind,
    /// USB interface that answers Protocol 2.5 feature reports.
    #[serde(default)]
    pub interface: Option<i32>,
    /// Transaction id the device accepts. Probed when absent.
    #[serde(default, deserialize_with = "opt_hex_u8")]
    pub txid: Option<u8>,
    #[serde(default)]
    pub dpi: Option<DpiRange>,
    #[serde(default)]
    pub polling: Option<Polling>,
    #[serde(default)]
    pub lighting: Lighting,
    #[serde(default)]
    pub leds: Vec<Led>,
    #[serde(default)]
    pub matrix: Option<Matrix>,
    #[serde(default)]
    pub wireless: bool,
    #[serde(default)]
    pub onboard_profiles: u8,
    /// Where this entry's facts come from (e.g. "synapse-descriptor", "openrazer").
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Default)]
pub struct Db {
    specs: HashMap<u16, DeviceSpec>,
}

impl Db {
    pub fn builtin() -> Result<Self> {
        let mut db = Db::default();
        db.merge_json(BUILTIN)?;
        Ok(db)
    }

    /// Built-in list plus an optional user file, which wins on conflicts.
    pub fn load(user_file: Option<&Path>) -> Result<Self> {
        let mut db = Self::builtin()?;
        if let Some(path) = user_file.filter(|p| p.exists()) {
            let text = std::fs::read_to_string(path)
                .map_err(|e| Error::Db(format!("{}: {e}", path.display())))?;
            db.merge_json(&text)?;
        }
        Ok(db)
    }

    pub fn merge_json(&mut self, text: &str) -> Result<()> {
        // Notepad and PowerShell 5 may save with a UTF-8 BOM.
        let list: Vec<DeviceSpec> = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| Error::Db(e.to_string()))?;
        for spec in list {
            self.specs.insert(spec.pid, spec);
        }
        Ok(())
    }

    pub fn get(&self, pid: u16) -> Option<&DeviceSpec> {
        self.specs.get(&pid)
    }

    pub fn iter(&self) -> impl Iterator<Item = &DeviceSpec> {
        self.specs.values()
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }
}

fn parse_hex(s: &str) -> std::result::Result<u32, String> {
    let digits = s.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(digits, 16).map_err(|e| format!("bad hex '{s}': {e}"))
}

fn hex_u16<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<u16, D::Error> {
    let s = String::deserialize(d)?;
    parse_hex(&s)
        .and_then(|v| u16::try_from(v).map_err(|e| e.to_string()))
        .map_err(serde::de::Error::custom)
}

fn opt_hex_u8<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<u8>, D::Error> {
    match Option::<String>::deserialize(d)? {
        None => Ok(None),
        Some(s) => parse_hex(&s)
            .and_then(|v| u8::try_from(v).map_err(|e| e.to_string()))
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_parses_and_has_basilisk_v3() {
        let db = Db::builtin().unwrap();
        let spec = db.get(0x0099).expect("Basilisk V3 entry");
        assert_eq!(spec.interface, Some(3));
        assert_eq!(spec.dpi.as_ref().unwrap().max, 26000);
    }

    #[test]
    fn builtin_entries_are_sane() {
        let text = BUILTIN;
        let list: Vec<DeviceSpec> = serde_json::from_str(text).unwrap();
        let db = Db::builtin().unwrap();
        assert_eq!(db.len(), list.len(), "duplicate product ids");
        assert!(db.len() > 250);
        for s in db.iter() {
            let id = format!("{:04X} {}", s.pid, s.name);
            assert!(!s.name.trim().is_empty(), "{id}: name");
            if let Some(d) = &s.dpi {
                assert!(d.min < d.max && d.step > 0 && d.max <= 45000, "{id}: dpi");
            }
            let mut ids: Vec<u8> = s.leds.iter().map(|l| l.id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), s.leds.len(), "{id}: duplicate LED ids");
            if let Some(m) = s.matrix {
                assert!(m.rows > 0 && m.cols > 0 && m.first <= 1, "{id}: matrix");
            }
            if s.lighting == Lighting::Kraken {
                assert!(s.leds.is_empty() && s.matrix.is_none(), "{id}: kraken has no 2.5 lighting");
            }
        }
        let kinds: std::collections::HashSet<_> = db.iter().map(|s| s.kind).collect();
        for k in [Kind::Mouse, Kind::Keyboard, Kind::Headset, Kind::Mousepad, Kind::Keypad, Kind::Laptop, Kind::Accessory] {
            assert!(kinds.contains(&k), "{k:?} missing");
        }
    }

    #[test]
    fn accepts_utf8_bom() {
        let mut db = Db::default();
        db.merge_json("\u{feff}[{\"pid\":\"0001\",\"name\":\"X\",\"kind\":\"mouse\"}]").unwrap();
        assert!(db.get(0x0001).is_some());
    }

    #[test]
    fn user_entries_override_builtin() {
        let mut db = Db::builtin().unwrap();
        db.merge_json(r#"[{"pid":"0099","name":"Renamed","kind":"mouse"}]"#).unwrap();
        assert_eq!(db.get(0x0099).unwrap().name, "Renamed");
    }
}
