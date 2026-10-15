//! Device and profile handling for the tray, kept free of Win32 code.

use secbird_core::{config_dir, discover, hidapi::HidApi, Db, Device, Result, Store};
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

pub struct Engine {
    api: HidApi,
    devices: Vec<Device>,
    unsupported: Vec<Unsupported>,
    /// Synapse processes seen at the last rescan.
    synapse: Vec<String>,
    /// The program in front, when some profile is tied to it.
    foreground: Option<String>,
    /// Programs tied to any profile, and the profiles.json timestamp they were read at.
    apps: HashSet<String>,
    apps_read: Option<SystemTime>,
    /// Latest DPI each device reported through its event reports.
    live_dpi: HashMap<String, (u16, u16)>,
    live_seq: u64,
}

/// Battery state of one wireless device.
pub struct Battery {
    pub key: String,
    pub name: String,
    pub percent: u8,
    pub charging: bool,
}

/// A connected Razer product that can't be controlled (yet), shown so it isn't silently missing.
pub struct Unsupported {
    pub pid: u16,
    pub name: String,
    pub reason: &'static str,
}

/// What happened when a device's active profile was sent to it.
pub struct ApplyReport {
    pub device: String,
    pub profile: String,
    pub failures: Vec<String>,
}

impl Engine {
    /// The text of `secbird report`, for the tray's "Make a device report" item. Only reads.
    pub fn report_text(&mut self) -> String {
        let _ = self.api.refresh_devices();
        let db = Db::load(Some(&config_dir().join("devices.json"))).unwrap_or_default();
        secbird_core::report::build(&self.api, &db).to_string()
    }

    pub fn new() -> Result<Self> {
        Ok(Self {
            api: HidApi::new()?,
            devices: Vec::new(),
            unsupported: Vec::new(),
            synapse: Vec::new(),
            foreground: None,
            apps: HashSet::new(),
            apps_read: None,
            live_dpi: HashMap::new(),
            live_seq: 0,
        })
    }

    pub fn foreground(&self) -> Option<&str> {
        self.foreground.as_deref()
    }

    /// Collections that carry a device's own event reports (DPI and profile changes made
    /// with its buttons), as (device key, path). On the Basilisk V3 that is interface 1,
    /// collection 5 (usage 0001:0000, 16-byte reports); its sibling with the same usage
    /// stays silent, so listening to both costs nothing.
    pub fn event_paths(&self) -> Vec<(String, std::ffi::CString)> {
        self.devices
            .iter()
            .flat_map(|dev| {
                self.api
                    .device_list()
                    .filter(move |i| {
                        i.vendor_id() == secbird_core::db::RAZER_VID
                            && i.product_id() == dev.pid
                            && i.usage_page() == 0x0001
                            && i.usage() == 0x0000
                    })
                    .map(move |i| (dev.key(), i.path().to_owned()))
            })
            .collect()
    }

    pub fn open_events(&self, path: &std::ffi::CStr) -> Option<secbird_core::hidapi::HidDevice> {
        self.api.open_path(path).ok()
    }

    /// The device changed DPI by itself (its DPI button). Record it in the profile in
    /// use, so a replug or wake keeps it instead of going back.
    pub fn dpi_changed(&mut self, key: &str, x: u16, y: u16) {
        self.live_seq += 1;
        self.live_dpi.insert(key.to_string(), (x, y));
        let Some(dev) = self.devices.iter().find(|d| d.key() == key) else { return };
        if let Ok(mut store) = Self::store() {
            let p = store.for_device(dev);
            let name = p.effective(self.foreground.as_deref()).to_string();
            if let Some(profile) = p.profiles.get_mut(&name) {
                profile.dpi = Some((x, y));
                let _ = store.save();
            }
        }
    }

    /// Latest DPI reported by each device, and a counter that grows with every report,
    /// so the editor follows new reports without undoing a DPI just set in the editor.
    pub fn live_dpi(&self) -> (u64, &HashMap<String, (u16, u16)>) {
        (self.live_seq, &self.live_dpi)
    }

    /// Called when another window comes to the front, with its exe name. Devices whose
    /// profile depends on the program in front get that profile; nothing is sent when
    /// neither the old nor the new program is tied to a profile.
    pub fn set_foreground(&mut self, exe: Option<String>) -> Vec<ApplyReport> {
        self.refresh_apps();
        let exe = exe.map(|e| secbird_core::profile::normalize_app(&e)).filter(|e| self.apps.contains(e));
        if exe == self.foreground {
            return Vec::new();
        }
        let before = std::mem::replace(&mut self.foreground, exe);
        let Ok(mut store) = Self::store() else { return Vec::new() };
        let changed: Vec<String> = self
            .devices
            .iter()
            .filter(|d| {
                let p = store.for_device(d);
                p.effective(before.as_deref()) != p.effective(self.foreground.as_deref())
            })
            .map(|d| d.key())
            .collect();
        self.apply(&changed)
    }

    /// Re-read the tied programs only when profiles.json has changed: a foreground switch
    /// then costs one file-time lookup.
    fn refresh_apps(&mut self) {
        let stamp = std::fs::metadata(config_dir().join("profiles.json")).and_then(|m| m.modified()).ok();
        if stamp != self.apps_read || stamp.is_none() {
            self.apps = Self::store().map(|s| s.apps()).unwrap_or_default();
            self.apps_read = stamp;
        }
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    pub fn unsupported(&self) -> &[Unsupported] {
        &self.unsupported
    }

    pub fn synapse(&self) -> &[String] {
        &self.synapse
    }

    /// Battery of every wireless device that answers.
    pub fn batteries(&self) -> Vec<Battery> {
        self.devices
            .iter()
            .filter_map(|d| {
                let percent = d.battery().ok().flatten()?;
                Some(Battery { key: d.key(), name: d.name(), percent, charging: d.charging().unwrap_or(false) })
            })
            .collect()
    }

    /// Re-enumerate devices. Returns the keys of devices that were not present before.
    pub fn rescan(&mut self) -> Vec<String> {
        let before: HashSet<String> = self.devices.iter().map(|d| d.key()).collect();
        self.devices.clear(); // close stale handles before reopening
        let _ = self.api.refresh_devices();
        let db = Db::load(Some(&config_dir().join("devices.json")))
            .or_else(|_| Db::builtin())
            .unwrap_or_default();
        self.devices = discover(&self.api, &db);
        self.unsupported = secbird_core::device::unanswered(&self.api, &self.devices)
            .into_iter()
            .map(|pid| match db.get(pid) {
                Some(spec) if secbird_core::headset::is_headset(pid) => {
                    Unsupported { pid, name: spec.name.clone(), reason: "No answer on its HID audio reports. Check it is on, and close Synapse" }
                }
                Some(spec) if spec.lighting == secbird_core::Lighting::Kraken => {
                    Unsupported { pid, name: spec.name.clone(), reason: "Headset protocol not supported yet" }
                }
                Some(spec) => Unsupported { pid, name: spec.name.clone(), reason: "Not answering. Is Synapse running?" },
                None => Unsupported { pid, name: format!("Razer device {pid:04X}"), reason: "Unknown model. Run secbird report" },
            })
            .collect();
        self.synapse = secbird_core::debloat::synapse_running();
        self.devices.iter().map(|d| d.key()).filter(|k| !before.contains(k)).collect()
    }

    /// Read profiles fresh from disk, so changes made with the CLI are picked up.
    fn store() -> Result<Store> {
        Store::load(&config_dir().join("profiles.json"))
    }

    /// Profile names for a device and which one is active.
    pub fn profiles(dev: &Device) -> (Vec<String>, String) {
        match Self::store() {
            Ok(mut store) => {
                let p = store.for_device(dev);
                (p.profiles.keys().cloned().collect(), p.active.clone())
            }
            Err(_) => (Vec::new(), String::new()),
        }
    }

    /// The profile a device shows right now (see [`Engine::set_foreground`]).
    pub fn effective_profile(&self, dev: &Device) -> String {
        Self::store().map(|mut s| s.for_device(dev).effective(self.foreground()).to_string()).unwrap_or_default()
    }

    /// Apply the current profile of each listed device (by [`Device::key`]): the one tied
    /// to the program in front, else the active one.
    pub fn apply(&self, keys: &[String]) -> Vec<ApplyReport> {
        let mut store = Self::store();
        self.devices
            .iter()
            .filter(|d| keys.contains(&d.key()))
            .map(|dev| match &mut store {
                Ok(store) => {
                    let p = store.for_device(dev);
                    let name = p.effective(self.foreground()).to_string();
                    let failures = p
                        .apply(&name, dev)
                        .into_iter()
                        .filter_map(|a| a.result.err().map(|e| format!("{}: {e}", a.setting)))
                        .collect();
                    ApplyReport { device: dev.name(), profile: name, failures }
                }
                Err(e) => ApplyReport {
                    device: dev.name(),
                    profile: String::new(),
                    failures: vec![format!("profiles.json: {e}")],
                },
            })
            .collect()
    }

    pub fn all_keys(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.key()).collect()
    }

    /// Make `name` the active profile for a device and apply it.
    /// Every mouse's remapped buttons back to what the mouse stores. Returns (device, count).
    pub fn restore_buttons(&self) -> Vec<(String, Result<usize>)> {
        let mut store = match Self::store() {
            Ok(s) => s,
            Err(e) => return vec![("profiles".into(), Err(e))],
        };
        self.devices
            .iter()
            .filter(|d| d.spec.as_ref().is_some_and(|s| s.kind == secbird_core::Kind::Mouse))
            .map(|d| (self.label(d), secbird_core::actions::restore_all_buttons(d, &mut store)))
            .collect()
    }

    pub fn switch_profile(&self, key: &str, name: &str) -> Result<Vec<ApplyReport>> {
        let dev = self
            .devices
            .iter()
            .find(|d| d.key() == key)
            .ok_or_else(|| secbird_core::Error::NotFound(format!("device {key}")))?;
        let mut store = Self::store()?;
        store.for_device(dev).switch(name)?;
        store.save()?;
        Ok(self.apply(&[key.to_string()]))
    }

    /// Menu/tooltip label: the name, plus a serial tail when several units share it.
    pub fn label(&self, dev: &Device) -> String {
        let twins = self.devices.iter().filter(|d| d.pid == dev.pid).count();
        match dev.serial.len() {
            n if twins > 1 && n > 0 => format!("{} ({})", dev.name(), &dev.serial[n.saturating_sub(4)..]),
            _ => dev.name(),
        }
    }
}
