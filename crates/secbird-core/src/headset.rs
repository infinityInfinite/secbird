//! Razer audio headsets on their own protocol, vendor collection FF14:0001: 64-byte HID
//! output reports out and input reports back, not feature reports. No initialization or
//! RF wake loop is needed.
//!
//! Confirmed on hardware (BlackShark V3 Pro, 1532:0577 wireless and 1532:0576 wired —
//! both answer identically): battery, charging, ANC/ambient, six speaker EQ slots,
//! microphone EQ and presets, sidetone and auto-off. Packet facts:
//! https://github.com/openrazer/openrazer/issues/2838
//!
//! The plain BlackShark V3 (1532:0579 wired, 1532:057A wireless) reports the same vendor
//! collection in issue #15 and is expected to answer the same way, but there is no such
//! device here to check. One that doesn't answer stays in the "not answering" list.

use crate::error::{Error, Result};
use crate::proto::{Command, Status};
use crate::transport::Transport;
use crate::Device;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Wireless dongle; see [`PIDS`] for the rest.
pub const PID: u16 = 0x0577;
/// Every product that speaks this protocol.
pub const PIDS: [u16; 4] = [0x0576, 0x0577, 0x0579, 0x057A];

/// Whether this product id speaks the headset protocol rather than Protocol 2.5.
pub fn is_headset(pid: u16) -> bool {
    PIDS.contains(&pid)
}

pub const BATTERY: u8 = 0x21;
pub const CHARGING: u8 = 0x2A;

/// Every setting, in the order the editor and `secbird report` list them: the name
/// [`Device::audio_read`] takes, the label a person sees, and the GET command behind it.
pub const SETTINGS: &[(&str, &str, u8)] = &[
    ("anc", "noise cancelling", 0x12),
    ("eq-preset", "active speaker EQ slot", 0x13),
    ("eq-0", "speaker EQ slot 0", 0x15),
    ("eq-1", "speaker EQ slot 1", 0x15),
    ("eq-2", "speaker EQ slot 2", 0x15),
    ("eq-3", "speaker EQ slot 3", 0x15),
    ("eq-4", "speaker EQ slot 4", 0x15),
    ("eq-5", "speaker EQ slot 5", 0x15),
    ("mic-eq", "microphone EQ", 0x17),
    ("mic-preset", "microphone EQ preset", 0x16),
    ("sidetone", "sidetone level", 0x19),
    ("mic-mute", "microphone mute", 0x55),
    ("auto-off", "auto power-off", 0x2C),
];

fn request(command: u8, args: &[u8]) -> [u8; 64] {
    let mut packet = [0u8; 64];
    packet[0] = 2; // report id
    packet[2] = 0x60;
    packet[6] = 4 + args.len() as u8;
    packet[9] = 0x80; // route to the wireless headset (not a GET flag)
    packet[10] = command;
    packet[12] = args.len() as u8;
    packet[13..13 + args.len()].copy_from_slice(args);
    packet[62] = packet[..62].iter().fold(0, |crc, b| crc ^ b);
    packet
}

fn reply(packet: &[u8], command: u8) -> Result<Option<Vec<u8>>> {
    if packet.len() != 64 || packet[0] != 2 || packet[2] != 0x60
        || packet[9] != 0x80 || packet[10] != command
        || packet[7] != 0 || packet[8] != 0 || packet[11] != 1
        || packet[12] > 49 || packet[6] != 4 + packet[12]
        || packet[..62].iter().fold(0, |crc, b| crc ^ b) != packet[62]
    {
        return Ok(None);
    }
    match Status::from(packet[1]) {
        Status::Success => {
            Ok(Some(packet[13..13 + packet[12] as usize].to_vec()))
        }
        Status::New | Status::Busy => Ok(None),
        status => Err(Error::Device { cmd: Command::new(command, 0x80), status }),
    }
}

pub(crate) fn query(transport: &dyn Transport, command: u8) -> Result<u8> {
    let data = transact(transport, command, &[])?;
    if data.len() != 1 || (command == BATTERY && data[0] > 100) || (command == CHARGING && data[0] > 1) {
        return Err(Error::Unsupported("invalid headset status value".into()));
    }
    Ok(data[0])
}

pub(crate) fn transact(transport: &dyn Transport, command: u8, args: &[u8]) -> Result<Vec<u8>> {
    if args.len() > 49 {
        return Err(Error::InvalidArg("headset packet too large".into()));
    }
    // Synapse's audio transport spaces consecutive commands by at least 10 ms.
    std::thread::sleep(Duration::from_millis(10));
    transport.send_output_report(&request(command, args))?;
    let end = Instant::now() + Duration::from_millis(500);
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        let mut packet = [0u8; 64];
        let n = transport.read_timeout(&mut packet, left.as_millis().max(1) as i32)?;
        if n == 0 {
            break;
        }
        if let Some(n) = reply(&packet[..n.min(packet.len())], command)? {
            return Ok(n);
        }
    }
    Err(Error::NoReply(Command::new(command, 0x80)))
}

/// Settings implemented in the BlackShark firmware, not Synapse's THX driver.
/// EQ uses sign/magnitude bytes; speaker EQ has six separately addressable slots.
pub type Settings = BTreeMap<String, Vec<i16>>;

fn command(name: &str) -> Result<(u8, u8, Vec<u8>, usize)> {
    let (get, set, args, len) = match name {
        "anc" => (0x12, 0x92, vec![], 2),
        "eq-preset" => (0x13, 0x93, vec![], 1),
        "mic-preset" => (0x16, 0x96, vec![], 1),
        "mic-eq" => (0x17, 0x97, vec![], 10),
        "sidetone" => (0x19, 0x99, vec![], 1),
        "mic-mute" => (0x55, 0xE9, vec![], 1),
        "auto-off" => (0x2C, 0xAC, vec![], 1),
        "eq-0" | "eq-1" | "eq-2" | "eq-3" | "eq-4" | "eq-5" =>
            (0x15, 0x95, vec![name.as_bytes()[3] - b'0'], 11),
        _ => return Err(Error::InvalidArg(format!("unknown headset setting '{name}'"))),
    };
    Ok((get, set, args, len))
}

fn encode(name: &str, values: &[i16]) -> Result<(u8, Vec<u8>)> {
    if name == "mic-mute" {
        return Err(Error::Unsupported("headset mute is a physical switch; software mute belongs to Windows audio settings".into()));
    }
    let (_, set, mut args, len) = command(name)?;
    let eq = name == "mic-eq" || name.starts_with("eq-") && name != "eq-preset";
    let valid = if eq {
        values.len() == 10 && values.iter().all(|n| (-12..=12).contains(n))
    } else {
        match (name, values) {
            ("anc", [mode, level]) => [0, 1, 80].contains(mode) && (1..=10).contains(level),
            ("eq-preset", [n]) => (0..=5).contains(n),
            ("mic-preset", [n]) => (32..=35).contains(n),
            ("sidetone", [n]) => (0..=100).contains(n),
            ("mic-mute", [n]) => (0..=1).contains(n),
            ("auto-off", [n]) => [0, 5, 15, 30, 45, 60].contains(n),
            _ => false,
        }
    };
    if !valid || args.len() + values.len() != len {
        return Err(Error::InvalidArg(format!("invalid values for headset {name}")));
    }
    args.extend(values.iter().map(|n| if *n < 0 { 128 | (-*n as u8) } else { *n as u8 }));
    Ok((set, args))
}

impl Device {
    pub fn audio_read(&self, name: &str) -> Result<Vec<i16>> {
        if name == "eq-preset" {
            // 13 reports the legacy EQ selector; Valkyrie selects the active slot in
            // its EQ order table instead (the same table Synapse reads).
            let mut active = Vec::new();
            for slot in 0..6 {
                let order = self.headset_transact(0x60, &[slot])?;
                if order.len() != 6 || order[0] != slot {
                    return Err(Error::Unsupported("invalid speaker EQ slot metadata".into()));
                }
                if order[2] == 1 && order[3] == 1 { active.push(slot as i16); }
            }
            if active.len() != 1 { return Err(Error::Unsupported("ambiguous active speaker EQ slot".into())); }
            return Ok(active);
        }
        let (get, _, args, len) = command(name)?;
        let data = self.headset_transact(get, &args)?;
        if data.len() != len || (!args.is_empty() && data[0] != args[0]) {
            return Err(Error::Unsupported(format!("invalid headset {name} reply")));
        }
        let eq = name == "mic-eq" || !args.is_empty();
        Ok(data[args.len()..].iter().map(|n| {
            if eq && *n > 128 { 128 - *n as i16 } else { *n as i16 }
        }).collect())
    }

    pub fn audio_set(&self, name: &str, values: &[i16]) -> Result<()> {
        let (set, args) = encode(name, values)?;
        if name == "eq-preset" {
            let mut orders = Vec::new();
            for slot in 0..6 {
                let mut order = self.headset_transact(0x60, &[slot])?;
                if order.len() != 6 || order[0] != slot {
                    return Err(Error::Unsupported("invalid speaker EQ slot metadata".into()));
                }
                if slot == args[0] && order[2] != 1 {
                    return Err(Error::Unsupported("this speaker EQ slot is disabled in firmware".into()));
                }
                order[3] = u8::from(slot == args[0]);
                orders.push(order);
            }
            self.headset_transact(0xE1, &[1])?;
            let write = orders.iter().try_for_each(|order| self.headset_transact(0xE0, order).map(|_| ()));
            let commit = self.headset_transact(0xE1, &[2]);
            write?;
            commit?;
        } else if set == 0x95 {
            // Speaker EQ writes are staged: preserve the slot's order/voice metadata,
            // bracket the band + order writes, and commit even if a write fails.
            // This follows Synapse's setCloudBasedEqualizer_Stereo sequence.
            let order = self.headset_transact(0x60, &args[..1])?;
            if order.len() != 6 || order[0] != args[0] {
                return Err(Error::Unsupported("invalid speaker EQ slot metadata".into()));
            }
            self.headset_transact(0xE1, &[1])?;
            let write = self.headset_transact(set, &args)
                .and_then(|_| self.headset_transact(0xE0, &order));
            let commit = self.headset_transact(0xE1, &[2]);
            write?;
            commit?;
        } else {
            self.headset_transact(set, &args)?;
        }
        let actual = self.audio_read(name)?;
        if actual != values {
            return Err(Error::Unsupported(format!("headset {name} read-back {actual:?} differs from {values:?}; close Synapse and retry")));
        }
        Ok(())
    }

    /// Individual read failures are surfaced, not replaced with guessed defaults.
    pub fn audio_state(&self) -> (Settings, BTreeMap<String, String>) {
        let mut settings = Settings::new();
        let mut errors = BTreeMap::new();
        if is_headset(self.pid) {
            for name in ["anc", "eq-preset", "eq-0", "eq-1", "eq-2", "eq-3", "eq-4", "eq-5",
                "mic-preset", "mic-eq", "sidetone", "mic-mute", "auto-off"] {
                match self.audio_read(name) {
                    Ok(value) => { settings.insert(name.into(), value); }
                    Err(e) => { errors.insert(name.into(), e.to_string()); }
                }
            }
        }
        (settings, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct AudioSim(Mutex<Vec<Vec<u8>>>);

    impl Transport for AudioSim {
        fn send_feature_report(&self, _: &[u8]) -> Result<()> {
            Err(Error::Unsupported("no feature reports".into()))
        }
        fn get_feature_report(&self, _: &mut [u8]) -> Result<usize> {
            Err(Error::Unsupported("no feature reports".into()))
        }
        fn send_output_report(&self, data: &[u8]) -> Result<()> {
            let command = data[10];
            let args = &data[13..13 + data[12] as usize];
            assert_eq!(data, request(command, args));
            // Answer every read the report sends, shaped like the headset's own replies.
            let len = match command {
                BATTERY | CHARGING | 0x13 | 0x16 | 0x19 | 0x55 | 0x2C => 1,
                0x12 => 2,
                0x17 => 10,
                0x15 => 11,
                0x60 => 6,
                other => panic!("unexpected headset GET {other:02X}"),
            };
            let mut reply = data.to_vec();
            reply[1] = 2; // success
            reply[6] = 4 + len;
            reply[11] = 1;
            reply[12] = len;
            reply[13..13 + len as usize].fill(0);
            match command {
                BATTERY => reply[13] = 43,
                // A slot and its EQ order both echo the slot they were asked about.
                0x15 | 0x60 => {
                    let slot = args[0];
                    reply[13] = slot;
                    if command == 0x60 {
                        reply[13 + 2] = 1; // enabled
                        reply[13 + 3] = u8::from(slot == 0); // and slot 0 is the active one
                    }
                }
                _ => {}
            }
            reply[62] = reply[..62].iter().fold(0, |crc, b| crc ^ b);
            *self.0.lock().unwrap() = vec![reply, vec![0]]; // unrelated input first
            Ok(())
        }
        fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
            assert!((1..=500).contains(&timeout_ms));
            let reply = self.0.lock().unwrap().pop().unwrap_or_default();
            buf[..reply.len()].copy_from_slice(&reply);
            Ok(reply.len())
        }
    }

    #[test]
    fn headset_discovery_and_telemetry_use_output_input_reports_only() {
        let db = crate::Db::builtin().unwrap();
        let dev = crate::Device::open(
            Box::new(AudioSim(Mutex::new(Vec::new()))), PID, 5,
            std::ffi::CString::new("sim:headset").unwrap(), &[], db.get(PID).cloned(),
        ).expect("audio GET answers even though feature reports are unsupported");
        assert_eq!(dev.name(), "BlackShark V3 Pro (Wireless)");
        assert_eq!(dev.battery().unwrap(), Some(43));
        assert!(!dev.charging().unwrap());
        assert!(dev.dpi().is_err());
        let report = crate::report::probe(&dev, true);
        assert!(
            report.probes.iter().all(|p| p.status == "ok" && p.command.starts_with("HID64:")),
            "{:#?}",
            report.probes
        );
    }

    #[test]
    fn audio_state_reports_every_setting_and_its_value() {
        let db = crate::Db::builtin().unwrap();
        let dev = crate::Device::open(
            Box::new(AudioSim(Mutex::new(Vec::new()))), PID, 5,
            std::ffi::CString::new("sim:headset").unwrap(), &[], db.get(PID).cloned(),
        ).unwrap();
        let (settings, errors) = dev.audio_state();
        assert!(errors.is_empty(), "{errors:?}");
        let names: Vec<&str> = settings.keys().map(String::as_str).collect();
        assert_eq!(names, ["anc", "auto-off", "eq-0", "eq-1", "eq-2", "eq-3", "eq-4", "eq-5",
            "eq-preset", "mic-eq", "mic-mute", "mic-preset", "sidetone"]);
        assert_eq!(settings["eq-preset"], [0], "the active slot comes from the EQ order table");
        assert_eq!(settings["eq-2"].len(), 10);
    }

    #[test]
    fn packets_match_hardware_and_reject_invalid_replies() {
        assert_eq!(request(BATTERY, &[])[62], 0xC7);
        assert_eq!(request(CHARGING, &[])[62], 0xCC);
        let mut packet = request(BATTERY, &[]);
        packet[1] = 2;
        packet[6] = 5;
        packet[11] = 1;
        packet[12] = 1;
        packet[13] = 43;
        packet[62] = packet[..62].iter().fold(0, |crc, b| crc ^ b);
        assert_eq!(packet[62], 0xEF); // captured on this BlackShark
        assert_eq!(reply(&packet, BATTERY).unwrap(), Some(vec![43]));
        assert_eq!(reply(&packet, CHARGING).unwrap(), None);
        assert_eq!(reply(&packet[..63], BATTERY).unwrap(), None);
        packet[62] ^= 1;
        assert_eq!(reply(&packet, BATTERY).unwrap(), None);
        assert_eq!(encode("anc", &[80, 3]).unwrap(), (0x92, vec![80, 3]));
        assert_eq!(encode("eq-2", &[-5; 10]).unwrap(), (0x95, [vec![2], vec![133; 10]].concat()));
        for (name, values) in [("eq-6", vec![0; 10]), ("mic-eq", vec![13; 10]),
            ("anc", vec![255, 3]), ("mic-mute", vec![2]), ("sidetone", vec![-1]),
            ("mic-preset", vec![255]), ("auto-off", vec![255]), ("factory-reset", vec![1])] {
            assert!(encode(name, &values).is_err(), "{name}");
        }
    }
}
