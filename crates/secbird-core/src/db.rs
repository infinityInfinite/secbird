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

/// Bluetooth ids and the product each belongs to, from Synapse's AvailableDevices.json
/// (`bleId`), leaving out ids that are also the cable or receiver id. Synapse 4 reaches some
/// of these over its own GATT service; OpenRazer reaches the Orochi V2 over Bluetooth with
/// the usual feature reports, which Secretarybird tries under the descriptor's report id
/// (#10, #51, see `crate::transport::NumberedReport`).
const BLUETOOTH: &[(u16, u16)] = &[
    (0x0061, 0x0062), (0x0076, 0x0080), (0x0082, 0x0083), (0x008E, 0x007C), (0x0092, 0x008F),
    (0x009B, 0x009A), (0x009D, 0x009C), (0x00A9, 0x00A7), (0x00AC, 0x00AA), (0x00B1, 0x00AF),
    (0x00BA, 0x00B9), (0x00C9, 0x00C7), (0x00CE, 0x00CC), (0x00D2, 0x00D0), (0x00D5, 0x00D3),
    (0x00D8, 0x00D6), (0x00DC, 0x00DA), (0x00E1, 0x00E0), (0x00E4, 0x00E2), (0x00E9, 0x00E7),
    (0x00EC, 0x00EB), (0x0248, 0x0249), (0x025B, 0x025A), (0x0272, 0x0258), (0x0278, 0x0277),
    (0x0291, 0x0292), (0x0297, 0x0298), (0x02B5, 0x02B3), (0x02BB, 0x02B9), (0x02C3, 0x02C4),
    (0x02CA, 0x02CC), (0x02CE, 0x02CD), (0x02D3, 0x02D4), (0x02D6, 0x02D7), (0x02F6, 0x02F4),
    (0x054C, 0x0532), (0x0558, 0x0548), (0x0559, 0x054A), (0x055B, 0x055A), (0x055D, 0x055C),
    (0x0572, 0x0567), (0x0575, 0x0576), (0x0578, 0x0579), (0x057B, 0x057C), (0x0589, 0x0587),
    (0x0A4F, 0x0A4E), (0x0A52, 0x0A51), (0x0A56, 0x0A54), (0x0A82, 0x0A81), (0x0F2D, 0x0F2C),
    (0x0F46, 0x0F45), (0x101D, 0x101C), (0x1023, 0x1022), (0x1028, 0x1025), (0x1033, 0x1030),
    (0xB006, 0xB005), (0xB00C, 0xB00A),
];

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

    /// The name of a device connected over Bluetooth, e.g. "DeathAdder V2 Pro (Bluetooth)".
    /// `None` when `pid` isn't one of the Bluetooth ids in Synapse's catalogue.
    pub fn bluetooth_name(&self, pid: u16) -> Option<String> {
        let product = BLUETOOTH.iter().find(|b| b.0 == pid)?.1;
        let name = self.get(product).map_or("Razer device", |s| {
            let name = s.name.as_str();
            [" (Wired)", " (Receiver)", " Wired"].iter().find_map(|end| name.strip_suffix(end)).unwrap_or(name)
        });
        Some(format!("{name} (Bluetooth)"))
    }

    /// The entry for `pid`, or for a Bluetooth id its product's entry renamed, so a mouse
    /// that answers over Bluetooth keeps its DPI range and lighting zones.
    pub fn spec(&self, pid: u16) -> Option<DeviceSpec> {
        if let Some(spec) = self.get(pid) {
            return Some(spec.clone());
        }
        let product = BLUETOOTH.iter().find(|b| b.0 == pid)?.1;
        let mut spec = self.get(product)?.clone();
        spec.name = self.bluetooth_name(pid)?;
        spec.pid = pid;
        spec.interface = None;
        spec.wireless = true;
        spec.source = "bluetooth".into();
        Some(spec)
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
    fn bluetooth_ids_are_named_after_their_product() {
        let db = Db::builtin().unwrap();
        assert_eq!(db.bluetooth_name(0x008E).as_deref(), Some("DeathAdder V2 Pro (Bluetooth)"));
        assert_eq!(db.bluetooth_name(0x00E1).as_deref(), Some("Razer device (Bluetooth)"));
        assert_eq!(db.bluetooth_name(0x007C), None);
        assert!(BLUETOOTH.iter().all(|b| db.get(b.0).is_none()), "a Bluetooth id is also in devices.json");

        // Over Bluetooth a mouse keeps its product's settings, under its own id and name.
        let bt = db.spec(0x008E).unwrap();
        let wired = db.get(0x007C).unwrap();
        assert_eq!((bt.pid, bt.name.as_str(), bt.wireless), (0x008E, "DeathAdder V2 Pro (Bluetooth)", true));
        assert_eq!((bt.dpi.is_some(), bt.leds.len()), (wired.dpi.is_some(), wired.leds.len()));
        assert!(db.spec(0x00E1).is_none(), "product not in the database");
        assert_eq!(db.spec(0x007C).unwrap().name, wired.name);
    }

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
