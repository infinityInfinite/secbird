//! Discovery and request/reply transport over HID feature reports.

use crate::commands as cmd;
use crate::db::{Db, DeviceSpec, Kind, RAZER_VID};
use crate::error::{Error, Result};
use crate::proto::{Command, Report, Status, REPORT_LEN};
use crate::transport::Transport;
use hidapi::HidApi;
use std::ffi::CString;
use std::thread::sleep;

/// Synapse retries reads up to 30 times (5 ms apart, see [`Transport::read_delay`]).
const READ_RETRIES: u32 = 30;

/// Writes to the device's flash (macros) take longer: Synapse waits at least 10 ms between
/// reads and retries 40 times. Allow up to about two seconds.
const FLASH_RETRIES: u32 = 200;

/// Transaction ids seen across Razer devices, tried in order when the database has none.
pub const TXID_CANDIDATES: [u8; 4] = [0x1F, 0x3F, 0xFF, 0x9F];

pub struct Device {
    transport: Box<dyn Transport>,
    pub pid: u16,
    /// Serial number read during discovery; empty if the device reports none.
    pub serial: String,
    pub interface: i32,
    pub path: CString,
    pub txid: u8,
    pub spec: Option<DeviceSpec>,
}

impl Device {
    pub fn name(&self) -> String {
        match &self.spec {
            Some(s) => s.name.clone(),
            None => format!("Unknown Razer device {:04X}", self.pid),
        }
    }

    /// A Mouse Dock Pro is the docked mouse's receiver: every command reaches the mouse.
    /// Ask which mouse is paired and take its name, DPI range and lighting zones. Polling
    /// stays on the dock's `00:C0` command. If the answer is missing or isn't a mouse that
    /// pairs with the dock, keep the generic database entry.
    pub(crate) fn identify_docked_mouse(&mut self, db: &Db) {
        if self.pid != MOUSE_DOCK_PRO {
            return;
        }
        let Ok(reply) = self.get(cmd::WIRELESS_DEVICES, cmd::WIRELESS_DEVICES_SIZE, &[]) else { return };
        let Some(pid) = connected_mouse_pid(reply.args()).filter(|p| DOCK_PRO_MICE.contains(p)) else { return };
        let Some(mouse) = db.get(pid).filter(|s| s.kind == Kind::Mouse) else { return };
        if let Some(spec) = &mut self.spec {
            spec.name = format!("{} on Mouse Dock Pro", mouse.name.trim_end_matches(" (Wireless)"));
            spec.dpi = mouse.dpi.clone();
            spec.lighting = mouse.lighting;
            spec.leds = mouse.leds.clone();
            spec.onboard_profiles = mouse.onboard_profiles;
        }
    }

    /// Send one command and wait for its reply. `size` is the argument length the
    /// device expects for this command.
    pub fn transact(&self, command: Command, size: u8, args: &[u8]) -> Result<Report> {
        if crate::headset::is_headset(self.pid) {
            return Err(Error::Unsupported("this headset speaks HID audio reports, not Protocol 2.5".into()));
        }
        transact(self.transport.as_ref(), self.txid, command, size, args, READ_RETRIES)
    }

    /// A read-only GET on a headset's own protocol ([`crate::headset`]).
    pub(crate) fn headset_query(&self, command: u8) -> Result<u8> {
        if !crate::headset::is_headset(self.pid) {
            return Err(Error::Unsupported("HID audio reports on this device".into()));
        }
        crate::headset::query(self.transport.as_ref(), command)
    }

    /// One headset request with arguments, answered by the headset.
    pub(crate) fn headset_transact(&self, command: u8, args: &[u8]) -> Result<Vec<u8>> {
        if !crate::headset::is_headset(self.pid) {
            return Err(Error::Unsupported("HID audio reports on this device".into()));
        }
        crate::headset::transact(self.transport.as_ref(), command, args)
    }

    /// [`Device::transact`] for commands that write flash and answer slowly.
    pub(crate) fn transact_slow(&self, command: Command, size: u8, args: &[u8]) -> Result<Report> {
        transact(self.transport.as_ref(), self.txid, command, size, args, FLASH_RETRIES)
    }

    pub fn get(&self, command: Command, size: u8, args: &[u8]) -> Result<Report> {
        self.transact(command.get(), size, args)
    }

    /// Stable identity of this physical device: `PID:SERIAL`, or just `PID` when the
    /// device has no usable serial. Profiles are stored under this key, so two
    /// identical mice keep separate settings.
    pub fn key(&self) -> String {
        if self.serial.is_empty() {
            format!("{:04X}", self.pid)
        } else {
            format!("{:04X}:{}", self.pid, self.serial)
        }
    }
}

/// Serial from a GET serial reply: printable ASCII only; placeholders count as none.
fn parse_serial(args: &[u8]) -> String {
    let raw = &args[..(cmd::SERIAL_SIZE as usize).min(args.len())];
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    let s: String = raw[..end].iter().map(|&b| b as char).collect::<String>().trim().to_string();
    let usable = !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && s.chars().any(|c| c != '0' && c != 'F' && c != 'f');
    if usable { s } else { String::new() }
}

const MOUSE_DOCK_PRO: u16 = 0x00A4;

/// Mice that pair with the Mouse Dock Pro (Synapse's `DualDongleCompatibleDevices.json`).
const DOCK_PRO_MICE: [u16; 11] =
    [0x00A8, 0x00AB, 0x00B0, 0x00CD, 0x00D7, 0x00DB, 0x00DF, 0x00E3, 0x00E8, 0x00EA, 0xB00B];

/// The one connected mouse in a paired-mice reply. `None` if there is none, or more than one.
fn connected_mouse_pid(args: &[u8]) -> Option<u16> {
    let count = *args.first()? as usize;
    let entries = args.get(1..1 + count * 3)?;
    let mut connected = entries.as_chunks::<3>().0.iter().filter_map(|e| {
        let pid = u16::from_be_bytes([e[1], e[2]]);
        (e[0] == 1 && pid != 0 && pid != 0xFFFF).then_some(pid)
    });
    let pid = connected.next()?;
    connected.next().is_none().then_some(pid)
}

fn transact(hid: &dyn Transport, txid: u8, command: Command, size: u8, args: &[u8], retries: u32) -> Result<Report> {
    let req = Report::request(txid, command, size, args);
    let mut buf = [0u8; REPORT_LEN + 1]; // byte 0 is the HID report id (0)
    buf[1..].copy_from_slice(&req.0);
    hid.send_feature_report(&buf)?;

    for _ in 0..retries {
        sleep(hid.read_delay());
        let mut reply = [0u8; REPORT_LEN + 1];
        let n = hid.get_feature_report(&mut reply)?;
        if n < REPORT_LEN {
            continue;
        }
        let mut body = [0u8; REPORT_LEN];
        body.copy_from_slice(&reply[n - REPORT_LEN..n]);
        let rep = Report(body);
        if rep.command() != command {
            continue;
        }
        match rep.status() {
            Status::Success => return Ok(rep),
            Status::Busy | Status::New => continue,
            status => return Err(Error::Device { cmd: command, status }),
        }
    }
    Err(Error::NoReply(command))
}

/// A Razer HID interface seen during enumeration, before we know if it speaks Protocol 2.5.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub pid: u16,
    pub interface: i32,
    pub path: CString,
}

pub fn candidates(api: &HidApi) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = api
        .device_list()
        .filter(|d| d.vendor_id() == RAZER_VID)
        // Audio headsets only speak their own protocol, and only on the vendor collection.
        .filter(|d| !crate::headset::is_headset(d.product_id()) || (d.usage_page() == 0xFF14 && d.usage() == 1))
        .map(|d| Candidate {
            pid: d.product_id(),
            interface: d.interface_number(),
            path: d.path().to_owned(),
        })
        .collect();
    out.sort_by(|a, b| (a.pid, a.interface, &a.path).cmp(&(b.pid, b.interface, &b.path)));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// Find every connected Razer device that answers Protocol 2.5, one handle per physical
/// device (several units of the same model are told apart by serial number).
///
/// Simulated devices named in `SECBIRD_SIM` (e.g. `mouse,keyboard`) are added at the end.
pub fn discover(api: &HidApi, db: &Db) -> Vec<Device> {
    let mut found = discover_hid(api, db);
    for dev in &mut found {
        dev.identify_docked_mouse(db);
    }
    found.extend(crate::sim::from_env());
    found
}

fn discover_hid(api: &HidApi, db: &Db) -> Vec<Device> {
    let all = candidates(api);
    let mut pids: Vec<u16> = all.iter().map(|c| c.pid).collect();
    pids.dedup();

    pids.into_iter()
        .filter_map(|pid| -> Option<Vec<Device>> {
            let spec = db.get(pid).cloned();
            let mut ifaces: Vec<&Candidate> = all.iter().filter(|c| c.pid == pid).collect();
            // Known interface first; the rest still get a chance.
            if let Some(want) = spec.as_ref().and_then(|s| s.interface) {
                ifaces.sort_by_key(|c| c.interface != want);
            }
            let txids: Vec<u8> = match spec.as_ref().and_then(|s| s.txid) {
                Some(t) => std::iter::once(t).chain(TXID_CANDIDATES).collect(),
                None => TXID_CANDIDATES.to_vec(),
            };
            let mut iter = ifaces.into_iter();
            let first = iter.by_ref().find_map(|c| probe(api, c, &txids, spec.clone()))?;
            // More units of the same model answer on the same interface number.
            let mut units = vec![first];
            let rest: Vec<&Candidate> = iter.filter(|c| c.interface == units[0].interface).collect();
            for c in rest {
                if let Some(dev) = probe(api, c, &[units[0].txid], spec.clone()) {
                    if !dev.serial.is_empty() && units.iter().all(|u| u.serial != dev.serial) {
                        units.push(dev);
                    }
                }
            }
            Some(units)
        })
        .flatten()
        .collect()
}

fn probe(api: &HidApi, c: &Candidate, txids: &[u8], spec: Option<DeviceSpec>) -> Option<Device> {
    let hid = api.open_path(&c.path).ok()?;
    Device::open(Box::new(hid), c.pid, c.interface, c.path.clone(), txids, spec)
}

impl Device {
    /// Wrap a transport, keeping the first txid that answers a harmless GET (serial number).
    pub fn open(
        transport: Box<dyn Transport>,
        pid: u16,
        interface: i32,
        path: CString,
        txids: &[u8],
        spec: Option<DeviceSpec>,
    ) -> Option<Device> {
        if crate::headset::is_headset(pid) {
            // Headsets have no Protocol 2.5 serial; a battery read proves it answers, and
            // gives the device its key along the way.
            let dev = Device { transport, pid, serial: String::new(), interface, path, txid: 0x60, spec };
            dev.headset_query(crate::headset::BATTERY).ok()?;
            return Some(dev);
        }
        let (txid, reply) = txids.iter().copied().find_map(|t| {
            transact(transport.as_ref(), t, cmd::SERIAL.get(), cmd::SERIAL_SIZE, &[], READ_RETRIES).ok().map(|r| (t, r))
        })?;
        let serial = parse_serial(reply.args());
        Some(Device { transport, pid, serial, interface, path, txid, spec })
    }
}

/// Razer products that are plugged in but didn't answer Protocol 2.5 (e.g. Kraken
/// headsets), so the user sees them instead of wondering where they went.
pub fn unanswered(api: &HidApi, found: &[Device]) -> Vec<u16> {
    let mut pids: Vec<u16> = candidates(api).iter().map(|c| c.pid).filter(|p| !found.iter().any(|d| d.pid == *p)).collect();
    pids.dedup();
    pids
}

#[cfg(test)]
mod tests {
    use super::{connected_mouse_pid, parse_serial};

    #[test]
    fn paired_mice_reply_needs_exactly_one_connected_mouse() {
        assert_eq!(connected_mouse_pid(&[2, 1, 0, 0xAB, 0, 0xFF, 0xFF]), Some(0x00AB));
        for args in [
            &[][..], &[0], &[2, 1, 0, 0xAB], &[255, 1, 0, 0xAB],
            &[1, 0, 0, 0xAB], &[1, 1, 0xFF, 0xFF], &[1, 1, 0, 0],
            &[2, 1, 0, 0xAB, 1, 0, 0xA8],
        ] {
            assert_eq!(connected_mouse_pid(args), None, "{args:?}");
        }
    }

    #[test]
    fn serials_are_cleaned() {
        assert_eq!(parse_serial(b"AB1234C56789012\0\0\0\0\0\0\0"), "AB1234C56789012");
        assert_eq!(parse_serial(b"  ABC123  \0"), "ABC123");
        assert_eq!(parse_serial(&[0u8; 22]), "");
        assert_eq!(parse_serial(b"000000000000\0"), "");
        assert_eq!(parse_serial(&[0xFF; 22]), "");
        assert_eq!(parse_serial(b"AB\x01CD\0"), "");
    }
}
