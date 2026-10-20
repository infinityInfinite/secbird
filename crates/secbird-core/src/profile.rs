//! Named settings profiles, kept per device and re-applied on demand.
//!
//! Every setting the user changes is recorded in the device's active profile, so the
//! store always mirrors what was last sent to the hardware.

use crate::device::Device;
use crate::error::{Error, Result};
use crate::features::{DpiStages, Rgb};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const DEFAULT_PROFILE: &str = "Default";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Rgb>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub off: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
    /// A built-in effect instead of the static color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<crate::effects::Effect>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<(u16, u16)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polling: Option<u32>,
    /// What the DPI button cycles through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi_stages: Option<DpiStages>,
    /// Minutes idle before a wireless device sleeps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep_minutes: Option<u16>,
    /// Sleeping when idle on or off, for devices with that switch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_sleep: Option<bool>,
    /// Laptops: what the keyboard backlight does when idle (`stays-lit`, `half`, `quarter`,
    /// `dark`), see [`crate::Dimming`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimming: Option<String>,
    /// Lighting zone name (or numeric id) -> state.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lighting: BTreeMap<String, Zone>,
    /// Per-key colors, row-major over the device's matrix. Empty when unused.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<Rgb>,
    /// Programs (lowercase exe names, e.g. `cs2.exe`) that switch the device to this
    /// profile while their window is in front.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    /// Button remaps: setting key (`side-forward`, `sniper/hypershift`, see
    /// [`crate::buttons::setting_key`]) -> action text (`key:ctrl+c`, `turbo:left@15`).
    /// Buttons not listed keep their saved on-board setting.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub buttons: BTreeMap<String, String>,
    /// Headset hardware settings by name (`anc`, `mic-eq`, `eq-2`, ...), see
    /// [`crate::headset`]. Written back to the headset when the profile is applied.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub audio: crate::headset::Settings,
}

/// `C:\Games\CS2.exe`, `cs2` or ` CS2.EXE ` -> `cs2.exe`. Empty when nothing is left.
pub fn normalize_app(name: &str) -> String {
    let name = name.trim().rsplit(['\\', '/']).next().unwrap_or("").trim().to_lowercase();
    if name.is_empty() || name.ends_with(".exe") {
        name
    } else {
        format!("{name}.exe")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfiles {
    pub active: String,
    pub profiles: BTreeMap<String, Profile>,
    /// Leave this device's lighting to another app, across every profile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lighting_off: bool,
}

impl Default for DeviceProfiles {
    fn default() -> Self {
        Self {
            active: DEFAULT_PROFILE.into(),
            profiles: BTreeMap::from([(DEFAULT_PROFILE.into(), Profile::default())]),
            lighting_off: false,
        }
    }
}

impl DeviceProfiles {
    pub fn active(&self) -> &Profile {
        &self.profiles[&self.active]
    }

    pub fn active_mut(&mut self) -> &mut Profile {
        self.profiles.entry(self.active.clone()).or_default()
    }

    /// Create `name` as a copy of the active profile.
    pub fn create(&mut self, name: &str) -> Result<()> {
        if self.profiles.contains_key(name) {
            return Err(Error::InvalidArg(format!("profile '{name}' already exists")));
        }
        // A program belongs to one profile, so the copy starts without any.
        let copy = Profile { apps: Vec::new(), ..self.active().clone() };
        self.profiles.insert(name.into(), copy);
        Ok(())
    }

    /// Set the programs that bring up profile `name`. A program listed here is taken
    /// off any other profile of this device.
    pub fn set_apps(&mut self, name: &str, apps: &[String]) -> Result<()> {
        if !self.profiles.contains_key(name) {
            return Err(Error::NotFound(format!("profile '{name}'")));
        }
        let mut list: Vec<String> = apps.iter().map(|a| normalize_app(a)).filter(|a| !a.is_empty()).collect();
        list.sort();
        list.dedup();
        for (other, p) in self.profiles.iter_mut() {
            if other != name {
                p.apps.retain(|a| !list.contains(a));
            }
        }
        self.profiles.get_mut(name).expect("checked above").apps = list;
        Ok(())
    }

    /// The profile tied to a program (a normalized exe name), if any.
    pub fn for_app(&self, exe: &str) -> Option<&str> {
        self.profiles.iter().find(|(_, p)| p.apps.iter().any(|a| a == exe)).map(|(n, _)| n.as_str())
    }

    /// The profile to show right now: the one tied to the program in front, else the active one.
    pub fn effective(&self, foreground: Option<&str>) -> &str {
        foreground.and_then(|exe| self.for_app(exe)).unwrap_or(&self.active)
    }

    /// Send profile `name` to the device. Buttons remapped by another profile of this
    /// device but not by this one go back to their saved setting, so switching profiles
    /// never leaves a stray remap behind.
    pub fn apply(&self, name: &str, dev: &Device) -> Vec<Applied> {
        let Some(profile) = self.profiles.get(name) else {
            return vec![Applied { setting: format!("profile '{name}'"), result: Err(Error::NotFound(name.into())) }];
        };
        let mut out = profile.apply_settings(dev, !self.lighting_off);
        let others: std::collections::BTreeSet<&String> = self
            .profiles
            .values()
            .flat_map(|p| p.buttons.keys())
            .filter(|k| !profile.buttons.contains_key(*k))
            .collect();
        for key in others {
            let result = crate::buttons::parse_setting_key(key).and_then(|(id, layer)| dev.reset_button(id, layer));
            out.push(Applied { setting: format!("{key} back to saved"), result: result.map(|_| ()) });
        }
        out
    }

    pub fn switch(&mut self, name: &str) -> Result<()> {
        if !self.profiles.contains_key(name) {
            return Err(Error::NotFound(format!("profile '{name}'")));
        }
        self.active = name.into();
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        if name == self.active {
            return Err(Error::InvalidArg("cannot delete the active profile; switch first".into()));
        }
        self.profiles
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| Error::NotFound(format!("profile '{name}'")))
    }
}

/// All profiles on disk, keyed by device (`"0099:SERIAL"`, or `"0099"` without a serial).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub devices: BTreeMap<String, DeviceProfiles>,
    #[serde(skip)]
    path: PathBuf,
}

impl Store {
    pub fn load(path: &Path) -> Result<Self> {
        let mut store: Store = match std::fs::read_to_string(path) {
            // Notepad and PowerShell 5 may save with a UTF-8 BOM.
            Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .map_err(|e| Error::Db(format!("{}: {e}", path.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::default(),
            Err(e) => return Err(Error::Db(format!("{}: {e}", path.display()))),
        };
        store.path = path.to_path_buf();
        Ok(store)
    }

    /// Write atomically: a crash mid-save never leaves a half-written file.
    pub fn save(&self) -> Result<()> {
        let io = |e: std::io::Error| Error::Db(format!("{}: {e}", self.path.display()));
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(|e| Error::Db(e.to_string()))?;
        std::fs::write(&tmp, text).map_err(io)?;
        std::fs::rename(&tmp, &self.path).map_err(io)
    }

    /// Add every profile from `other` (an export). A name that already exists for that
    /// device is imported as "name (imported)", "name (imported 2)", ...; active
    /// profiles are left alone. Returns how many profiles were added.
    pub fn import(&mut self, other: Store) -> usize {
        let mut added = 0;
        for (key, incoming) in other.devices {
            let target = self.devices.entry(key).or_insert_with(|| DeviceProfiles {
                active: incoming.active.clone(),
                profiles: BTreeMap::new(),
                lighting_off: incoming.lighting_off,
            });
            for (name, profile) in incoming.profiles {
                let mut unique = name.clone();
                let mut n = 1;
                while target.profiles.contains_key(&unique) {
                    unique = if n == 1 { format!("{name} (imported)") } else { format!("{name} (imported {n})") };
                    n += 1;
                }
                target.profiles.insert(unique, profile);
                added += 1;
            }
            if !target.profiles.contains_key(&target.active) {
                target.active = target.profiles.keys().next().cloned().unwrap_or_default();
            }
        }
        added
    }

    /// Profiles of one physical device (see [`Device::key`](crate::Device::key)).
    /// Profiles saved before devices were told apart by serial live under the bare
    /// product id; the first device of that model to ask takes them over.
    pub fn profiles_for(&mut self, key: &str, pid: u16) -> &mut DeviceProfiles {
        let legacy = format!("{pid:04X}");
        if key != legacy && !self.devices.contains_key(key) {
            if let Some(old) = self.devices.remove(&legacy) {
                self.devices.insert(key.to_string(), old);
            }
        }
        self.devices.entry(key.to_string()).or_default()
    }

    pub fn for_device(&mut self, dev: &crate::Device) -> &mut DeviceProfiles {
        self.profiles_for(&dev.key(), dev.pid)
    }

    pub fn device(&mut self, pid: u16) -> &mut DeviceProfiles {
        self.devices.entry(format!("{pid:04X}")).or_default()
    }

    /// Every program tied to a profile of any device.
    pub fn apps(&self) -> std::collections::HashSet<String> {
        self.devices.values().flat_map(|d| d.profiles.values()).flat_map(|p| p.apps.iter().cloned()).collect()
    }
}

/// Outcome of applying one setting from a profile.
#[derive(Debug)]
pub struct Applied {
    pub setting: String,
    pub result: Result<()>,
}

impl Profile {
    /// Send every recorded setting to the device. Keeps going past individual failures.
    /// For stored profiles, use [`DeviceProfiles::apply`] to respect lighting ownership.
    pub fn apply(&self, dev: &Device) -> Vec<Applied> {
        self.apply_settings(dev, true)
    }

    fn apply_settings(&self, dev: &Device, lighting: bool) -> Vec<Applied> {
        let mut out = Vec::new();
        let mut push = |setting: String, result: Result<()>| out.push(Applied { setting, result });

        if let Some(hz) = self.polling {
            push(format!("polling {hz} Hz"), dev.set_polling_rate(hz));
        }
        // Stages first: setting them also moves the mouse to the active stage's DPI.
        if let Some(stages) = &self.dpi_stages {
            push(format!("dpi stages ({})", stages.stages.len()), dev.set_dpi_stages(stages).map(|_| ()));
        }
        if let Some((x, y)) = self.dpi {
            push(format!("dpi {x}x{y}"), dev.set_dpi(x, y).map(|_| ()));
        }
        if let Some(on) = self.auto_sleep {
            push(format!("auto sleep {}", if on { "on" } else { "off" }), dev.set_auto_sleep(on));
        }
        if let Some(minutes) = self.sleep_minutes {
            push(format!("sleep after {minutes} min"), dev.set_sleep_minutes(minutes));
        }
        if let Some(d) = &self.dimming {
            push(format!("keyboard when idle: {d}"), d.parse().and_then(|d| dev.set_dimming(d)));
        }
        for (name, zone) in self.lighting.iter().filter(|_| lighting) {
            let led = match dev.led_id(name) {
                Ok(led) => led,
                Err(e) => {
                    push(format!("{name} lighting"), Err(e));
                    continue;
                }
            };
            if let Some(pct) = zone.brightness {
                push(format!("{name} brightness {pct}%"), dev.set_brightness(led, pct));
            }
            if zone.off {
                push(format!("{name} off"), dev.lights_off(led));
            } else if let Some(effect) = &zone.effect {
                push(format!("{name} {effect}"), dev.set_effect(led, effect));
            } else if let Some(color) = zone.color {
                push(format!("{name} color {color}"), dev.set_static_color(led, color));
            }
        }
        if lighting && !self.keys.is_empty() {
            push(format!("per-key colors ({} keys)", self.keys.len()), dev.show_frame(&self.keys));
        }
        for (key, action) in &self.buttons {
            let result = crate::buttons::parse_setting_key(key).and_then(|(id, layer)| {
                let action: crate::buttons::Action = action.parse()?;
                dev.set_button(id, layer, action).map(|_| ())
            });
            push(format!("{key} -> {action}"), result);
        }
        // Headset settings, presets before bands: switching a preset resets that EQ's bands.
        // Each setting is read once and only what differs is written back, so a profile
        // or program switch doesn't wear the headset's memory with identical values.
        let (current, _) = dev.audio_state();
        for (name, values) in self.audio.iter().filter(|(n, _)| n.ends_with("preset"))
            .chain(self.audio.iter().filter(|(n, _)| !n.ends_with("preset"))) {
            if current.get(name) == Some(values) {
                continue;
            }
            push(format!("audio {name}"), dev.audio_set(name, values));
        }
        out
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighting_ownership_defaults_migrates_and_survives_import() {
        let mut old: DeviceProfiles = serde_json::from_str(r#"{"active":"Default","profiles":{"Default":{}}}"#).unwrap();
        assert!(!old.lighting_off);
        assert!(serde_json::to_value(&old).unwrap().get("lighting_off").is_none());
        old.lighting_off = true;
        let restored: DeviceProfiles = serde_json::from_value(serde_json::to_value(&old).unwrap()).unwrap();
        assert!(restored.lighting_off);

        let mut mine = Store::default();
        mine.devices.insert("0099".into(), restored);
        assert!(mine.profiles_for("0099:UNIT1", 0x0099).lighting_off);
        assert!(!mine.profiles_for("0099:UNIT2", 0x0099).lighting_off);
        let mut incoming = Store::default();
        incoming.devices.insert("0099:UNIT1".into(), DeviceProfiles::default());
        incoming.devices.insert("0084:NEW".into(), old.clone());
        incoming.devices.insert("0099:UNIT2".into(), old);
        mine.import(incoming);
        assert!(mine.devices["0099:UNIT1"].lighting_off, "import must not re-enable an existing device");
        assert!(!mine.devices["0099:UNIT2"].lighting_off, "existing ownership is kept in both directions");
        assert!(mine.devices["0084:NEW"].lighting_off, "new devices restore backed-up ownership");
    }

    #[test]
    fn create_copies_active_and_switch_changes_it() {
        let mut p = DeviceProfiles::default();
        p.active_mut().dpi = Some((800, 800));
        p.create("Gaming").unwrap();
        p.switch("Gaming").unwrap();
        assert_eq!(p.active().dpi, Some((800, 800)));
        p.active_mut().dpi = Some((1600, 1600));
        assert_eq!(p.profiles[DEFAULT_PROFILE].dpi, Some((800, 800)));
    }

    #[test]
    fn app_names_are_normalized() {
        assert_eq!(normalize_app(r"C:\Games\CS2.exe"), "cs2.exe");
        assert_eq!(normalize_app(" dota2 "), "dota2.exe");
        assert_eq!(normalize_app("  "), "");
    }

    #[test]
    fn an_app_belongs_to_one_profile() {
        let mut p = DeviceProfiles::default();
        p.create("Gaming").unwrap();
        p.create("Work").unwrap();
        p.set_apps("Gaming", &["CS2.exe".into(), "dota2".into(), "cs2.exe".into()]).unwrap();
        assert_eq!(p.profiles["Gaming"].apps, ["cs2.exe", "dota2.exe"]);
        p.set_apps("Work", &["dota2.exe".into()]).unwrap();
        assert_eq!(p.profiles["Gaming"].apps, ["cs2.exe"]);
        assert_eq!(p.for_app("dota2.exe"), Some("Work"));
        assert_eq!(p.effective(Some("cs2.exe")), "Gaming");
        assert_eq!(p.effective(Some("notepad.exe")), DEFAULT_PROFILE);
        assert_eq!(p.effective(None), DEFAULT_PROFILE);
        assert!(p.set_apps("Missing", &[]).is_err());
    }

    #[test]
    fn new_profile_does_not_copy_apps() {
        let mut p = DeviceProfiles::default();
        p.set_apps(DEFAULT_PROFILE, &["cs2.exe".into()]).unwrap();
        p.create("Copy").unwrap();
        assert!(p.profiles["Copy"].apps.is_empty());
        assert_eq!(p.for_app("cs2.exe"), Some(DEFAULT_PROFILE));
    }

    #[test]
    fn cannot_delete_active_profile() {
        let mut p = DeviceProfiles::default();
        assert!(p.delete(DEFAULT_PROFILE).is_err());
    }

    #[test]
    fn store_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("secbird-test-{}", std::process::id()));
        let path = dir.join("profiles.json");
        let mut store = Store::load(&path).unwrap();
        let dev = store.device(0x0099);
        dev.active_mut().polling = Some(500);
        dev.active_mut().lighting.insert(
            "logo".into(),
            Zone { color: Some(Rgb(255, 0, 0)), off: false, brightness: Some(40), effect: None },
        );
        store.save().unwrap();

        let mut again = Store::load(&path).unwrap();
        let p = again.device(0x0099).active().clone();
        assert_eq!(p.polling, Some(500));
        assert_eq!(p.lighting["logo"].color, Some(Rgb(255, 0, 0)));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn legacy_profiles_move_to_the_serial_key() {
        let mut store = Store::default();
        store.device(0x0099).active_mut().dpi = Some((1600, 1600));
        let p = store.profiles_for("0099:PM24AB", 0x0099);
        assert_eq!(p.active().dpi, Some((1600, 1600)));
        assert!(!store.devices.contains_key("0099"));
        // A second unit of the same model starts fresh.
        assert_eq!(store.profiles_for("0099:XYZ999", 0x0099).active().dpi, None);
        assert_eq!(store.profiles_for("0099:PM24AB", 0x0099).active().dpi, Some((1600, 1600)));
    }

    #[test]
    fn import_keeps_existing_profiles() {
        let mut mine = Store::default();
        mine.device(0x0099).active_mut().dpi = Some((800, 800));
        let mut theirs = Store::default();
        theirs.device(0x0099).active_mut().dpi = Some((3200, 3200));
        theirs.device(0x0099).create("Gaming").unwrap();
        theirs.device(0x0084).active_mut().polling = Some(500);

        assert_eq!(mine.import(theirs), 3);
        let d = mine.device(0x0099);
        assert_eq!(d.active, "Default");
        assert_eq!(d.active().dpi, Some((800, 800)));
        assert_eq!(d.profiles["Default (imported)"].dpi, Some((3200, 3200)));
        assert!(d.profiles.contains_key("Gaming"));
        assert_eq!(mine.device(0x0084).active().polling, Some(500));
    }
}
