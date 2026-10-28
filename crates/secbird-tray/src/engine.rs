//! Device and profile handling for the tray, kept free of Win32 code.

use secbird_core::{config_dir, discover, hidapi::HidApi, Db, Device, Dimming, Result, Store};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};

pub struct Engine {
    api: HidApi,
    devices: Vec<Device>,
    camera_backend: Box<dyn secbird_core::camera::CameraBackend>,
    cameras: Vec<secbird_core::camera::CameraInfo>,
    camera_error: Option<String>,
    profile_path: std::path::PathBuf,
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
    /// No per-key refresh until then, while the editor's light finder is open.
    frames_paused_until: Option<Instant>,
    /// Brightness percent while the PC is locked or its screen is off ([`Engine::lights_away`]).
    away: Option<u8>,
    /// Devices that didn't answer [`Engine::lights_back`], and until when to keep trying.
    relight: HashMap<String, Instant>,
}

const FINDER_PAUSE: Duration = Duration::from_secs(10 * 60);
/// A wireless keyboard can miss the "back" write and answer ~20 s later (#52).
const RELIGHT_FOR: Duration = Duration::from_secs(2 * 60);

/// Battery state of one wireless device.
pub struct Battery {
    pub key: String,
    pub name: String,
    pub percent: u8,
    pub charging: bool,
}

/// A connected Razer product that can't be controlled (yet), shown so it isn't silently missing.
/// Why a wireless device we know is quiet, most often: it sleeps until a keypress (#37).
const ASLEEP: &str = "Not answering. Asleep? Press a key or move it";
const BLUETOOTH: &str = "Connected over Bluetooth and not answering. Use the cable or the USB receiver";

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

pub struct ProfileDevice {
    pub id: String,
    pub label: String,
    pub profiles: Vec<String>,
    pub active: String,
}

impl Engine {
    /// The text of `secbird report`, for the tray's "Make a device report" item. Only reads.
    pub fn report_text(&mut self) -> String {
        let _ = self.api.refresh_devices();
        let db = Db::load(Some(&config_dir().join("devices.json"))).unwrap_or_default();
        secbird_core::report::build(&self.api, &db).to_string()
    }

    pub fn new() -> Result<Self> {
        let (camera_backend, camera_error) = match secbird_core::camera::backend() {
            Ok(backend) => (backend, None),
            Err(error) => (Box::new(FailedCamera(error.to_string())) as Box<dyn secbird_core::camera::CameraBackend>, Some(error.to_string())),
        };
        Ok(Self {
            api: HidApi::new()?,
            devices: Vec::new(),
            camera_backend,
            cameras: Vec::new(),
            camera_error,
            profile_path: config_dir().join("profiles.json"),
            unsupported: Vec::new(),
            synapse: Vec::new(),
            foreground: None,
            apps: HashSet::new(),
            apps_read: None,
            live_dpi: HashMap::new(),
            live_seq: 0,
            frames_paused_until: None,
            away: None,
            relight: HashMap::new(),
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
        if let Ok(mut store) = self.store() {
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
        let Ok(mut store) = self.store() else { return Vec::new() };
        let changed: Vec<String> = self
            .devices
            .iter()
            .filter(|d| {
                let p = store.for_device(d);
                p.effective(before.as_deref()) != p.effective(self.foreground.as_deref())
            })
            .map(|d| d.key())
            .collect();
        let mut changed = changed;
        for camera in &self.cameras {
            let profiles = store.for_camera(&camera.id);
            if profiles.effective(before.as_deref()) != profiles.effective(self.foreground.as_deref()) {
                changed.push(camera.id.clone());
            }
        }
        self.apply(&changed)
    }

    /// Re-read the tied programs only when profiles.json has changed: a foreground switch
    /// then costs one file-time lookup.
    fn refresh_apps(&mut self) {
        let stamp = std::fs::metadata(&self.profile_path).and_then(|m| m.modified()).ok();
        if stamp != self.apps_read || stamp.is_none() {
            self.apps = self.store().map(|s| s.apps()).unwrap_or_default();
            self.apps_read = stamp;
        }
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    pub fn cameras(&self) -> &[secbird_core::camera::CameraInfo] { &self.cameras }
    pub fn camera_backend(&self) -> &dyn secbird_core::camera::CameraBackend { self.camera_backend.as_ref() }
    pub fn camera_error(&self) -> Option<&str> { self.camera_error.as_deref() }

    pub fn profile_devices(&self) -> Vec<ProfileDevice> {
        let mut out: Vec<_> = self.devices.iter().map(|device| {
            let (profiles, active) = Self::profiles(device);
            ProfileDevice { id: device.key(), label: self.label(device), profiles, active }
        }).collect();
        let mut store = self.store().unwrap_or_default();
        for camera in &self.cameras {
            let profiles = store.for_camera(&camera.id);
            let label = crate::camera::label(camera, &self.cameras);
            out.push(ProfileDevice { id: camera.id.clone(), label,
                profiles: profiles.profiles.keys().cloned().collect(), active: profiles.active.clone() });
        }
        out
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

    /// Zones showing a wireless mouse's battery (a dock's light, #39) follow its level.
    /// Failures are left out: a mouse off its dock or asleep doesn't answer, and the
    /// colour stays until the next check.
    pub fn refresh_battery_lights(&self) {
        let Ok(mut store) = Self::store() else { return };
        for dev in self.devices.iter().filter(|d| d.spec.as_ref().is_some_and(|s| s.wireless)) {
            let p = store.for_device(dev);
            let name = p.effective(self.foreground()).to_string();
            p.refresh_battery_lights(&name, dev);
        }
    }

    /// Per-key colors of wired keyboards and keypads, sent again. Leaving quick adjustment
    /// mode (#45) leaves them dark, and the keyboard doesn't say when; Synapse streams
    /// frames all the time, so this does the same at a slow pace. Wireless boards are left
    /// alone: the traffic would keep them awake. Failures are left out, as for batteries.
    /// Not while away: the lights are down then anyway.
    pub fn refresh_frames(&self) {
        if self.away.is_some() || self.frames_paused_until.is_some_and(|t| Instant::now() < t) {
            return;
        }
        let Ok(mut store) = Self::store() else { return };
        let wired_boards = self.devices.iter().filter(|d| {
            d.matrix().is_some()
                && d.spec.as_ref().is_some_and(|s| {
                    !s.wireless && matches!(s.kind, secbird_core::Kind::Keyboard | secbird_core::Kind::Keypad)
                })
        });
        for dev in wired_boards {
            let p = store.for_device(dev);
            let name = p.effective(self.foreground()).to_string();
            p.refresh_frame(&name, dev);
        }
    }

    /// Laptops set to "stays lit" still fade by themselves (#34), but any read restarts
    /// that idle fade, so read the setting back now and then. Only while the tray runs.
    pub fn keep_laptops_lit(&self) {
        if self.away.is_some() {
            return;
        }
        let Ok(mut store) = Self::store() else { return };
        let laptops = self.devices.iter().filter(|d| d.spec.as_ref().is_some_and(|s| s.kind == secbird_core::Kind::Laptop));
        for dev in laptops {
            let p = store.for_device(dev);
            let name = p.effective(self.foreground()).to_string();
            let stays_lit = Dimming::StaysLit.to_string();
            if p.profiles.get(&name).is_some_and(|p| p.dimming.as_deref() == Some(stays_lit.as_str())) {
                let _ = dev.dimming();
            }
        }
    }

    /// The PC was locked or its screen went off (#48): every zone this app controls goes
    /// to `percent` of its saved brightness, with writes that aren't saved, so a replug or
    /// power cycle brings the lights back. Also call it after a rescan while away, for
    /// devices that just arrived. Devices with the older lighting commands are left lit.
    pub fn lights_away(&mut self, percent: u8) {
        self.away = Some(percent);
        self.relight.clear();
        self.each_zone(|dev, led| {
            let saved = dev.brightness(led)?;
            dev.set_brightness_live(led, (saved as u32 * percent as u32 / 100) as u8)
        });
    }

    /// Back from [`Engine::lights_away`]: the saved brightness again, read from the device
    /// (it answers the saved level, not the dimmed one), so it works after a tray restart too.
    /// Then the profiles, in case one changed while away.
    pub fn lights_back(&mut self) -> Vec<ApplyReport> {
        if self.away.take().is_none() {
            return Vec::new();
        }
        let missed = self.each_zone(|dev, led| dev.set_brightness_live(led, dev.brightness(led)?));
        let until = Instant::now() + RELIGHT_FOR;
        self.relight = missed.into_iter().map(|key| (key, until)).collect();
        self.apply(&self.all_keys())
    }

    /// [`Engine::lights_back`] again for the devices that missed it, until they answer or
    /// [`RELIGHT_FOR`] runs out.
    pub fn retry_lights_back(&mut self) {
        let now = Instant::now();
        self.relight.retain(|_, until| now < *until);
        if self.relight.is_empty() || self.away.is_some() {
            return;
        }
        let missed = self.each_zone(|dev, led| {
            if !self.relight.contains_key(&dev.key()) {
                return Ok(());
            }
            dev.set_brightness_live(led, dev.brightness(led)?)
        });
        let back: Vec<String> = self.relight.keys().filter(|k| !missed.contains(k)).cloned().collect();
        for key in &back {
            self.relight.remove(key);
        }
        if !back.is_empty() {
            self.apply(&back);
        }
    }

    pub fn is_away(&self) -> bool {
        self.away.is_some()
    }

    /// Run `f` on each lighting zone of each device whose lighting this app controls.
    /// Returns the keys of the devices where `f` failed: an asleep wireless device doesn't answer.
    fn each_zone(&self, f: impl Fn(&Device, u8) -> Result<()>) -> Vec<String> {
        let Ok(mut store) = Self::store() else { return Vec::new() };
        let mut failed = Vec::new();
        for dev in &self.devices {
            if store.for_device(dev).lighting_off {
                continue;
            }
            let leds: Vec<u8> = dev.spec.iter().flat_map(|s| s.leds.iter().map(|l| l.id)).collect();
            if leds.into_iter().map(|led| f(dev, led)).filter(Result::is_err).count() > 0 {
                failed.push(dev.key());
            }
        }
        failed
    }

    /// The editor's light finder shows one light at a time; don't paint over it. A finder
    /// left open (a closed browser never says) stops pausing after a while.
    pub fn pause_frames(&mut self, pause: bool) {
        self.frames_paused_until = pause.then(|| Instant::now() + FINDER_PAUSE);
    }

    /// Re-enumerate devices. Returns the keys of devices that were not present before.
    pub fn rescan(&mut self) -> Vec<String> {
        let before: HashSet<String> = self.all_keys().into_iter().collect();
        self.devices.clear(); // close stale handles before reopening
        let _ = self.api.refresh_devices();
        let db = Db::load(Some(&config_dir().join("devices.json")))
            .or_else(|_| Db::builtin())
            .unwrap_or_default();
        self.devices = discover(&self.api, &db);
        match self.camera_backend.discover() {
            Ok(cameras) => { self.cameras = cameras; self.camera_error = None; }
            Err(error) => { self.cameras.clear(); self.camera_error = Some(error.to_string()); }
        }
        self.unsupported = secbird_core::device::unanswered(&self.api, &self.devices)
            .into_iter()
            .filter(|pid| !db.get(*pid).is_some_and(|spec| spec.kind == secbird_core::Kind::Webcam))
            .map(|pid| match db.get(pid) {
                Some(spec) if spec.lighting == secbird_core::Lighting::Kraken => {
                    Unsupported { pid, name: spec.name.clone(), reason: "Headset protocol not supported yet" }
                }
                Some(spec) if spec.wireless => Unsupported { pid, name: spec.name.clone(), reason: ASLEEP },
                Some(spec) => Unsupported { pid, name: spec.name.clone(), reason: "Not answering. Is Synapse running?" },
                None => match db.bluetooth_name(pid) {
                    Some(name) => Unsupported { pid, name, reason: BLUETOOTH },
                    None => Unsupported { pid, name: format!("Razer device {pid:04X}"), reason: "Unknown model. Run secbird report" },
                },
            })
            .collect();
        #[cfg(windows)]
        { self.synapse = secbird_core::debloat::synapse_running(); }
        self.all_keys().into_iter().filter(|key| !before.contains(key)).collect()
    }

    /// A known wireless device is plugged in (its receiver is) but not answering yet.
    pub fn has_sleeping(&self) -> bool {
        self.unsupported.iter().any(|u| u.reason == ASLEEP)
    }

    /// Read profiles fresh from disk, so changes made with the CLI are picked up.
    fn store(&self) -> Result<Store> {
        Store::load(&self.profile_path)
    }

    /// Profile names for a device and which one is active.
    pub fn profiles(dev: &Device) -> (Vec<String>, String) {
        match Store::load(&config_dir().join("profiles.json")) {
            Ok(mut store) => {
                let p = store.for_device(dev);
                (p.profiles.keys().cloned().collect(), p.active.clone())
            }
            Err(_) => (Vec::new(), String::new()),
        }
    }

    /// The profile a device shows right now (see [`Engine::set_foreground`]).
    pub fn effective_profile(&self, dev: &Device) -> String {
        self.store().map(|mut s| s.for_device(dev).effective(self.foreground()).to_string()).unwrap_or_default()
    }

    /// Apply the current profile of each listed device (by [`Device::key`]): the one tied
    /// to the program in front, else the active one.
    pub fn apply(&self, keys: &[String]) -> Vec<ApplyReport> {
        let mut store = self.store();
        let mut reports: Vec<_> = self.devices
            .iter()
            .filter(|d| keys.contains(&d.key()))
            .map(|dev| match &mut store {
                Ok(store) => {
                    let p = store.for_device(dev);
                    // Lights stay down while away (a lock screen in front switches profiles);
                    // this copy of the store is never saved.
                    p.lighting_off |= self.away.is_some();
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
            .collect();
        for camera in self.cameras.iter().filter(|camera| keys.contains(&camera.id)) {
            match &mut store {
                Ok(store) => {
                    let profiles = store.for_camera(&camera.id);
                    let name = profiles.effective(self.foreground()).to_owned();
                    let failures = secbird_core::camera::actions::apply(self.camera_backend(), &camera.id, &profiles.profiles[&name])
                        .into_iter().filter_map(|item| item.result.err().map(|e| format!("{}: {e}", item.setting))).collect();
                    reports.push(ApplyReport { device: camera.name.clone(), profile: name, failures });
                }
                Err(error) => reports.push(ApplyReport { device: camera.name.clone(), profile: String::new(), failures: vec![error.to_string()] }),
            }
        }
        reports
    }

    /// Wireless mice that dropped their button remaps while asleep (#45) get them back.
    /// `skip` holds keys re-sent moments ago, left alone so a mouse that keeps losing
    /// them isn't written over and over. Returns the keys re-sent, with their reports.
    pub fn restore_lost_buttons(&self, skip: &HashSet<String>) -> Vec<(String, ApplyReport)> {
        let Ok(mut store) = self.store() else { return Vec::new() };
        self.devices
            .iter()
            .filter(|d| d.spec.as_ref().is_some_and(|s| s.wireless && s.kind == secbird_core::Kind::Mouse))
            .filter(|d| !skip.contains(&d.key()))
            .filter_map(|dev| {
                let p = store.for_device(dev);
                let name = p.effective(self.foreground()).to_string();
                if !p.buttons_lost(&name, dev) {
                    return None;
                }
                let failures = p
                    .apply_buttons(&name, dev)
                    .into_iter()
                    .filter_map(|a| a.result.err().map(|e| format!("{}: {e}", a.setting)))
                    .collect();
                Some((dev.key(), ApplyReport { device: self.label(dev), profile: name, failures }))
            })
            .collect()
    }

    pub fn all_keys(&self) -> Vec<String> {
        self.devices.iter().map(|d| d.key()).chain(self.cameras.iter().map(|camera| camera.id.clone())).collect()
    }

    /// Make `name` the active profile for a device and apply it.
    /// Every mouse's remapped buttons back to what the mouse stores. Returns (device, count).
    pub fn restore_buttons(&self) -> Vec<(String, Result<usize>)> {
        let mut store = match self.store() {
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
        if self.cameras.iter().any(|camera| camera.id == key) {
            let mut store = self.store()?;
            store.for_camera(key).switch(name)?;
            store.save()?;
            return Ok(self.apply(&[key.to_owned()]));
        }
        let dev = self
            .devices
            .iter()
            .find(|d| d.key() == key)
            .ok_or_else(|| secbird_core::Error::NotFound(format!("device {key}")))?;
        let mut store = self.store()?;
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

struct FailedCamera(String);
impl secbird_core::camera::CameraBackend for FailedCamera {
    fn discover(&self) -> Result<Vec<secbird_core::camera::CameraInfo>> { Err(secbird_core::Error::System(self.0.clone())) }
    fn set(&self, _: &str, _: secbird_core::camera::Setting) -> Result<()> { Err(secbird_core::Error::System(self.0.clone())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secbird_core::camera::{CameraBackend, CameraInfo, CameraSettings, Setting};
    struct Fake;
    impl CameraBackend for Fake {
        fn discover(&self) -> Result<Vec<CameraInfo>> { Err(secbird_core::Error::System("camera access denied".into())) }
        fn set(&self, _: &str, _: Setting) -> Result<()> { Err(secbird_core::Error::System("camera disconnected".into())) }
    }

    #[test]
    fn camera_failure_does_not_block_hid_and_foreground_profiles_apply() {
        let path = std::env::temp_dir().join(format!("secbird-engine-{}-{}.json", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let mut engine = Engine::new().unwrap();
        engine.profile_path = path.clone();
        engine.camera_backend = Box::new(Fake);
        engine.cameras = vec![CameraInfo { id: "camera:test".into(), name: "Kiyo Pro".into(), pid: 0x0e05, control_error: None }];
        assert_eq!(engine.all_keys(), vec!["camera:test"]);
        let sim = secbird_core::sim::Sim::mouse();
        let device = sim.open(secbird_core::sim::MOUSE_PID, secbird_core::sim::Sim::spec_for(secbird_core::sim::MOUSE_PID)).unwrap();
        let key = device.key();
        engine.devices.push(device);
        let mut store = engine.store().unwrap();
        store.for_device(&engine.devices[0]).active_mut().dpi = Some((1600, 1600));
        let profiles = store.for_camera("camera:test");
        profiles.active_mut().camera = Some(CameraSettings { hdr: Some(true), ..Default::default() });
        profiles.create("Streaming").unwrap();
        profiles.set_apps("Streaming", &["obs.exe".into()]).unwrap();
        store.save().unwrap();
        let reports = engine.apply(&engine.all_keys());
        assert_eq!(reports.len(), 2);
        assert!(reports[0].failures.is_empty());
        assert!(!reports[1].failures.is_empty());
        assert_eq!(engine.devices[0].key(), key);
        assert_eq!(engine.devices[0].dpi().unwrap(), (1600, 1600));
        let switched = engine.set_foreground(Some("obs.exe".into()));
        assert_eq!(switched.len(), 1);
        assert_eq!(switched[0].profile, "Streaming");
        assert_eq!(engine.set_foreground(None)[0].profile, "Default");
        engine.rescan();
        assert!(engine.camera_error().unwrap().contains("access denied"));
        std::fs::remove_file(path).unwrap();
    }
}
