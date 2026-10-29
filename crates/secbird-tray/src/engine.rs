//! Device and profile handling for the tray, kept free of Win32 code.

use crate::wake_sync::Peer;
use secbird_core::{config_dir, discover, effects::Effect, hidapi::HidApi, Db, Device, Dimming, Result, Store};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime};

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
    /// No per-key refresh until then, while the editor's light finder is open.
    frames_paused_until: Option<Instant>,
    /// Brightness percent while the PC is locked or its screen is off ([`Engine::lights_away`]).
    away: Option<u8>,
    /// Devices that didn't answer [`Engine::lights_back`], and until when to keep trying.
    relight: HashMap<String, Instant>,
    /// Devices that didn't answer [`Engine::lights_away`], and until when to keep trying.
    dim: HashMap<String, Instant>,
}

const FINDER_PAUSE: Duration = Duration::from_secs(10 * 60);
/// A wireless keyboard can miss the "back" write and answer ~20 s later (#52).
pub const RELIGHT_FOR: Duration = Duration::from_secs(2 * 60);
/// A device can miss the "away" write too (a Basilisk V4 HyperSpeed often does, right as the
/// screen goes off) and then stays lit the whole time: try again for a while.
pub const DIM_FOR: Duration = Duration::from_secs(30);

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

/// What a retry of [`Engine::lights_away`] or [`Engine::lights_back`] did, by device key.
pub struct LightsRetry {
    /// Took the write this time.
    pub done: Vec<String>,
    /// Still not answering when the time ran out: no more tries.
    pub gave_up: Vec<String>,
}

/// One zone written by [`Engine::restart_spectrum`].
pub struct ZoneWrite {
    pub pid: u16,
    pub zone: String,
    pub result: Result<()>,
    pub took: Duration,
}

/// For the log: `02C9 backlight ok (31 ms), 00EA underglow failed: ... (160 ms)`.
pub fn describe_writes(writes: &[ZoneWrite]) -> String {
    if writes.is_empty() {
        return "nothing written".into();
    }
    let parts: Vec<String> = writes
        .iter()
        .map(|w| {
            let ms = w.took.as_millis();
            match &w.result {
                Ok(()) => format!("{:04X} {} ok ({ms} ms)", w.pid, w.zone),
                Err(e) => format!("{:04X} {} failed: {e} ({ms} ms)", w.pid, w.zone),
            }
        })
        .collect();
    parts.join(", ")
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
            frames_paused_until: None,
            away: None,
            relight: HashMap::new(),
            dim: HashMap::new(),
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

    /// Devices whose profile in use shows Spectrum on a lit zone, by product id. `Err` says
    /// why none can be written now.
    pub fn spectrum_pids(&self) -> std::result::Result<Vec<u16>, String> {
        if self.away.is_some() {
            return Err("lights are down while away".into());
        }
        let mut store = Self::store().map_err(|e| format!("profiles.json: {e}"))?;
        let mut pids = Vec::new();
        for dev in &self.devices {
            if !pids.contains(&dev.pid) && !self.spectrum_zones(&mut store, dev).is_empty() {
                pids.push(dev.pid);
            }
        }
        Ok(pids)
    }

    /// LED id and name of each zone of `dev` that shows Spectrum in its profile in use.
    fn spectrum_zones(&self, store: &mut Store, dev: &Device) -> Vec<(u8, String)> {
        let p = store.for_device(dev);
        if p.lighting_off {
            return Vec::new();
        }
        let name = p.effective(self.foreground()).to_string();
        let (Some(profile), Some(spec)) = (p.profiles.get(&name), &dev.spec) else { return Vec::new() };
        spec.leds
            .iter()
            .filter(|led| {
                profile.lighting.get(&led.name).is_some_and(|z| !z.off && z.effect.as_ref() == Some(&Effect::Spectrum))
            })
            .map(|led| (led.id, led.name.clone()))
            .collect()
    }

    /// Restart the hardware Spectrum of the devices in `order`, in that order, without
    /// changing any profile. The first `strict` of them just woke: when one of those doesn't
    /// take the write, stop, so the rest aren't restarted out of step with it. Returns
    /// every write made.
    pub fn restart_spectrum(&self, order: &[u16], strict: usize) -> Vec<ZoneWrite> {
        let Ok(mut store) = Self::store() else { return Vec::new() };
        let mut writes = Vec::new();
        for (i, &pid) in order.iter().enumerate() {
            for dev in self.devices.iter().filter(|d| d.pid == pid) {
                for (led, zone) in self.spectrum_zones(&mut store, dev) {
                    let start = Instant::now();
                    let result = dev.set_effect(led, &Effect::Spectrum);
                    let stop = result.is_err() && i < strict;
                    writes.push(ZoneWrite { pid, zone, result, took: start.elapsed() });
                    if stop {
                        return writes;
                    }
                }
            }
        }
        writes
    }

    /// Devices whose Raw Input may wake a Spectrum sync: those found, with the sleep timer
    /// their profile sets, and the known wireless ones plugged in but asleep.
    pub fn wake_peers(&self) -> HashMap<u16, Peer> {
        let mut store = Self::store().ok();
        let mut peers = HashMap::new();
        for dev in &self.devices {
            let minutes = store.as_mut().and_then(|s| {
                let p = s.for_device(dev);
                let name = p.effective(self.foreground()).to_string();
                p.profiles.get(&name)?.sleep_minutes
            });
            let sleep = minutes.filter(|m| *m > 0).map(|m| Duration::from_secs(u64::from(m) * 60));
            let wireless = dev.spec.as_ref().is_some_and(|s| s.wireless);
            peers.insert(dev.pid, Peer { wireless, sleep });
        }
        for asleep in self.unsupported.iter().filter(|u| u.reason == ASLEEP) {
            peers.entry(asleep.pid).or_insert(Peer { wireless: true, sleep: None });
        }
        peers
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
    /// Returns the keys of the devices that missed it: [`Engine::retry_lights_away`] tries
    /// them again for [`DIM_FOR`].
    pub fn lights_away(&mut self, percent: u8) -> Vec<String> {
        self.away = Some(percent);
        self.relight.clear();
        let missed = self.each_zone(|dev, led| dim_zone(dev, led, percent));
        let until = Instant::now() + DIM_FOR;
        self.dim = missed.iter().map(|key| (key.clone(), until)).collect();
        missed
    }

    /// [`Engine::lights_away`] again for the devices that missed it, while still away, until
    /// they answer or [`DIM_FOR`] runs out.
    pub fn retry_lights_away(&mut self) -> LightsRetry {
        let gave_up = expire(&mut self.dim, Instant::now());
        let Some(percent) = self.away.filter(|_| !self.dim.is_empty()) else {
            return LightsRetry { done: Vec::new(), gave_up };
        };
        let missed = self.each_zone(|dev, led| {
            if !self.dim.contains_key(&dev.key()) {
                return Ok(());
            }
            dim_zone(dev, led, percent)
        });
        let done: Vec<String> = self.dim.keys().filter(|k| !missed.contains(k) && self.has(k)).cloned().collect();
        for key in &done {
            self.dim.remove(key);
        }
        LightsRetry { done, gave_up }
    }

    /// Back from [`Engine::lights_away`]: the saved brightness again, read from the device
    /// (it answers the saved level, not the dimmed one), so it works after a tray restart too.
    /// Then the profiles, in case one changed while away. Returns the keys of the devices
    /// that missed it: [`Engine::retry_lights_back`] tries them again for [`RELIGHT_FOR`].
    pub fn lights_back(&mut self) -> Vec<String> {
        if self.away.take().is_none() {
            return Vec::new();
        }
        self.dim.clear();
        let missed = self.each_zone(|dev, led| dev.set_brightness_live(led, dev.brightness(led)?));
        let until = Instant::now() + RELIGHT_FOR;
        self.relight = missed.iter().map(|key| (key.clone(), until)).collect();
        self.apply(&self.all_keys());
        missed
    }

    /// [`Engine::lights_back`] again for the devices that missed it, until they answer or
    /// [`RELIGHT_FOR`] runs out. Those that answer get their profile again.
    pub fn retry_lights_back(&mut self) -> LightsRetry {
        let gave_up = expire(&mut self.relight, Instant::now());
        if self.relight.is_empty() || self.away.is_some() {
            return LightsRetry { done: Vec::new(), gave_up };
        }
        let missed = self.each_zone(|dev, led| {
            if !self.relight.contains_key(&dev.key()) {
                return Ok(());
            }
            dev.set_brightness_live(led, dev.brightness(led)?)
        });
        let done: Vec<String> = self.relight.keys().filter(|k| !missed.contains(k) && self.has(k)).cloned().collect();
        for key in &done {
            self.relight.remove(key);
        }
        if !done.is_empty() {
            self.apply(&done);
        }
        LightsRetry { done, gave_up }
    }

    /// Whether the device with this key is in the list.
    fn has(&self, key: &str) -> bool {
        self.devices.iter().any(|d| d.key() == key)
    }

    /// Device names for the log, without serial numbers.
    pub fn names(&self, keys: &[String]) -> String {
        let names: Vec<String> = keys
            .iter()
            .map(|key| match self.devices.iter().find(|d| d.key() == *key) {
                Some(dev) => self.label(dev),
                None => key.split(':').next().unwrap_or_default().to_string(),
            })
            .collect();
        names.join(", ")
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
        self.synapse = secbird_core::debloat::synapse_running();
        self.devices.iter().map(|d| d.key()).filter(|k| !before.contains(k)).collect()
    }

    /// A known wireless device is plugged in (its receiver is) but not answering yet.
    pub fn has_sleeping(&self) -> bool {
        self.unsupported.iter().any(|u| u.reason == ASLEEP)
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
            .collect()
    }

    /// Wireless mice that dropped their button remaps while asleep (#45) get them back.
    /// `skip` holds keys re-sent moments ago, left alone so a mouse that keeps losing
    /// them isn't written over and over. Returns the keys re-sent, with their reports.
    pub fn restore_lost_buttons(&self, skip: &HashSet<String>) -> Vec<(String, ApplyReport)> {
        let Ok(mut store) = Self::store() else { return Vec::new() };
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

/// One zone to `percent` of its saved brightness, not saved on the device.
fn dim_zone(dev: &Device, led: u8, percent: u8) -> Result<()> {
    let saved = dev.brightness(led)?;
    dev.set_brightness_live(led, (saved as u32 * percent as u32 / 100) as u8)
}

/// Drop the entries whose time ran out, and return their keys.
fn expire(until: &mut HashMap<String, Instant>, now: Instant) -> Vec<String> {
    let mut gone: Vec<String> = until.iter().filter(|(_, t)| now >= **t).map(|(key, _)| key.clone()).collect();
    gone.sort();
    for key in &gone {
        until.remove(key);
    }
    gone
}

#[cfg(test)]
mod tests {
    use super::expire;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn retries_run_out_one_device_at_a_time() {
        let now = Instant::now();
        let mut until = HashMap::from([("00EA:A".to_string(), now + Duration::from_secs(30)), ("02C9:B".to_string(), now)]);
        assert_eq!(expire(&mut until, now), ["02C9:B"]);
        assert_eq!(until.len(), 1);
        assert!(expire(&mut until, now + Duration::from_secs(29)).is_empty());
        assert_eq!(expire(&mut until, now + Duration::from_secs(30)), ["00EA:A"]);
        assert!(until.is_empty());
    }
}
