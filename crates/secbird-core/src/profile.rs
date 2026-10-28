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
    /// UVC controls; absent fields leave the camera unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<crate::camera::CameraSettings>,
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
    /// Keyboards: the keys gaming mode blocks (`off`, or like `win,alt-tab`), see
    /// [`crate::GameMode`]. Per profile, so a profile tied to a game can turn it on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_mode: Option<String>,
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
    /// Button remaps written into the mouse's own memory with "Save to mouse".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onboard: Option<OnboardSave>,
}

/// A profile's button remaps saved in one of the mouse's on-board slots, so they work
/// without Secretarybird (on another PC, on Linux, before the tray starts, after sleep).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OnboardSave {
    /// The on-board slot written: the one active when saving (1-5).
    pub slot: u8,
    /// The profile saved, and its remaps as saved, to tell when it changed since.
    pub profile: String,
    #[serde(default)]
    pub buttons: BTreeMap<String, String>,
    /// What each written button held before Secretarybird first wrote it (Synapse's or the
    /// factory setting), to put back.
    #[serde(default)]
    pub before: BTreeMap<String, String>,
}

impl Default for DeviceProfiles {
    fn default() -> Self {
        Self {
            active: DEFAULT_PROFILE.into(),
            profiles: BTreeMap::from([(DEFAULT_PROFILE.into(), Profile::default())]),
            lighting_off: false,
            onboard: None,
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
    /// device (or saved on the mouse) but not by this one go back to their original
    /// setting, so switching profiles never leaves a stray remap behind.
    pub fn apply(&self, name: &str, dev: &Device) -> Vec<Applied> {
        let Some(profile) = self.profiles.get(name) else {
            return vec![Applied { setting: format!("profile '{name}'"), result: Err(Error::NotFound(name.into())) }];
        };
        let mut out = profile.apply_settings(dev, !self.lighting_off);
        let saved = self.onboard.iter().flat_map(|o| o.before.keys());
        let others: std::collections::BTreeSet<&String> = self
            .profiles
            .values()
            .flat_map(|p| p.buttons.keys())
            .chain(saved)
            .filter(|k| !profile.buttons.contains_key(*k))
            .collect();
        for key in others {
            let result = self.reset_button(key, dev);
            out.push(Applied { setting: format!("{key} back to saved"), result: result.map(|_| ()) });
        }
        out
    }

    /// Put a button or key (a setting key like `side-forward/hypershift`) back to its original
    /// setting: what it held before "Save to mouse" wrote the slot, else the slot's own.
    pub fn reset_button(&self, key: &str, dev: &Device) -> Result<bool> {
        let (input, layer) = crate::buttons::parse_setting_key(key)?;
        match self.onboard.as_ref().and_then(|o| o.before.get(key)) {
            Some(original) => dev.set_input(input, layer, original.parse()?),
            None => dev.reset_input(input, layer),
        }
    }

    /// Whether the device has lost profile `name`'s button remaps, as a wireless mouse
    /// does when it sleeps and wakes (#45). Reads back one remapped button; `false` when
    /// the profile remaps nothing or the device doesn't answer.
    pub fn buttons_lost(&self, name: &str, dev: &Device) -> bool {
        let Some((key, action)) = self.profiles.get(name).and_then(|p| p.buttons.iter().next()) else {
            return false;
        };
        let Ok((input, layer)) = crate::buttons::parse_setting_key(key) else { return false };
        let Ok(want) = action.parse::<crate::buttons::Action>() else { return false };
        dev.input(input, layer).is_ok_and(|have| have != want)
    }

    /// Send only profile `name`'s button remaps (see [`DeviceProfiles::buttons_lost`]).
    pub fn apply_buttons(&self, name: &str, dev: &Device) -> Vec<Applied> {
        self.profiles.get(name).map(|p| p.apply_buttons(dev)).unwrap_or_default()
    }

    /// Send the battery colour again to each zone of profile `name` that shows the battery
    /// ([`crate::effects::Effect::Battery`]), so it follows the level. Nothing when this
    /// device's lighting is left to another app.
    pub fn refresh_battery_lights(&self, name: &str, dev: &Device) -> Vec<Applied> {
        let Some(profile) = self.profiles.get(name).filter(|_| !self.lighting_off) else { return Vec::new() };
        profile
            .lighting
            .iter()
            .filter(|(_, z)| !z.off && z.effect == Some(crate::effects::Effect::Battery))
            .map(|(zone, _)| Applied {
                setting: format!("{zone} battery"),
                result: dev.led_id(zone).and_then(|led| dev.set_effect(led, &crate::effects::Effect::Battery)),
            })
            .collect()
    }

    /// Send profile `name`'s per-key colors again: a keyboard drops them in its quick
    /// adjustment mode (#45) and never says when it leaves it. `None` when the profile has
    /// none or this device's lighting is left to another app.
    pub fn refresh_frame(&self, name: &str, dev: &Device) -> Option<Result<()>> {
        let profile = self.profiles.get(name).filter(|p| !self.lighting_off && !p.keys.is_empty())?;
        Some(dev.show_frame(&profile.keys))
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
        store.rename_old_buttons();
        Ok(store)
    }

    /// Button 128 was saved as `top-extra` before 0.5; it is the extra side button (#54).
    fn rename_old_buttons(&mut self) {
        let rename = |map: &mut BTreeMap<String, String>| {
            let old: Vec<String> = map.keys().filter(|k| k.split('/').next() == Some("top-extra")).cloned().collect();
            for key in old {
                let value = map.remove(&key).unwrap();
                map.insert(key.replacen("top-extra", "side-extra", 1), value);
            }
        };
        for device in self.devices.values_mut() {
            device.profiles.values_mut().for_each(|p| rename(&mut p.buttons));
            if let Some(onboard) = device.onboard.as_mut() {
                rename(&mut onboard.buttons);
                rename(&mut onboard.before);
            }
        }
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
                // The only copy of what the mouse's slot held before a "Save to mouse".
                onboard: incoming.onboard.clone(),
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

    /// [`Store::import`] for a backup made on another PC. Devices are keyed by serial, so
    /// profiles saved for a unit that isn't connected here go to a connected device of the
    /// same model that the backup has nothing for (#40). `connected` are device keys.
    pub fn import_for(&mut self, mut other: Store, connected: &[String]) -> usize {
        let pid = |key: &str| key.split(':').next().unwrap_or(key).to_string();
        let keys: Vec<String> = other.devices.keys().cloned().collect();
        for key in keys.into_iter().filter(|k| !connected.contains(k)) {
            let Some(target) = connected.iter().find(|c| pid(c) == pid(&key) && !other.devices.contains_key(*c)) else {
                continue;
            };
            if let Some(profiles) = other.devices.remove(&key) {
                other.devices.insert(target.clone(), profiles);
            }
        }
        self.import(other)
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

    pub fn for_camera(&mut self, id: &str) -> &mut DeviceProfiles {
        self.devices.entry(id.to_owned()).or_default()
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
        if let Some(g) = &self.game_mode {
            push(format!("gaming mode: {g}"), g.parse().and_then(|g| dev.set_game_mode(g)));
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
        out.extend(self.apply_buttons(dev));
        out
    }

    fn apply_buttons(&self, dev: &Device) -> Vec<Applied> {
        self.buttons
            .iter()
            .map(|(key, action)| {
                let result = crate::buttons::parse_setting_key(key).and_then(|(input, layer)| {
                    let action: crate::buttons::Action = action.parse()?;
                    dev.set_input(input, layer, action).map(|_| ())
                });
                Applied { setting: format!("{key} -> {action}"), result }
            })
            .collect()
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

    /// On the first real mouse: `cargo test -p secbird-core real_mouse -- --ignored`.
    /// Sends F13 to side-forward for a moment and puts it back to the saved setting.
    #[test]
    #[ignore]
    fn real_mouse_lost_buttons_are_noticed_and_sent_again() {
        use crate::buttons::{Action, Layer};
        let api = crate::hidapi::HidApi::new().unwrap();
        let devs = crate::discover(&api, &crate::Db::builtin().unwrap());
        let dev = devs.iter().find(|d| !d.buttons().is_empty()).expect("a mouse with remappable buttons");
        let mut p = DeviceProfiles::default();
        p.active_mut().buttons.insert("side-forward".into(), "key:f13".into());
        assert!(p.apply_buttons("Default", dev).iter().all(|a| a.result.is_ok()));
        assert!(!p.buttons_lost("Default", dev), "just sent");
        dev.reset_button(5, Layer::Normal).unwrap(); // what a wake does to the live slot
        assert!(p.buttons_lost("Default", dev), "noticed");
        assert!(p.apply_buttons("Default", dev).iter().all(|a| a.result.is_ok()));
        assert_eq!(dev.button(5, Layer::Normal).unwrap(), Action::Key { modifiers: 0, usage: 0x68 });
        assert!(!p.buttons_lost("Default", dev), "sent again");
        dev.reset_button(5, Layer::Normal).unwrap();
    }

    /// On the first real mouse: `cargo test -p secbird-core real_mouse_saves -- --ignored --nocapture`.
    /// Saves F13 on side-forward into the active on-board slot, then puts the slot back.
    #[test]
    #[ignore]
    fn real_mouse_saves_to_its_slot_and_puts_it_back() {
        use crate::buttons::{Action, Layer};
        let api = crate::hidapi::HidApi::new().unwrap();
        let devs = crate::discover(&api, &crate::Db::builtin().unwrap());
        let dev = devs.iter().find(|d| !d.buttons().is_empty()).expect("a mouse with remappable buttons");
        let slot = dev.saved_slot();
        let before = dev.slot_button(slot, 5, Layer::Normal).unwrap();
        println!("slot {slot}: side-forward is {before}");
        let path = std::env::temp_dir().join(format!("secbird-real-onboard-{}.json", std::process::id()));
        let mut store = Store::load(&path).unwrap();
        let f13 = Action::Key { modifiers: 0, usage: 0x68 };
        crate::actions::set_button(dev, &mut store, "side-forward", Layer::Normal, Some(f13)).unwrap();
        crate::actions::save_buttons_to_mouse(dev, &mut store).unwrap();
        let saved = dev.slot_button(slot, 5, Layer::Normal).unwrap();
        println!("after save: slot has {saved}, backup {:?}", store.for_device(dev).onboard.as_ref().map(|o| &o.before));
        crate::actions::restore_all_buttons(dev, &mut store).unwrap();
        let after = dev.slot_button(slot, 5, Layer::Normal).unwrap();
        println!("after put back: slot has {after}, live {}", dev.button(5, Layer::Normal).unwrap());
        let _ = std::fs::remove_file(&path);
        assert_eq!(saved, f13);
        assert_eq!(after, before);
        assert_eq!(dev.button(5, Layer::Normal).unwrap(), before);
    }

    /// On the first real mouse: `cargo test -p secbird-core real_mouse_dims -- --ignored --nocapture`.
    /// Its first zone goes dark for 3 seconds with a write that isn't saved, then back.
    #[test]
    #[ignore]
    fn real_mouse_dims_without_saving() {
        let api = crate::hidapi::HidApi::new().unwrap();
        let devs = crate::discover(&api, &crate::Db::builtin().unwrap());
        let dev = devs.iter().find(|d| !d.buttons().is_empty()).expect("a mouse");
        let led = dev.spec.as_ref().unwrap().leds[0].id;
        let saved = dev.brightness(led).unwrap();
        dev.set_brightness_live(led, 0).unwrap();
        let during = dev.brightness(led).unwrap();
        println!("led {led}: saved {saved}%, read while dark {during}%");
        std::thread::sleep(std::time::Duration::from_secs(3));
        dev.set_brightness_live(led, saved).unwrap();
        assert_eq!(dev.brightness(led).unwrap(), saved);
    }

    #[test]
    fn old_top_extra_button_is_renamed_on_load() {
        let path = std::env::temp_dir().join(format!("secbird-top-extra-{}.json", std::process::id()));
        let json = r#"{"devices":{"00E3":{"active":"Default","profiles":{"Default":{"buttons":{"top-extra":"key:f13","top-extra/hypershift":"disabled","wheel":"right"}}}}}}"#;
        std::fs::write(&path, json).unwrap();
        let store = Store::load(&path).unwrap();
        std::fs::remove_file(&path).ok();
        let buttons = &store.devices["00E3"].profiles["Default"].buttons;
        let keys: Vec<&str> = buttons.keys().map(String::as_str).collect();
        assert_eq!(keys, ["side-extra", "side-extra/hypershift", "wheel"]);
        assert_eq!(buttons["side-extra"], "key:f13");
    }

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

    #[test]
    fn import_from_another_pc_goes_to_the_connected_mouse() {
        let mut theirs = Store::default();
        theirs.profiles_for("0096:OTHERPC01", 0x0096).create("Gaming").unwrap();
        theirs.profiles_for("0084:UNPLUGGED", 0x0084).active_mut().polling = Some(500);
        let mut mine = Store::default();
        let connected = ["0096:THISPC001".to_string()];

        assert_eq!(mine.import_for(theirs, &connected), 3);
        assert!(mine.devices["0096:THISPC001"].profiles.contains_key("Gaming"));
        assert!(!mine.devices.contains_key("0096:OTHERPC01"));
        // Nothing connected of that model: kept as it was, for when it's plugged in.
        assert!(mine.devices.contains_key("0084:UNPLUGGED"));
    }
}
