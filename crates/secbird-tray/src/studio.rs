//! Streams each device's layered lighting ([`secbird_core::studio`]) while the tray runs.
//! Lives inside the [`crate::engine::Engine`], behind the same lock as everything else, and
//! runs on the tray's 30 fps timer.

use secbird_core::studio::{render, Canvas, Inputs, Kind, Press, Scene, FPS, WIRELESS_EVERY};
use secbird_core::{Device, Rgb, Store};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

/// Shared with the sensor threads: the latest sound level and screen colours.
#[derive(Default)]
pub struct Senses {
    pub audio: (f32, f32, f32),
    pub screen: Option<(usize, usize, Vec<Rgb>)>,
}

/// One device's scene while it streams.
struct Running {
    profile: String,
    scene: Scene,
    canvas: Canvas,
    /// Last frame sent; `None` until the device took the first one.
    last: Option<Vec<Rgb>>,
    /// Streams in a row that failed; the device is left alone after a few.
    failures: u32,
    wireless: bool,
    battery: Option<u8>,
}

pub struct Studio {
    clock: Instant,
    tick: u32,
    running: HashMap<String, Running>,
    /// profiles.json time when the scenes were last read.
    read_at: Option<SystemTime>,
    /// Re-read the scenes on the next tick (the editor just saved one).
    dirty: bool,
    presses: HashMap<String, Vec<Press>>,
    pub senses: Arc<Mutex<Senses>>,
    /// Devices whose scene just ended: the tray puts their own lighting back.
    pub ended: Vec<String>,
    /// Devices that stopped taking frames, for the log.
    pub gave_up: Vec<String>,
}

/// A device that misses this many frames in a row is asleep or doesn't take frames.
const GIVE_UP_AFTER: u32 = 30;
/// Read profiles.json for changes about once a second.
const READ_EVERY: u32 = FPS;
/// Static scenes and wired keyboards: send the whole frame again every few seconds, as
/// keyboards drop per-key colours in quick adjustment mode without saying so (#45).
const RESEND_EVERY: u32 = FPS * 3;
/// Battery read once a minute, on tick [`BATTERY_TICK`] of that minute. Wireless devices only
/// get every [`WIRELESS_EVERY`]th tick, so the read has to fall on one of those: on any other
/// it never happened, and a battery layer kept the level it had when the stream started.
const BATTERY_EVERY: u32 = FPS * 60;
const BATTERY_TICK: u32 = WIRELESS_EVERY;
const _: () = assert!(BATTERY_TICK < BATTERY_EVERY && BATTERY_EVERY.is_multiple_of(WIRELESS_EVERY) && BATTERY_TICK.is_multiple_of(WIRELESS_EVERY));

/// Whether this tick is the battery's turn.
fn battery_turn(tick: u32) -> bool {
    tick % BATTERY_EVERY == BATTERY_TICK
}

impl Default for Studio {
    fn default() -> Self {
        Studio {
            clock: Instant::now(),
            tick: 0,
            running: HashMap::new(),
            read_at: None,
            dirty: true,
            presses: HashMap::new(),
            senses: Arc::new(Mutex::new(Senses::default())),
            ended: Vec::new(),
            gave_up: Vec::new(),
        }
    }
}

impl Studio {
    pub fn now(&self) -> f32 {
        self.clock.elapsed().as_secs_f32()
    }

    /// Scenes changed (saved from the editor, a profile switch): read them on the next tick.
    pub fn reload(&mut self) {
        self.dirty = true;
    }

    /// Something was sent to these devices outside the stream (a profile was applied):
    /// send them a whole frame again, starting the stream over.
    pub fn restart(&mut self, keys: &[String]) {
        for key in keys {
            if let Some(r) = self.running.get_mut(key) {
                r.last = None;
                r.failures = 0;
            }
        }
        self.dirty = true;
    }

    pub fn is_streaming(&self, key: &str) -> bool {
        self.running.contains_key(key)
    }

    /// Which inputs some running scene needs: (key presses, sound, screen).
    pub fn wants(&self) -> (bool, bool, bool) {
        let any = |k: Kind| self.running.values().any(|r| r.scene.uses(k));
        (any(Kind::Reactive) || any(Kind::Ripple), any(Kind::Audio), any(Kind::Screen))
    }

    /// Whether the timer has anything to do.
    pub fn busy(&self) -> bool {
        !self.running.is_empty() || self.dirty
    }

    /// A key press or a click on the device with this product id. `names` are the key's
    /// names, the first one the device's layout has counts; none (a click, or a key the
    /// layout doesn't have) lights the whole device.
    pub fn press(&mut self, pid: u16, names: &[&str]) {
        let at = self.now();
        for (dev_key, r) in self.running.iter().filter(|(k, _)| k.starts_with(&format!("{pid:04X}"))) {
            let pixel = names.iter().find_map(|name| r.canvas.pixels.iter().position(|p| p.key.as_deref() == Some(*name)));
            let list = self.presses.entry(dev_key.clone()).or_default();
            list.retain(|p| at - p.at < 6.0);
            if list.len() >= 64 {
                list.remove(0);
            }
            list.push(Press { at, pixel });
        }
    }

    fn inputs(&self, key: &str, battery: Option<u8>) -> Inputs {
        let senses = self.senses.lock().unwrap_or_else(|p| p.into_inner());
        Inputs {
            presses: self.presses.get(key).cloned().unwrap_or_default(),
            audio: senses.audio,
            screen: senses.screen.clone(),
            battery,
        }
    }

    /// Re-read every device's scene when profiles.json changed. `foreground` picks the
    /// profile in use, as for everything else.
    fn read_scenes(&mut self, devices: &[Device], foreground: Option<&str>, path: &std::path::Path) {
        let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if !self.dirty && stamp == self.read_at {
            return;
        }
        self.dirty = false;
        self.read_at = stamp;
        let Ok(mut store) = Store::load(path) else { return };
        let mut next = HashMap::new();
        for dev in devices {
            let key = dev.key();
            let p = store.for_device(dev);
            if p.lighting_off {
                continue;
            }
            let name = p.effective(foreground).to_string();
            let Some(scene) = p.profiles.get(&name).and_then(|p| p.studio.clone()).filter(Scene::is_on) else {
                continue;
            };
            let canvas = Canvas::for_device(dev, &p.layout);
            if canvas.pixels.is_empty() {
                continue;
            }
            let wireless = dev.spec.as_ref().is_some_and(|s| s.wireless);
            match self.running.remove(&key) {
                // Same scene on the same lights: keep streaming without starting over.
                Some(r) if r.profile == name && r.scene == scene && r.canvas == canvas => {
                    next.insert(key, r);
                }
                old => {
                    let battery = old.and_then(|r| r.battery);
                    next.insert(key, Running { profile: name, scene, canvas, last: None, failures: 0, wireless, battery });
                }
            }
        }
        // Whatever is left stopped streaming: its device gets its own lighting back.
        self.ended.extend(self.running.keys().cloned());
        self.running = next;
    }

    /// One frame for every device with a scene. `away` and `paused` stop drawing (the
    /// lights are down, or the editor's light finder is showing one light).
    pub fn tick(&mut self, devices: &[Device], foreground: Option<&str>, path: &std::path::Path, away: bool, paused: bool) {
        self.tick = self.tick.wrapping_add(1);
        if self.dirty || self.running.is_empty() || self.tick.is_multiple_of(READ_EVERY) {
            self.read_scenes(devices, foreground, path);
        }
        if away || paused {
            // Start over when back: the device may have lost the stream meanwhile.
            for r in self.running.values_mut() {
                r.last = None;
            }
            return;
        }
        let t = self.now();
        let tick = self.tick;
        let keys: Vec<String> = self.running.keys().cloned().collect();
        for key in keys {
            let Some(dev) = devices.iter().find(|d| d.key() == key) else { continue };
            let (wireless, animated, started, battery_due) = {
                let r = &self.running[&key];
                (r.wireless, r.scene.animated(), r.last.is_some(), r.scene.uses(Kind::Battery) && (battery_turn(tick) || r.battery.is_none()))
            };
            if wireless && started && !tick.is_multiple_of(WIRELESS_EVERY) {
                continue;
            }
            // A failed start waits for replies that don't come: try again now and then,
            // not every frame, so other devices keep their pace.
            let failures = self.running[&key].failures;
            let backoff = if failures >= GIVE_UP_AFTER { FPS * 5 } else { FPS / 3 };
            if failures > 0 && !started && !tick.is_multiple_of(backoff) {
                continue;
            }
            if battery_due {
                let level = dev.battery().ok().flatten();
                if let Some(r) = self.running.get_mut(&key) {
                    r.battery = level.or(r.battery);
                }
            }
            let resend = !wireless && tick.is_multiple_of(RESEND_EVERY);
            if started && !animated && !resend {
                continue;
            }
            let input = self.inputs(&key, self.running[&key].battery);
            let r = self.running.get_mut(&key).expect("listed above");
            let frame = render(&r.scene, &r.canvas, t, &input);
            let result = match &r.last {
                None => dev.start_stream(&r.canvas, &frame),
                Some(_) if resend => dev.stream(&r.canvas, &frame, None),
                Some(last) => dev.stream(&r.canvas, &frame, Some(last)),
            };
            match result {
                Ok(()) => {
                    r.last = Some(frame);
                    r.failures = 0;
                }
                Err(_) => {
                    r.failures += 1;
                    if r.failures == GIVE_UP_AFTER {
                        self.gave_up.push(key.clone());
                    }
                    // Asleep or gone: try a whole frame again now and then.
                    r.last = None;
                }
            }
        }
    }

    /// Whether this device is skipped for now after failing ([`GIVE_UP_AFTER`]).
    pub fn failing(&self, key: &str) -> bool {
        self.running.get(key).is_some_and(|r| r.failures >= GIVE_UP_AFTER)
    }

    /// The editor's live view: what the device shows now, one colour per light, drawn
    /// from `scene` (the one being edited) or the running one.
    pub fn preview(&self, dev: &Device, layout: &[secbird_core::studio::KeyPos], scene: Option<&Scene>) -> Vec<Rgb> {
        let key = dev.key();
        let canvas = Canvas::for_device(dev, layout);
        let battery = self.running.get(&key).and_then(|r| r.battery);
        let input = self.inputs(&key, battery);
        match scene.or(self.running.get(&key).map(|r| &r.scene)) {
            Some(scene) => render(scene, &canvas, self.now(), &input),
            None => Vec::new(),
        }
    }

    /// Every streaming device, for exit: they get their own lighting back.
    pub fn stop_all(&mut self) -> Vec<String> {
        let keys = self.running.keys().cloned().collect();
        self.running.clear();
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secbird_core::sim::{Sim, WIRELESS_MOUSE_PID};
    use secbird_core::studio::Layer;

    /// A wireless device is skipped on every tick but each [`WIRELESS_EVERY`]th. The battery's
    /// turn used to be tick 1 of each minute, which is never one of those, so a battery layer
    /// kept showing the level from when its stream started while the device charged.
    #[test]
    fn a_wireless_device_gets_its_battery_read_again() {
        let sim = Sim::wireless_mouse();
        let dev = sim.open(WIRELESS_MOUSE_PID, Sim::spec_for(WIRELESS_MOUSE_PID)).expect("the simulated wireless mouse");
        let key = dev.key();
        let scene = Scene { layers: vec![Layer::parse(&["battery".to_string()]).expect("a battery layer")] };
        let canvas = Canvas::for_device(&dev, &[]);
        // A stream that started a minute ago, when the battery was at 75 %.
        let last = Some(vec![Rgb(0, 0, 0); canvas.pixels.len()]);
        let mut studio = Studio { dirty: false, tick: BATTERY_EVERY, ..Studio::default() };
        studio.running.insert(key.clone(), Running { profile: "Default".into(), scene, canvas, last, failures: 0, wireless: true, battery: Some(75) });
        sim.state().battery = Some(255); // charged since
        let devices = [dev];
        let no_profiles = std::path::Path::new("no-profiles.json");

        // Ticks 1801 and 1802 of this minute are not the device's; 1803 is, and the battery's.
        for (tick, level) in [(BATTERY_EVERY + 1, 75), (BATTERY_EVERY + 2, 75), (BATTERY_EVERY + 3, 100)] {
            studio.tick(&devices, None, no_profiles, false, false);
            assert_eq!((studio.tick, studio.running[&key].battery), (tick, Some(level)));
        }
    }
}
