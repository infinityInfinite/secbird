//! `secbird report`: a read-only description of connected Razer hardware that users
//! can share, so devices we don't own can be added to the database.
//!
//! Only GET commands are sent. Nothing on the device changes.

use crate::commands as cmd;
use crate::db::{Db, RAZER_VID};
use crate::device::{Device, DOCK_LED, DOCK_TXID, TXID_CANDIDATES};
use crate::error::Error;
use crate::proto::Command;
use hidapi::HidApi;
use serde::Serialize;

/// One Razer HID interface as Windows lists it.
#[derive(Debug, Clone, Serialize)]
pub struct Interface {
    pub pid: String,
    pub interface: i32,
    pub usage_page: String,
    pub usage: String,
    /// Transaction id this interface answered Protocol 2.5 with, if any.
    pub txid: Option<String>,
    /// Feature report id it answered under; 0 over USB (see `NumberedReport`).
    pub report_id: Option<u8>,
    /// Feature reports its HID descriptor declares: (report id, bytes).
    pub features: Vec<(u8, usize)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    pub name: &'static str,
    pub command: String,
    /// "ok", or why it failed ("NotSupported", "no reply", ...).
    pub status: String,
    /// Reply arguments in hex, trailing zeros trimmed.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reply: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceReport {
    pub pid: String,
    pub name: String,
    pub in_database: bool,
    pub simulated: bool,
    pub interface: i32,
    pub txid: String,
    /// Feature report id the commands went under; 0 over USB.
    pub report_id: u8,
    /// Shortened for privacy.
    pub serial: String,
    pub probes: Vec<Probe>,
    /// LED ids that answered a brightness GET.
    pub leds: Vec<u8>,
    /// Mouse docks only: the same kind of reads sent to the dock itself (see [`DOCK_TXID`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dock: Vec<Probe>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dock_leds: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub secbird: &'static str,
    pub os: String,
    pub interfaces: Vec<Interface>,
    pub devices: Vec<DeviceReport>,
    pub cameras: Vec<CameraReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CameraReport {
    pub id: String,
    pub name: String,
    pub pid: String,
    pub error: Option<String>,
}

/// Read-only GETs: (name, command, size, args).
const PROBES: &[(&str, Command, u8, &[u8])] = &[
    ("firmware", cmd::FIRMWARE.get(), cmd::FIRMWARE_SIZE, &[]),
    ("firmware (full)", cmd::FIRMWARE_EXT.get(), cmd::FIRMWARE_EXT_SIZE, &[]),
    ("device mode", cmd::DEVICE_MODE.get(), cmd::DEVICE_MODE_SIZE, &[]),
    ("active on-board slot", cmd::ACTIVE_SLOT.get(), 1, &[]),
    ("polling rate", cmd::POLLING.get(), cmd::POLLING_SIZE, &[]),
    ("polling rate (high speed)", cmd::POLLING_HS.get(), cmd::POLLING_HS_SIZE, &[]),
    ("dpi", cmd::DPI.get(), cmd::DPI_SIZE, &[cmd::NOSTORE]),
    ("dpi stages", cmd::DPI_STAGES.get(), cmd::DPI_STAGES_SIZE, &[cmd::VARSTORE]),
    ("battery", cmd::BATTERY.get(), cmd::POWER_SIZE, &[]),
    ("charging", cmd::CHARGING.get(), cmd::POWER_SIZE, &[]),
    ("idle time", cmd::IDLE_TIME.get(), cmd::IDLE_TIME_SIZE, &[]),
    ("auto sleep", cmd::AUTO_SLEEP.get(), cmd::AUTO_SLEEP_SIZE, &[]),
    ("paired mice", cmd::WIRELESS_DEVICES.get(), cmd::WIRELESS_DEVICES_SIZE, &[]),
    ("macro storage", cmd::MACRO_STORAGE.get(), cmd::MACRO_STORAGE_SIZE, &[]),
    ("macro count", cmd::MACRO_COUNT.get(), cmd::MACRO_COUNT_SIZE, &[]),
    ("button ids", cmd::BUTTON_IDS.get(), cmd::BUTTON_IDS_SIZE, &[]),
    ("sensor rotation", cmd::SENSOR_ANGLE.get(), cmd::SENSOR_ANGLE_SIZE, &[cmd::VARSTORE]),
    ("gaming mode", cmd::GAME_MODE.get(), cmd::GAME_MODE_SIZE, &[]),
];

/// Keyboard key remapping (02:0D), which Synapse defines but never sends: whether the
/// keyboard answers the key list and a read of key A (31), in the live slot and slot 1.
const KEY_PROBES: &[(&str, Command, u8, &[u8])] = &[
    ("key ids", cmd::KEY_IDS, cmd::KEY_SIZE, &[1, 0, 0]),
    ("key ids (live slot)", cmd::KEY_IDS, cmd::KEY_SIZE, &[0, 0, 0]),
    ("key A (live slot)", cmd::KEY.get(), cmd::KEY_SIZE, &[0, 31, 0]),
    ("key A (slot 1)", cmd::KEY.get(), cmd::KEY_SIZE, &[1, 31, 0]),
    ("key A HyperShift (slot 1)", cmd::KEY.get(), cmd::KEY_SIZE, &[1, 31, 1]),
];

/// Laptop fan and power reads (Synapse's thermal module, class 0D, and power class 07), for
/// Blades and devices not in the database. Read-only like the rest: fan ids are 1 (CPU) and
/// 2 (GPU), the first argument is Synapse's "class id", always 0.
const LAPTOP_PROBES: &[(&str, Command, u8, &[u8])] = &[
    ("fan ids", Command::new(0x0D, 0x80), 0x50, &[]),
    ("fan 1 mode", Command::new(0x0D, 0x82), 0x04, &[0, 1]),
    ("fan 2 mode", Command::new(0x0D, 0x82), 0x04, &[0, 2]),
    ("fan 1 speed setting", Command::new(0x0D, 0x81), 0x03, &[0, 1]),
    ("fan 2 speed setting", Command::new(0x0D, 0x81), 0x03, &[0, 2]),
    ("fan 1 current speed", Command::new(0x0D, 0x88), 0x03, &[0, 1]),
    ("fan 2 current speed", Command::new(0x0D, 0x88), 0x03, &[0, 2]),
    ("advanced fan mode", Command::new(0x0D, 0x87), 0x03, &[0, 1]),
    ("cooling mode", Command::new(0x0D, 0x90), 0x03, &[0]),
    ("fan duty cycle", Command::new(0x0D, 0x8B), 0x02, &[]),
    ("gpu mode", Command::new(0x0D, 0x89), 0x02, &[]),
    ("sku", Command::new(0x0D, 0x8F), 0x01, &[]),
    ("cpu boost / power mode", Command::new(0x07, 0x8F), 0x01, &[]),
    ("charge limiter", Command::new(0x07, 0x92), 0x01, &[]),
    ("adapter wattage", Command::new(0x07, 0x8C), 0x02, &[]),
    // Profile 1, as Synapse asks; profile 0 answers all zeros (#20).
    ("smart dimming", Command::new(0x07, 0x93), 0x03, &[1]),
    ("time to dim", Command::new(0x07, 0x8B), 0x05, &[0]),
    // Keyboard backlight level, three ways: the Blade-only brightness (class 0E), and the
    // newer and older lighting commands on the backlight LED. A report taken while the
    // keyboard is dark shows which one the laptop turned down (#24).
    ("keyboard brightness", Command::new(0x0E, 0x84), 0x02, &[1]),
    ("backlight brightness", cmd::MATRIX_BRIGHTNESS.get(), cmd::MATRIX_BRIGHTNESS_SIZE, &[cmd::VARSTORE, 5]),
    ("backlight brightness (old)", cmd::LED_BRIGHTNESS.get(), 0x03, &[cmd::VARSTORE, 5]),
];

/// Read-only GETs for a mouse dock's own settings, from Synapse's dock code
/// (`rzDevice25Linker`). Serial is left out: its reply would print in full.
const DOCK_PROBES: &[(&str, Command, u8, &[u8])] = &[
    ("firmware", cmd::FIRMWARE.get(), cmd::FIRMWARE_SIZE, &[]),
    ("dock firmware", Command::new(0x00, 0x91), 0x02, &[]),
    ("dock edition", Command::new(0x00, 0xBC), 0x03, &[]),
    ("mouse on dock charging", Command::new(0x00, 0xBB), 0x01, &[]),
    ("paired mice", cmd::WIRELESS_DEVICES.get(), cmd::WIRELESS_DEVICES_SIZE, &[]),
    ("auto pairing", Command::new(0x00, 0xD5), 0x01, &[]),
    ("dock LED brightness", cmd::MATRIX_BRIGHTNESS.get(), cmd::MATRIX_BRIGHTNESS_SIZE, &[cmd::VARSTORE, DOCK_LED]),
    ("dock LED effect", cmd::MATRIX_EFFECT.get(), 0x0C, &[cmd::VARSTORE, DOCK_LED]),
    ("charging effect", Command::new(0x03, 0x90), 0x01, &[]),
    ("dock lighting sync", Command::new(0x03, 0x8F), 0x03, &[cmd::VARSTORE, DOCK_LED]),
];

/// LED ids tried with a brightness GET. Razer uses 0x00-0x14 across its products.
const LED_IDS: std::ops::RangeInclusive<u8> = 0x00..=0x14;

pub fn build(api: &HidApi, db: &Db) -> Report {
    let mut interfaces = Vec::new();
    let mut devices = Vec::new();

    let mut infos: Vec<_> = api.device_list().filter(|d| d.vendor_id() == RAZER_VID).collect();
    infos.sort_by_key(|d| (d.product_id(), d.interface_number(), d.usage_page(), d.usage()));
    infos.dedup_by(|a, b| a.path() == b.path());

    for info in infos {
        let pid = info.product_id();
        if db.get(pid).is_some_and(|spec| spec.kind == crate::Kind::Webcam) { continue; }
        let features = crate::transport::feature_reports(api, info.path());
        let dev = crate::device::open_interface(api, info.path(), pid, info.interface_number(), &TXID_CANDIDATES, db.spec(pid));
        interfaces.push(Interface {
            pid: format!("{pid:04X}"),
            interface: info.interface_number(),
            usage_page: format!("{:04X}", info.usage_page()),
            usage: format!("{:04X}", info.usage()),
            txid: dev.as_ref().map(|d| format!("{:02X}", d.txid)),
            report_id: dev.as_ref().map(|d| d.report_id()),
            features,
        });
        // One full probe per product, from the first interface that answers.
        if let Some(dev) = dev {
            if !devices.iter().any(|d: &DeviceReport| d.pid == format!("{pid:04X}")) {
                devices.push(probe(&dev, false));
            }
        }
    }
    devices.extend(crate::sim::from_env().iter().map(|d| probe(d, true)));

    Report {
        secbird: env!("CARGO_PKG_VERSION"),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        interfaces,
        devices,
        cameras: match crate::camera::backend().and_then(|backend| backend.discover()) {
            Ok(cameras) => cameras.into_iter().enumerate().map(|(index, camera)| CameraReport {
                id: format!("camera-{}", index + 1), name: camera.name,
                pid: format!("{:04X}", camera.pid), error: camera.control_error,
            }).collect(),
            Err(error) => vec![CameraReport { id: "camera-backend".into(), name: "Camera discovery".into(), pid: String::new(), error: Some(error.to_string()) }],
        },
    }
}

/// Send every read-only probe to one device.
pub fn probe(dev: &Device, simulated: bool) -> DeviceReport {
    let laptop = dev.spec.as_ref().is_none_or(|s| s.kind == crate::Kind::Laptop);
    let keyboard = laptop || dev.spec.as_ref().is_some_and(|s| matches!(s.kind, crate::Kind::Keyboard | crate::Kind::Keypad));
    let run = |txid: u8, &(name, command, size, args): &(&'static str, Command, u8, &[u8])| {
        let sent = if txid == dev.txid { dev.transact(command, size, args) } else { dev.transact_as(txid, command, size, args) };
        let (status, reply) = match sent {
            Ok(r) => ("ok".to_string(), hex(&r.args()[..size as usize])),
            Err(Error::Device { status, .. }) => (format!("{status:?}"), String::new()),
            Err(Error::NoReply(_)) => ("no reply".to_string(), String::new()),
            Err(e) => (e.to_string(), String::new()),
        };
        Probe { name, command: command.to_string(), status, reply }
    };
    let probes = PROBES
        .iter()
        .chain(KEY_PROBES.iter().filter(|_| keyboard))
        .chain(LAPTOP_PROBES.iter().filter(|_| laptop))
        .map(|p| run(dev.txid, p))
        .collect();
    let leds_at = |txid: u8| -> Vec<u8> {
        LED_IDS
            .filter(|&led| {
                let (get, size, args) = (cmd::MATRIX_BRIGHTNESS.get(), cmd::MATRIX_BRIGHTNESS_SIZE, [cmd::VARSTORE, led, 0]);
                let sent = if txid == dev.txid { dev.transact(get, size, &args) } else { dev.transact_as(txid, get, size, &args) };
                sent.is_ok()
            })
            .collect()
    };
    let leds = leds_at(dev.txid);
    let dock_txid = dev.is_mouse_dock() && dev.txid != DOCK_TXID;
    let dock = if dock_txid { DOCK_PROBES.iter().map(|p| run(DOCK_TXID, p)).collect() } else { Vec::new() };
    let dock_leds = if dock_txid { leds_at(DOCK_TXID) } else { Vec::new() };
    DeviceReport {
        pid: format!("{:04X}", dev.pid),
        name: dev.name(),
        in_database: dev.spec.is_some(),
        simulated,
        interface: dev.interface,
        txid: format!("{:02X}", dev.txid),
        report_id: dev.report_id(),
        // Some Blades answer 0xFF bytes (#34), which printed as replacement characters.
        serial: match dev.serial() {
            Ok(s) if s.chars().all(|c| c.is_ascii_graphic()) => shorten(&s),
            Ok(_) => "none".into(),
            Err(_) => String::new(),
        },
        probes,
        leds,
        dock,
        dock_leds,
    }
}

/// Every byte of a short reply: a trailing 00 can be the answer (#34: smart dimming's
/// `01 00 00` read as just `01`). Long replies drop their zero tail past the first 16.
fn hex(bytes: &[u8]) -> String {
    let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1).max(bytes.len().min(16));
    bytes[..end].iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// Keep the first 4 and last 2 characters: enough to tell devices apart.
fn shorten(serial: &str) -> String {
    let chars: Vec<char> = serial.chars().collect();
    if chars.len() <= 6 {
        return "*".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 2..].iter().collect();
    format!("{head}...{tail}")
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Secretarybird report (secbird {}, {})", self.secbird, self.os)?;
        writeln!(f, "Only read (GET) commands were sent; nothing on the device was changed.")?;
        writeln!(f, "Serial numbers are shortened for privacy.\n")?;

        writeln!(f, "Razer HID interfaces:")?;
        if self.interfaces.is_empty() {
            writeln!(f, "  (none)")?;
        }
        for i in &self.interfaces {
            let id = match i.report_id {
                Some(id) if id != 0 => format!(", report id {id}"),
                _ => String::new(),
            };
            let answers = i.txid.as_ref().map(|t| format!("  <- answers Protocol 2.5 (txid {t}{id})")).unwrap_or_default();
            // Feature reports matter for Bluetooth, whose descriptor numbers them (#10).
            let features = match i.features.as_slice() {
                [] => String::new(),
                list => {
                    let list: Vec<String> = list.iter().map(|(id, len)| format!("{id}:{len}")).collect();
                    format!("  feature {}", list.join(" "))
                }
            };
            writeln!(f, "  {}  interface {:<2} usage {}:{}{features}{answers}", i.pid, i.interface, i.usage_page, i.usage)?;
        }

        for d in &self.devices {
            let known = match (d.simulated, d.in_database) {
                (true, _) => "simulated",
                (false, true) => "in database",
                (false, false) => "NOT in database",
            };
            writeln!(f, "\nDevice {} {} ({known})", d.pid, d.name)?;
            let id = if d.report_id != 0 { format!(", report id {}", d.report_id) } else { String::new() };
            writeln!(f, "  interface {}, txid {}{id}, serial {}", d.interface, d.txid, d.serial)?;
            for p in &d.probes {
                writeln!(f, "  {:<26} {}  {:<12} {}", p.name, p.command, p.status, p.reply)?;
            }
            let leds: Vec<String> = d.leds.iter().map(|l| l.to_string()).collect();
            let leds = if leds.is_empty() { "none".to_string() } else { leds.join(", ") };
            writeln!(f, "  {:<26} 0F:84  {leds}", "lighting zones (LED ids)")?;
            if !d.dock.is_empty() {
                writeln!(f, "  The dock itself (txid {DOCK_TXID:02X}):")?;
                for p in &d.dock {
                    writeln!(f, "    {:<24} {}  {:<12} {}", p.name, p.command, p.status, p.reply)?;
                }
                let leds: Vec<String> = d.dock_leds.iter().map(|l| l.to_string()).collect();
                let leds = if leds.is_empty() { "none".to_string() } else { leds.join(", ") };
                writeln!(f, "    {:<24} 0F:84  {leds}", "lighting zones (LED ids)")?;
            }
        }
        if self.devices.is_empty() {
            writeln!(f, "\nNo interface answered Protocol 2.5.")?;
        }
        for camera in &self.cameras {
            writeln!(f, "\nCamera {} {} ({})", camera.pid, camera.name, camera.id)?;
            writeln!(f, "  {}", camera.error.as_deref().unwrap_or("UVC controls available; current settings not read back"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Sim, KEYBOARD_PID, MOUSE_PID, WIRELESS_MOUSE_PID};

    fn report_for(sim: &Sim, pid: u16) -> DeviceReport {
        probe(&sim.open(pid, Sim::spec_for(pid)).unwrap(), true)
    }

    fn status<'a>(r: &'a DeviceReport, name: &str) -> &'a str {
        &r.probes.iter().find(|p| p.name == name).unwrap().status
    }

    #[test]
    fn finds_mouse_zones_and_settings() {
        let r = report_for(&Sim::mouse(), MOUSE_PID);
        assert_eq!(r.leds, [1, 4, 10]);
        assert_eq!(status(&r, "dpi"), "ok");
        assert_eq!(r.probes.iter().find(|p| p.name == "dpi").unwrap().reply, "00 03 20 03 20 00 00");
        assert_eq!(status(&r, "battery"), "NotSupported");
        assert_eq!(r.serial, "SIMM...01");
    }

    #[test]
    fn wireless_and_keyboard_differ() {
        let w = report_for(&Sim::wireless_mouse(), WIRELESS_MOUSE_PID);
        assert_eq!(status(&w, "battery"), "ok");
        assert_eq!(w.txid, "3F");
        let k = report_for(&Sim::keyboard(), KEYBOARD_PID);
        assert_eq!(k.leds, [5]);
        assert_eq!(status(&k, "dpi"), "NotSupported");
    }

    #[test]
    fn laptop_probes_only_for_laptops_and_unknown_devices() {
        let has_fans = |r: &DeviceReport| r.probes.iter().any(|p| p.name == "fan 1 mode");
        assert!(!has_fans(&report_for(&Sim::mouse(), MOUSE_PID)));
        let sim = Sim::mouse();
        let unknown = probe(&sim.open(MOUSE_PID, None).unwrap(), true);
        assert!(has_fans(&unknown), "a new Blade isn't in the database yet");
        assert!(sim.state().log.iter().all(|c| c.id & 0x80 != 0), "still only GET commands");
    }

    #[test]
    fn report_only_reads() {
        let sim = Sim::mouse();
        report_for(&sim, MOUSE_PID);
        assert!(sim.state().log.iter().all(|c| c.id & 0x80 != 0), "only GET commands");
        assert_eq!(sim.state().dpi, Some((800, 800)));
    }

    #[test]
    fn hex_and_shorten() {
        assert_eq!(hex(&[1, 0, 0x20, 0, 0]), "01 00 20 00 00");
        assert_eq!(hex(&[0, 0]), "00 00");
        let mut long = [0u8; 0x50];
        long[1] = 7;
        assert_eq!(hex(&long), ["00 07", &" 00".repeat(14)].concat());
        long[20] = 1;
        assert_eq!(hex(&long).split(' ').count(), 21);
        assert_eq!(shorten("AB1234C56789012"), "AB12...12");
        assert_eq!(shorten("ABC"), "***");
    }
}
