//! `secbird listen`: a read-only recording of everything a device sends, to learn what its
//! interfaces carry (#21: does the Tartarus V2 Pro report its stick's position in 4-way and
//! 8-way mode?). It opens every HID collection of the device that Windows lets it read,
//! walks the user through a few steps (switch the mode in Synapse, move the stick), and
//! writes each new input report, with its collection and time, to a log file.
//!
//! Nothing is written to the device. The only feature reports are GET requests (bit 7 of the
//! command id set) for the stick settings, the same reads Synapse makes.

use secbird_core::hidapi::{HidApi, HidDevice};
use secbird_core::proto::Command;
use secbird_core::{config_dir, Device, Error};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TARTARUS_V2_PRO: u16 = 0x02EB;
const RECORD_FOR: Duration = Duration::from_secs(12);
/// At most this many changed reports are logged per collection and step.
const MAX_LINES: usize = 4000;

/// The steps, as (name, what to set in Synapse first).
const STEPS: [(&str, &str); 4] = [
    ("analog", "Thumbstick Input Mode: TRUE ANALOG (the first icon)"),
    ("8-way", "Thumbstick Input Mode: 8-WAY (the third icon)"),
    ("4-way", "Thumbstick Input Mode: 4-WAY (the middle icon)"),
    ("8-way, all disabled", "8-WAY, with every direction set to Disable (skip this one if you prefer)"),
];

/// `listen keys`: one step, with Synapse closed and the keypad in driver mode, to see whether
/// ordinary key presses come in the keypad's own report too (#21: remapping keys in software
/// while the stick is keys).
const KEY_STEPS: [(&str, &str); 1] = [("keys", "")];
const KEY_HOW: &str = "Press key 6 once, slowly all the way down and back up. Then press keys 1, 2 and 3 \
one after another. Then hold key 15 (S) down for two seconds. Then press each thumb button once.";

struct Collection {
    label: String,
    path: std::ffi::CString,
}

pub fn run(api: &HidApi, devices: &[Device], pid_arg: Option<u16>, keys: bool) -> Result<(), Error> {
    let pid = pid_arg.unwrap_or(TARTARUS_V2_PRO);
    let kind = if keys { "keys-" } else { "" };
    let path = config_dir().join(format!("listen-{kind}{pid:04x}-{}.txt", unix_now()));
    let _ = std::fs::create_dir_all(config_dir());
    let mut log = std::fs::File::create(&path).map_err(|e| Error::System(format!("{}: {e}", path.display())))?;
    let mut both = |line: &str| {
        println!("{line}");
        let _ = writeln!(log, "{line}");
    };

    both(&format!("Secretarybird {} listen, device {pid:04X}. Read-only: nothing is sent to the device", env!("CARGO_PKG_VERSION")));
    both("except GET requests for its stick settings.");
    both("");

    let collections: Vec<Collection> = api
        .device_list()
        .filter(|i| i.vendor_id() == secbird_core::db::RAZER_VID && i.product_id() == pid)
        .map(|i| Collection {
            label: format!(
                "interface {} usage {:04X}:{:04X}",
                i.interface_number(),
                i.usage_page(),
                i.usage()
            ),
            path: i.path().to_owned(),
        })
        .collect();
    if collections.is_empty() {
        return Err(Error::NotFound(format!("no Razer device {pid:04X} connected")));
    }

    // Open what can be opened; Windows keeps keyboards and mice to itself.
    let running = Arc::new(AtomicBool::new(true));
    let (tx, rx) = channel::<(usize, Instant, Vec<u8>)>();
    let mut opened = Vec::new();
    for (i, c) in collections.iter().enumerate() {
        match api.open_path(&c.path) {
            Ok(dev) => {
                both(&format!("  [{i}] {}  open", c.label));
                opened.push(i);
                spawn_reader(i, dev, tx.clone(), running.clone());
            }
            Err(e) => both(&format!("  [{i}] {}  can't be read ({e})", c.label)),
        }
    }
    drop(tx);
    both("");

    let keypad = devices.iter().find(|d| d.pid == pid);
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let steps: &[(&str, &str)] = if keys { &KEY_STEPS } else { &STEPS };
    if keys {
        println!("Synapse closed, and Secretarybird's tray app too (tray icon > Exit).");
        println!("Put the keypad in driver mode first (\"Keypad to driver mode (test).bat\"); the");
        println!("\"device mode\" line below should then say 03 00.");
        println!("Open Notepad if you like: the keys type as usual meanwhile.");
    } else {
        println!("Keep Synapse open: you switch the stick's mode in it.");
        println!("Exit Secretarybird's tray app first (tray icon > Exit) so it doesn't press keys meanwhile.");
    }
    println!();
    for (n, (name, what)) in steps.iter().enumerate() {
        if keys {
            println!("When you press Enter, recording starts for {} seconds. Then: {KEY_HOW}", RECORD_FOR.as_secs());
        } else {
            println!("Step {} of {}: in Synapse, set {what}.", n + 1, steps.len());
        }
        print!("Then press Enter here (or type s and Enter to skip): ");
        let _ = std::io::stdout().flush();
        let answer = lines.next().and_then(|l| l.ok()).unwrap_or_default();
        if answer.trim().eq_ignore_ascii_case("s") {
            both(&format!("== step {name}: skipped"));
            continue;
        }
        both(&format!("== step {name}"));
        if let Some(dev) = keypad {
            for line in stick_settings(dev) {
                both(&format!("   {line}"));
            }
        }
        // Drop anything that arrived while waiting for Enter.
        while rx.try_recv().is_ok() {}
        if keys {
            println!("Recording: go.");
        } else {
            println!("Recording for {} seconds: move the stick slowly round its edge twice, then", RECORD_FOR.as_secs());
            println!("push it to up, right, down and left, letting it spring back each time.");
        }
        let summary = record(&rx, &collections, &mut both);
        both(&summary);
        both("");
    }
    running.store(false, Ordering::Relaxed);
    if keys {
        println!("Done. Unplug and replug the keypad (or start the tray) to leave this mode.");
    } else {
        println!("Done. In Synapse, set the stick back to the mode you want.");
    }
    println!("The log is {}", path.display());
    Ok(())
}

fn spawn_reader(index: usize, dev: HidDevice, tx: std::sync::mpsc::Sender<(usize, Instant, Vec<u8>)>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        while running.load(Ordering::Relaxed) {
            match dev.read_timeout(&mut buf, 50) {
                Ok(0) => {}
                Ok(n) => {
                    if tx.send((index, Instant::now(), buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    });
}

/// Log each report that differs from the last one of its collection and report id, for a
/// while; returns a per-collection summary.
fn record(
    rx: &Receiver<(usize, Instant, Vec<u8>)>,
    collections: &[Collection],
    both: &mut impl FnMut(&str),
) -> String {
    let start = Instant::now();
    let mut last: HashMap<(usize, u8), Vec<u8>> = HashMap::new();
    let mut total: HashMap<usize, (usize, usize)> = HashMap::new(); // (reports, changes)
    while let Some(left) = RECORD_FOR.checked_sub(start.elapsed()) {
        let Ok((i, at, data)) = rx.recv_timeout(left.min(Duration::from_millis(200))) else { continue };
        let entry = total.entry(i).or_default();
        entry.0 += 1;
        let id = data.first().copied().unwrap_or(0);
        if last.get(&(i, id)) == Some(&data) {
            continue;
        }
        entry.1 += 1;
        if entry.1 <= MAX_LINES {
            let ms = at.saturating_duration_since(start).as_millis();
            both(&format!("{ms:>6} [{i}] {:>3}: {}", data.len(), hex(&data)));
        }
        last.insert((i, id), data);
    }
    let mut out = String::from("   summary:");
    if total.is_empty() {
        out.push_str(" no input reports at all");
    }
    let mut keys: Vec<_> = total.keys().copied().collect();
    keys.sort();
    for i in keys {
        let (n, changed) = total[&i];
        out.push_str(&format!("\n   [{i}] {}: {n} reports, {changed} changed", collections[i].label));
    }
    out
}

/// The stick settings as the keypad reports them (GET requests only).
fn stick_settings(dev: &Device) -> Vec<String> {
    let reads: [(&str, Command, u8, &[u8]); 5] = [
        ("device mode (00:84; 00 00 normal, 03 00 driver)", Command::new(0x00, 0x04), 2, &[]),
        ("report mode (02:B8; 0 analog, 1 directions)", Command::new(0x02, 0x38), 1, &[]),
        ("mapping mode (02:B9, profile 1; 0 analog, 1 4-way, 2 8-way)", Command::new(0x02, 0x39), 2, &[1]),
        ("stick output (02:C4, profile 1; 1 left, 0 right)", Command::new(0x02, 0x44), 2, &[1]),
        ("orientation (02:C5, profile 1; 1 rotated, 0 default)", Command::new(0x02, 0x45), 2, &[1]),
    ];
    reads
        .iter()
        .map(|(what, cmd, size, args)| match dev.get(*cmd, *size, args) {
            Ok(r) => format!("{what}: {}", hex(&r.args()[..usize::from(*size)])),
            Err(e) => format!("{what}: {e}"),
        })
        .collect()
}

fn hex(data: &[u8]) -> String {
    // Trailing zeros carry nothing; keep the length in the line instead.
    let end = data.iter().rposition(|b| *b != 0).map_or(0, |p| p + 1);
    data[..end].iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
