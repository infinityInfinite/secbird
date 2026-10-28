use secbird_core::{actions, config_dir, discover, hidapi::HidApi, Db, Device, Error, Rgb, Store};
use std::process::ExitCode;

mod camera;

const USAGE: &str = "\
Secretarybird - minimal control for Razer peripherals

USAGE: secbird [-d PID] <command> [args]

DEVICE:
  list                      Connected devices
  info                      Firmware, serial, DPI, polling rate, battery
  battery                   Battery level and charging state
  lighting [on|off]         Show or change lighting control for this device (all profiles);
                            off leaves RGB untouched for another app, not lights out
  firmware                  Is newer firmware out? Asks Razer's public product page
                            (apps.razer.com); never installs anything
  report [--json]           Read-only description of connected Razer hardware, to share
                            when a device is missing or misbehaves (serials shortened)
  camera controls           Read current image/camera controls and supported ranges
  dock-test <red|spectrum|battery|off>
                            Test: set a Mouse Dock Pro's own light once (#39)

SETTINGS (saved to the active profile):
  camera [<control> <value>] Kiyo Pro (Windows): hdr on|off, hdr-mode dark|bright,
                            fov wide|medium|narrow, af-mode responsive|passive
                            Standard controls: numeric driver value, auto, or default;
                            e.g. camera brightness 128; camera exposure -6;
                            camera focus auto; camera white-balance auto.
                            Anti-flicker: off|50hz|60hz|auto. Low-light: on|off.
                            Use camera controls to check driver support.
  dpi [X[xY]]               Show or set DPI (e.g. 1600 or 1600x800)
  poll [HZ]                 Show or set polling rate (125, 500, 1000, ...)
  stages [DPI ...] [--use N]
                            Show or set the DPI stages the DPI button cycles through
                            (1-5, each 1600 or 1600x800); --use picks the active one
  sleep [MINUTES|on|off]    Show or set how long a wireless device waits before it
                            sleeps; on/off where the device can stop sleeping
  dimming [stays-lit|half|quarter|dark]
                            Laptops: what the keyboard backlight does when idle
                            (Synapse's Smart Dimming). Untested on hardware yet
  gaming [off|win,alt-tab,alt-f4]
                            Keyboards: gaming mode, the keys it blocks (any of win,
                            alt-tab and alt-f4). Untested on hardware yet
  color <zone> <RRGGBB>     Static color for a lighting zone (name or id)
  brightness <zone> <0-100> Brightness for a lighting zone
  off <zone>                Turn a lighting zone off
  effect <zone> <effect>    Built-in effect: spectrum, breathing:RRGGBB[,RRGGBB]|random,
                            wave:left|right[@speed 1-5], reactive:RRGGBB[@1-3],
                            starlight:RRGGBB[,RRGGBB]|random[@1-3]
  keys <RRGGBB>             Every key one color (keyboards with per-key lighting)
  sync <RRGGBB>             Same color everywhere (skips devices with lighting control off)

BUTTONS (mice with on-board button memory; add --hypershift for the HyperShift layer):
  button [list]             Buttons and what they do now
  button <button> <action>  Remap a button, e.g. `button side-forward key:ctrl+c`
  button <button> reset     Back to its saved setting (Synapse's or the factory one)
  button restore-all        Every remapped button back to its saved setting at once
                            (also puts back a save-to-mouse)
  button save-to-mouse      Write this profile's remaps into the mouse's on-board slot, so
                            they work without Secretarybird; the slot's old settings are kept
  button put-back           Put the slot back to what it held before save-to-mouse
  Actions: left right middle back forward scroll-up scroll-down scroll-left scroll-right
           double-click  key:<combo> (key:a, key:ctrl+shift+f13, key:space)
           turbo:<button or key:..>[@presses-per-second]  (turbo:left@15)
           dpi-up dpi-down dpi-cycle-up dpi-cycle-down dpi-clutch:<dpi>
           hypershift  media:volume-up|volume-down|mute|play-pause|next-track|previous-track
           profile-cycle-up  disabled
           macro:<id>[@times|@hold|@toggle]  (macro:3, macro:3@5, macro:3@hold)

KEYS (keyboards and keypads; BETA, no keyboard has confirmed it works yet; --hypershift too):
  key [list]                Keys the keyboard lets us remap, and what they do now
  key <key> <action>        Remap a key, e.g. `key capslock key:esc` (actions as for buttons)
  key <key> reset           Back to its saved setting
  key restore-all           Every remapped key and button back to its saved setting
  Keys: a-z 0-9 f1-f12 esc tab capslock lshift rshift lctrl rctrl lalt ralt lwin menu space
        enter backspace grave minus equal lbracket rbracket backslash semicolon quote comma
        period slash insert delete home end pageup pagedown up down left right printscreen
        scrolllock pause numlock num0-num9 num-slash num-star num-minus num-plus num-enter
        num-period iso-hash iso-backslash

MACROS (stored on the device, shared by all profiles):
  macro [list]              Macros on the device and free memory
  macro show <id>           A macro's steps
  macro new <name> <steps>  Store a macro, e.g. `macro new Copy 'tap:ctrl+c wait:30'`
                            Steps: tap:<key or combo> down:<key> up:<key> wait:<ms>
                            click:<button> press:<button> release:<button> wheel:up|down
  macro delete <id>         Delete a macro

PROFILES:
  profile [list]            Profiles for this device (* = active)
  profile new <name>        New profile, copied from the active one
  profile use <name>        Switch to a profile and apply it
  profile delete <name>     Delete a profile
  profile show [name]       Settings stored in a profile
  profile apps <name> [program ...]
                            Switch to <name> while one of these programs is in front
                            (e.g. cs2.exe); no programs clears the list
  apply                     Re-send the active profile to the device
  export <file>             Save all profiles of all devices to a file
  import <file>             Add profiles from an exported file (keeps yours)

CLEANUP (Razer's background services and startup entries; drivers are never touched):
  debloat                   Show what Razer runs in the background (changes nothing)
  debloat apply [-y]        Turn it off. Needs administrator. Prints every command first
  debloat undo [-y]         Put back exactly what `debloat apply` changed

UPDATES:
  update                    Is a newer Secretarybird out? Asks GitHub (api.github.com);
                            never installs anything

OPTIONS:
  -V       Show the version (also --version)
  -d ID    Pick by USB product id (0099), serial, or full camera identity from list

Set SECBIRD_SIM=all (or a list: mouse,wireless,keyboard,old-keyboard,naga,cobra,dock,blackwidow,
goliathus,firefly,blade,base-station) to add simulated devices (no hardware needed).

Files live in %APPDATA%\\Secretarybird (profiles.json, devices.json).
";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut args: Vec<String>) -> Result<(), Error> {
    // -d takes a product id (0099), a device key (0099:AB1234C56789012) or a serial.
    let mut selector: Option<String> = None;
    if let Some(i) = args.iter().position(|a| a == "-d") {
        let v = args.get(i + 1).ok_or_else(|| Error::InvalidArg("-d needs a PID or serial".into()))?;
        selector = Some(v.trim_start_matches("0x").to_ascii_uppercase());
        args.drain(i..=i + 1);
    }
    let selects = |pid: u16, key: &str, serial: &str| match &selector {
        None => true,
        Some(v) => *v == format!("{pid:04X}") || v.eq_ignore_ascii_case(key) || (!serial.is_empty() && v.eq_ignore_ascii_case(serial)),
    };
    let Some(command) = args.first().cloned() else {
        print!("{USAGE}");
        return Ok(());
    };
    if matches!(command.as_str(), "-V" | "--version" | "version") {
        println!("secbird {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if matches!(command.as_str(), "-h" | "--help" | "help") {
        print!("{USAGE}");
        return Ok(());
    }

    if command == "update" {
        match secbird_core::update::check()? {
            Some(r) => println!("Secretarybird {} is out (you have {}).
Download: {}", r.version, secbird_core::update::CURRENT, r.url),
            None => println!("Secretarybird {} is the latest version.", secbird_core::update::CURRENT),
        }
        return Ok(());
    }
    if command == "debloat" {
        return debloat_cmd(&args[1..]);
    }
    if command == "export" || command == "import" {
        let file = args.get(1).ok_or_else(|| Error::InvalidArg(format!("usage: {command} <file>")))?;
        let path = std::path::Path::new(file);
        let mut store = Store::load(&config_dir().join("profiles.json"))?;
        if command == "export" {
            let text = serde_json::to_string_pretty(&store).map_err(|e| Error::Db(e.to_string()))?;
            std::fs::write(path, text).map_err(|e| Error::Db(format!("{file}: {e}")))?;
            let count: usize = store.devices.values().map(|d| d.profiles.len()).sum();
            println!("Exported {count} profile(s) for {} device(s) to {file}", store.devices.len());
        } else {
            if !path.exists() {
                return Err(Error::NotFound(file.clone()));
            }
            let incoming = Store::load(path)?;
            let connected: Vec<String> = match (HidApi::new(), Db::load(Some(&config_dir().join("devices.json")))) {
                (Ok(api), Ok(db)) => discover(&api, &db).iter().map(Device::key).collect(),
                _ => Vec::new(),
            };
            let added = store.import_for(incoming, &connected);
            store.save()?;
            println!("Imported {added} profile(s). Existing profiles were kept; clashing names got \"(imported)\".");
        }
        return Ok(());
    }

    #[cfg(not(windows))]
    if command == "camera" {
        return Err(Error::Unsupported("camera controls require Windows 10/11".into()));
    }

    let dir = config_dir();
    let api = HidApi::new()?;
    let db = Db::load(Some(&dir.join("devices.json")))?;
    let camera_backend = secbird_core::camera::backend();
    let cameras = match camera_backend.as_ref() {
        Ok(backend) => match backend.discover() {
            Ok(cameras) => cameras.into_iter().filter(|c| selects(c.pid, &c.id, "")).collect::<Vec<_>>(),
            Err(error) => { eprintln!("Camera discovery: {error}"); Vec::new() },
        },
        Err(error) => { eprintln!("Camera discovery: {error}"); Vec::new() },
    };
    let devices: Vec<Device> = discover(&api, &db)
        .into_iter()
        .filter(|d| selects(d.pid, &d.key(), &d.serial))
        .collect();
    #[cfg(target_os = "linux")]
    if hidraw_blocked(&api) {
        eprintln!("Note: Linux won't let this user open some Razer devices. Install the udev rule that comes");
        eprintln!("with Secretarybird (see the README, Linux), then unplug and replug them.");
    }

    if command == "report" {
        let report = secbird_core::report::build(&api, &db);
        if args.iter().any(|a| a == "--json") {
            let json = serde_json::to_string_pretty(&report).map_err(|e| Error::Db(e.to_string()))?;
            println!("{json}");
        } else {
            print!("{report}");
        }
        return Ok(());
    }

    if command == "dock-test" {
        return dock_test(&devices, args.get(1).map(String::as_str));
    }

    if command == "list" {
        if devices.is_empty() && cameras.is_empty() {
            println!("No Razer devices found.");
        }
        for d in &devices {
            let known = if d.spec.is_some() { "" } else { "  (not in database)" };
            let serial = if d.serial.is_empty() { String::new() } else { format!(", serial {}", d.serial) };
            println!("{:04X}  {}  [interface {}, txid {:02X}{serial}]{known}", d.pid, d.name(), d.interface, d.txid);
        }
        for camera in &cameras {
            println!("{:04X}  {}  [UVC, id {}]{}", camera.pid, camera.name, camera.id,
                camera.control_error.as_ref().map(|e| format!(" (controls unavailable: {e})")).unwrap_or_default());
        }
        for pid in secbird_core::device::unanswered(&api, &devices).into_iter().filter(|p| selects(*p, "", "")) {
            if db.get(pid).is_some_and(|spec| spec.kind == secbird_core::Kind::Webcam) { continue; }
            match db.get(pid) {
                Some(spec) if spec.lighting == secbird_core::Lighting::Kraken => {
                    println!("{pid:04X}  {}  (connected; its headset protocol isn't supported yet)", spec.name)
                }
                Some(spec) => println!("{pid:04X}  {}  (connected but not answering{})", spec.name, NOT_ANSWERING_HINT),
                None => match db.bluetooth_name(pid) {
                    Some(name) => println!("{pid:04X}  {name}  (connected over Bluetooth, not answering; use the cable or the USB receiver, and please share `secbird report`)"),
                    None => println!("{pid:04X}  Unknown Razer device  (connected, not answering; please share `secbird report`)"),
                },
            }
        }
        #[cfg(windows)]
        {
            let synapse = secbird_core::debloat::synapse_running();
            if !synapse.is_empty() {
                println!("
Note: Razer Synapse is running ({}). It can overwrite these settings;", synapse.join(", "));
                println!("close it, or run `secbird debloat` to stop it starting with Windows.");
            }
        }
        return Ok(());
    }

    if command == "camera" || (selector.is_some() && !cameras.is_empty())
        || (devices.is_empty() && cameras.len() == 1 && !matches!(command.as_str(), "sync" | "firmware")) {
        let camera = camera::select(&cameras, selector.as_deref())?;
        let backend = camera_backend.map_err(|error| Error::System(error.to_string()))?;
        let mut store = Store::load(&dir.join("profiles.json"))?;
        return camera::run(backend.as_ref(), camera, &command, &args[1..], &mut store);
    }
    if !cameras.is_empty() && !devices.is_empty() && !matches!(command.as_str(), "sync" | "firmware") {
        return Err(Error::InvalidArg("several devices connected; select one with -d (see secbird list)".into()));
    }

    if command == "firmware" {
        use secbird_core::firmware::{latest_for, status, Status};
        if devices.is_empty() {
            return Err(Error::NotFound("no matching Razer device".into()));
        }
        let connected: Vec<u16> = devices.iter().map(|d| d.pid).collect();
        for dev in &devices {
            let current = dev.firmware_version()?;
            match status(current, latest_for(dev.pid)?, &connected) {
                Status::UpToDate { current } => println!("{}: {current}, up to date", dev.name()),
                Status::NoneListed { current } => println!("{}: {current} (Razer lists no firmware for it)", dev.name()),
                Status::Update { current, latest } => {
                    println!("{}: {current} -> {} available", dev.name(), latest.version);
                    println!("  Download: {}", latest.url);
                    if let Some(guide) = latest.guide.filter(|g| *g != latest.url) {
                        println!("  Guide:    {guide}");
                    }
                }
            }
        }
        return Ok(());
    }
    if command == "sync" {
        let color: Rgb = args.get(1).ok_or_else(|| Error::InvalidArg("usage: sync <RRGGBB>".into()))?.parse()?;
        let mut store = Store::load(&dir.join("profiles.json"))?;
        let results = actions::sync_color(&devices, &mut store, color);
        if results.is_empty() {
            println!("No connected device has lighting control enabled.");
        }
        let mut failed = 0;
        for r in &results {
            match &r.result {
                Ok(()) => println!("  ok    {} {}", r.device, r.zone),
                Err(e) => {
                    failed += 1;
                    println!("  FAIL  {} {}: {e}", r.device, r.zone);
                }
            }
        }
        if failed > 0 {
            return Err(Error::Unsupported(format!("{failed} zone(s) could not be set")));
        }
        return Ok(());
    }

    // `key` with a mouse and one keyboard connected means the keyboard (#39).
    let keyboards: Vec<&Device> = devices
        .iter()
        .filter(|d| {
            use secbird_core::Kind;
            matches!(d.spec.as_ref().map(|s| s.kind), Some(Kind::Keyboard | Kind::Keypad | Kind::Laptop))
        })
        .collect();
    let dev = match devices.as_slice() {
        [] => return Err(Error::NotFound("no matching Razer device".into())),
        [one] => one,
        _ if command == "key" && keyboards.len() == 1 => keyboards[0],
        _ => {
            return Err(Error::InvalidArg(
                "several devices connected; pick one with -d PID or -d SERIAL (see `secbird list`)".into(),
            ))
        }
    };
    let rest = &args[1..];
    let mut store = Store::load(&dir.join("profiles.json"))?;

    match command.as_str() {
        "info" => info(dev, &mut store),
        "lighting" => {
            match rest.first().map(String::as_str) {
                None => {}
                Some(v @ ("on" | "off")) if rest.len() == 1 => actions::set_lighting_control(dev, &mut store, v == "on")?,
                _ => return Err(Error::InvalidArg("usage: lighting [on|off]".into())),
            }
            println!("Lighting control: {}", if store.for_device(dev).lighting_off { "off (RGB left untouched)" } else { "on" });
            Ok(())
        }
        "battery" => match dev.battery()? {
            None => {
                println!("Wired device, no battery.");
                Ok(())
            }
            Some(p) => {
                let state = if dev.charging()? { "charging" } else { "discharging" };
                println!("{p}% ({state})");
                Ok(())
            }
        },
        "dpi" => match rest.first() {
            None => {
                let (x, y) = dev.dpi()?;
                println!("{x}x{y}");
                Ok(())
            }
            Some(v) => {
                let (x, y) = parse_dpi(v)?;
                let (x, y) = actions::set_dpi(dev, &mut store, x, y)?;
                println!("DPI set to {x}x{y}");
                Ok(())
            }
        },
        "stages" => {
            if rest.is_empty() {
                let s = dev.dpi_stages()?;
                for (i, (x, y)) in s.stages.iter().enumerate() {
                    let mark = if i + 1 == s.active as usize { "*" } else { " " };
                    println!("{mark} {}  {x}x{y}", i + 1);
                }
                return Ok(());
            }
            let mut values = rest.to_vec();
            let active = match values.iter().position(|a| a == "--use") {
                Some(i) => {
                    let v = values.get(i + 1).ok_or_else(|| Error::InvalidArg("--use needs a stage number".into()))?;
                    let n = v.parse().map_err(|_| Error::InvalidArg(format!("bad stage '{v}'")))?;
                    values.drain(i..=i + 1);
                    n
                }
                None => 1,
            };
            let stages = values.iter().map(|v| parse_dpi(v)).collect::<Result<Vec<_>, _>>()?;
            let sent = actions::set_dpi_stages(dev, &mut store, &secbird_core::DpiStages { active, stages })?;
            let list: Vec<String> = sent.stages.iter().map(|(x, y)| if x == y { x.to_string() } else { format!("{x}x{y}") }).collect();
            println!("DPI stages set to {} (stage {} in use)", list.join(", "), sent.active);
            Ok(())
        }
        "sleep" => match rest.first().map(String::as_str) {
            None => {
                match dev.sleep_timer()? {
                    None => println!("Wired device, no sleep timer."),
                    Some(t) => {
                        let off = if t.auto_sleep == Some(false) { " (sleeping is off)" } else { "" };
                        println!("Sleeps after {} min{off}; 1 to {} min allowed", t.minutes, t.max_minutes);
                    }
                }
                Ok(())
            }
            Some(v @ ("on" | "off")) => {
                actions::set_auto_sleep(dev, &mut store, v == "on")?;
                println!("Sleeping when idle is {v}");
                Ok(())
            }
            Some(v) => {
                let minutes = v.parse().map_err(|_| Error::InvalidArg(format!("bad minutes '{v}'")))?;
                actions::set_sleep_minutes(dev, &mut store, minutes)?;
                println!("Sleeps after {minutes} min");
                Ok(())
            }
        },
        "dimming" => match rest.first() {
            None => {
                match dev.dimming()? {
                    None => println!("Only laptops have keyboard dimming."),
                    Some(d) => println!("Keyboard when idle: {}", dimming_text(d)),
                }
                Ok(())
            }
            Some(v) => {
                let d = v.parse()?;
                actions::set_dimming(dev, &mut store, d)?;
                println!("Keyboard when idle: {}", dimming_text(d));
                Ok(())
            }
        },
        "gaming" => match rest.first() {
            None => {
                match dev.game_mode()? {
                    None => println!("Only keyboards and keypads have a gaming mode."),
                    Some(g) => println!("Gaming mode: {}", game_mode_text(g)),
                }
                Ok(())
            }
            Some(v) => {
                let g = v.parse()?;
                actions::set_game_mode(dev, &mut store, g)?;
                println!("Gaming mode: {}", game_mode_text(g));
                Ok(())
            }
        },
        "macro" => macro_cmd(dev, &mut store, rest),
        "poll" => match rest.first() {
            None => {
                println!("{} Hz", dev.polling_rate()?);
                Ok(())
            }
            Some(v) => {
                let hz = v.parse().map_err(|_| Error::InvalidArg(format!("bad rate '{v}'")))?;
                actions::set_polling(dev, &mut store, hz)?;
                println!("Polling rate set to {hz} Hz");
                Ok(())
            }
        },
        "color" => {
            let (zone, color) = two(rest, "color <zone> <RRGGBB>")?;
            actions::set_color(dev, &mut store, zone, color.parse::<Rgb>()?)
        }
        "brightness" => {
            let (zone, pct) = two(rest, "brightness <zone> <0-100>")?;
            let pct: u8 = pct.parse().map_err(|_| Error::InvalidArg(format!("bad brightness '{pct}'")))?;
            actions::set_brightness(dev, &mut store, zone, pct)
        }
        "effect" => {
            let (zone, effect) = two(rest, "effect <zone> <effect>")?;
            actions::set_effect(dev, &mut store, zone, effect.parse()?)?;
            println!("{zone}: {effect}");
            Ok(())
        }
        "off" => {
            let zone = rest.first().ok_or_else(|| Error::InvalidArg("usage: off <zone>".into()))?;
            actions::lights_off(dev, &mut store, zone)
        }
        "keys" => {
            let color: Rgb = rest.first().ok_or_else(|| Error::InvalidArg("usage: keys <RRGGBB>".into()))?.parse()?;
            let m = dev.matrix().ok_or_else(|| Error::Unsupported(format!("{} has no per-key lighting", dev.name())))?;
            actions::set_keys(dev, &mut store, vec![color; m.rows as usize * m.cols as usize])?;
            println!("All {} keys set to #{color}", m.rows as usize * m.cols as usize);
            Ok(())
        }
        "apply" => {
            let profiles = store.for_device(dev);
            report_apply(profiles.apply(&profiles.active, dev))
        }
        "button" => button_cmd(dev, &mut store, rest),
        "key" => key_cmd(dev, &mut store, rest),
        "profile" => profile_cmd(dev, &mut store, rest),
        other => Err(Error::InvalidArg(format!("unknown command '{other}' (try --help)"))),
    }
}

fn profile_cmd(dev: &Device, store: &mut Store, rest: &[String]) -> Result<(), Error> {
    let sub = rest.first().map(String::as_str).unwrap_or("list");
    let name = rest.get(1).map(String::as_str);
    let need_name = || Error::InvalidArg(format!("usage: profile {sub} <name>"));
    let profiles = store.for_device(dev);

    match sub {
        "list" => {
            for name in profiles.profiles.keys() {
                let mark = if *name == profiles.active { "*" } else { " " };
                println!("{mark} {name}");
            }
            Ok(())
        }
        "show" => {
            let name = name.unwrap_or(&profiles.active).to_string();
            let p = profiles
                .profiles
                .get(&name)
                .ok_or_else(|| Error::NotFound(format!("profile '{name}'")))?;
            println!("Profile '{name}':");
            if let Some((x, y)) = p.dpi {
                println!("  dpi        {x}x{y}");
            }
            if let Some(hz) = p.polling {
                println!("  polling    {hz} Hz");
            }
            if let Some(s) = &p.dpi_stages {
                let list: Vec<String> = s.stages.iter().map(|(x, y)| format!("{x}x{y}")).collect();
                println!("  stages     {} (stage {} in use)", list.join(", "), s.active);
            }
            if let Some(m) = p.sleep_minutes {
                println!("  sleep      after {m} min");
            }
            if let Some(on) = p.auto_sleep {
                println!("  sleeping   {}", if on { "on" } else { "off" });
            }
            if let Some(d) = &p.dimming {
                println!("  dimming    {d}");
            }
            if let Some(g) = &p.game_mode {
                println!("  gaming     {g}");
            }
            for (zone, z) in &p.lighting {
                let state = match (z.off, z.color) {
                    (true, _) => "off".to_string(),
                    (false, Some(c)) => format!("#{c}"),
                    (false, None) => "unchanged".to_string(),
                };
                let bright = z.brightness.map(|b| format!(", {b}%")).unwrap_or_default();
                println!("  {zone:<10} {state}{bright}");
            }
            if !p.apps.is_empty() {
                println!("  apps       {}", p.apps.join(", "));
            }
            for (button, action) in &p.buttons {
                println!("  {button:<10} {action}");
            }
            if *p == Default::default() {
                println!("  (empty: change a setting to record it)");
            }
            Ok(())
        }
        "new" => {
            let name = name.ok_or_else(need_name)?;
            profiles.create(name)?;
            store.save()?;
            println!("Created profile '{name}' (copy of the active one). Switch with: secbird profile use {name}");
            Ok(())
        }
        "use" => {
            let name = name.ok_or_else(need_name)?;
            profiles.switch(name)?;
            let results = profiles.apply(name, dev);
            store.save()?;
            println!("Active profile: {name}");
            report_apply(results)
        }
        "apps" => {
            let name = name.ok_or_else(|| Error::InvalidArg("usage: profile apps <name> [program ...]".into()))?;
            profiles.set_apps(name, &rest[2..])?;
            let apps = profiles.profiles[name].apps.join(", ");
            store.save()?;
            if apps.is_empty() {
                println!("Profile '{name}' no longer switches with any program.");
            } else {
                println!("Profile '{name}' comes up while one of these is in front: {apps}");
                println!("(The tray app does the switching; it goes back to the active profile afterwards.)");
            }
            Ok(())
        }
        "delete" => {
            let name = name.ok_or_else(need_name)?;
            profiles.delete(name)?;
            store.save()?;
            println!("Deleted profile '{name}'");
            Ok(())
        }
        other => Err(Error::InvalidArg(format!("unknown profile command '{other}'"))),
    }
}

fn macro_cmd(dev: &Device, store: &mut Store, rest: &[String]) -> Result<(), Error> {
    use secbird_core::macros::Script;
    let id = |v: Option<&String>| -> Result<u16, Error> {
        let v = v.ok_or_else(|| Error::InvalidArg("which macro? (see `secbird macro`)".into()))?;
        v.parse().map_err(|_| Error::InvalidArg(format!("bad macro id '{v}'")))
    };
    match rest.first().map(String::as_str) {
        None | Some("list") => {
            let st = dev.macro_storage()?;
            let list = dev.macros()?;
            if list.is_empty() {
                println!("No macros on {}.", dev.name());
            }
            for m in &list {
                println!("  {:>3}  {:<32} {} bytes", m.id, m.name, m.size);
            }
            let recyclable = match st.recyclable_bytes {
                0 => String::new(),
                n => format!(" ({n} more from deleted macros, not reusable yet)"),
            };
            println!("Memory: {} of {} bytes free{recyclable}", st.free_bytes, st.max_bytes);
            Ok(())
        }
        Some("show") => {
            let id = id(rest.get(1))?;
            let info = dev.macro_info(id)?;
            println!("{} ({id}): {}", info.name, Script(dev.macro_steps(id)?));
            Ok(())
        }
        Some("new") => {
            let name = rest.get(1).ok_or_else(|| Error::InvalidArg("usage: macro new <name> <steps>".into()))?;
            let script: Script = rest[2..].join(" ").parse()?;
            let id = dev.create_macro(name, &script.0)?;
            println!("Stored macro {id} \"{name}\" ({} steps). Play it from a button: button <name> macro:{id}", script.0.len());
            Ok(())
        }
        Some("delete") => {
            let id = id(rest.get(1))?;
            let plays = |a: &String| a.strip_prefix("macro:").and_then(|v| v.split('@').next()) == Some(id.to_string().as_str());
            let used: Vec<String> = store
                .for_device(dev)
                .profiles
                .iter()
                .filter(|(_, p)| p.buttons.values().any(plays))
                .map(|(name, _)| name.clone())
                .collect();
            if !used.is_empty() {
                return Err(Error::InvalidArg(format!("a button in {} plays macro {id}; change that button first", used.join(", "))));
            }
            dev.delete_macro(id)?;
            println!("Deleted macro {id}");
            Ok(())
        }
        Some(other) => Err(Error::InvalidArg(format!("unknown macro command '{other}'"))),
    }
}

fn button_cmd(dev: &Device, store: &mut Store, rest: &[String]) -> Result<(), Error> {
    use secbird_core::buttons::{button_id, button_label, button_name, Layer};
    let layer = if rest.iter().any(|a| a == "--hypershift") { Layer::HyperShift } else { Layer::Normal };
    let args: Vec<&str> = rest.iter().map(String::as_str).filter(|a| *a != "--hypershift").collect();
    match args.as_slice() {
        [] | ["list"] => {
            let buttons = dev.buttons();
            if buttons.is_empty() {
                return Err(Error::Unsupported(format!("{} has no on-board button assignments", dev.name())));
            }
            for (id, normal) in buttons {
                let name = button_name(id);
                let action = match layer {
                    Layer::Normal => normal.to_string(),
                    Layer::HyperShift => dev.button(id, layer).map_or_else(|e| e.to_string(), |a| a.to_string()),
                };
                println!("  {name:<14} {action:<24} {}", button_label(id));
            }
            Ok(())
        }
        ["restore-all"] => {
            let n = actions::restore_all_buttons(dev, store)?;
            println!("Restored {n} button(s) to what the mouse stores.");
            Ok(())
        }
        ["save-to-mouse"] => {
            let slot = actions::save_buttons_to_mouse(dev, store)?;
            let n = store.for_device(dev).active().buttons.len();
            println!("Saved {n} remapped button(s) in on-board slot {slot}; `secbird button put-back` undoes it.");
            Ok(())
        }
        ["put-back"] => {
            let n = actions::put_back_onboard(dev, store)?;
            println!("Put {n} button(s) in the mouse's slot back to what they were.");
            Ok(())
        }
        [button, "reset"] => {
            button_id(button).ok_or_else(|| Error::InvalidArg(format!("unknown button '{button}' (see `secbird button`)")))?;
            actions::set_button(dev, store, button, layer, None)?;
            println!("{button}: back to {}", dev.button(button_id(button).unwrap_or(0), layer)?);
            Ok(())
        }
        [button, action] => {
            button_id(button).ok_or_else(|| Error::InvalidArg(format!("unknown button '{button}' (see `secbird button`)")))?;
            actions::set_button(dev, store, button, layer, Some(action.parse()?))?;
            let on = if layer == Layer::HyperShift { " (Hypershift)" } else { "" };
            println!("{button}{on} now does {action}");
            Ok(())
        }
        _ => Err(Error::InvalidArg("usage: button [list|restore-all|save-to-mouse|put-back] | button <button> <action|reset> [--hypershift]".into())),
    }
}

fn key_cmd(dev: &Device, store: &mut Store, rest: &[String]) -> Result<(), Error> {
    use secbird_core::buttons::{Input, Layer};
    let layer = if rest.iter().any(|a| a == "--hypershift") { Layer::HyperShift } else { Layer::Normal };
    let args: Vec<&str> = rest.iter().map(String::as_str).filter(|a| *a != "--hypershift").collect();
    // `capslock` or `key-capslock`; stored in the profile as `key-capslock`.
    let input = |key: &str| -> Result<Input, Error> {
        let name = if key.starts_with("key-") { key.to_string() } else { format!("key-{key}") };
        Input::parse(&name).ok_or_else(|| Error::InvalidArg(format!("unknown key '{key}' (see `secbird --help`)")))
    };
    let unsupported = || {
        use secbird_core::Kind;
        match dev.spec.as_ref().map(|s| s.kind) {
            Some(Kind::Keyboard | Kind::Keypad | Kind::Laptop) => Error::Unsupported(format!(
                "key remapping isn't supported on {} yet: it doesn't take Razer's remap commands",
                dev.name()
            )),
            None => Error::Unsupported(format!(
                "{} doesn't answer key remapping commands; please send `secbird report` in a GitHub issue",
                dev.name()
            )),
            Some(_) => Error::Unsupported(format!("{} has no keyboard keys (see `secbird button`)", dev.name())),
        }
    };
    match args.as_slice() {
        [] | ["list"] => {
            let keys = dev.keys();
            if keys.is_empty() {
                return Err(unsupported());
            }
            for (id, normal) in keys {
                let key = Input::Key(id);
                let action = match layer {
                    Layer::Normal => normal.to_string(),
                    Layer::HyperShift => dev.input(key, layer).map_or_else(|e| e.to_string(), |a| a.to_string()),
                };
                let name = key.name();
                println!("  {:<16} {action:<24} {}", &name["key-".len()..], key.label());
            }
            Ok(())
        }
        ["restore-all"] => {
            let n = actions::restore_all_buttons(dev, store)?;
            println!("Restored {n} key(s) and button(s) to what the device stores.");
            Ok(())
        }
        [key, "reset"] => {
            let input = input(key)?;
            actions::set_button(dev, store, &input.name(), layer, None)?;
            println!("{key}: back to {}", dev.input(input, layer)?);
            Ok(())
        }
        [key, action] => {
            let input = input(key)?;
            let action = action.parse()?;
            match actions::set_button(dev, store, &input.name(), layer, Some(action)) {
                // A keyboard that doesn't take key commands at all, rather than this one key.
                Err(Error::Device { .. } | Error::NoReply(_)) if dev.input(Input::Key(31), Layer::Normal).is_err() => {
                    return Err(unsupported())
                }
                other => other?,
            }
            let on = if layer == Layer::HyperShift { " (Hypershift)" } else { "" };
            println!("{key}{on} now does {action}");
            Ok(())
        }
        _ => Err(Error::InvalidArg("usage: key [list|restore-all] | key <key> <action|reset> [--hypershift]".into())),
    }
}

/// One write to a mouse dock's own light, to check what Synapse's dock code says: effect
/// 0F:02 on LED 15, sent with the dock's transaction id (#39).
fn dock_test(devices: &[Device], what: Option<&str>) -> Result<(), Error> {
    use secbird_core::device::{DOCK_LED, DOCK_TXID};
    let args: &[u8] = match what {
        Some("red") => &[1, DOCK_LED, 0x01, 0, 0, 1, 0xFF, 0, 0],
        Some("spectrum") => &[1, DOCK_LED, 0x03, 0, 0, 0],
        Some("battery") => &[1, DOCK_LED, 0x0E, 0, 0, 0],
        Some("off") => &[1, DOCK_LED, 0x00, 0, 0, 0],
        _ => return Err(Error::InvalidArg("usage: dock-test red|spectrum|battery|off".into())),
    };
    let dock = devices.iter().find(|d| d.is_mouse_dock()).ok_or_else(|| Error::NotFound("a Mouse Dock Pro".into()))?;
    let command = secbird_core::commands::MATRIX_EFFECT;
    match dock.transact_as(DOCK_TXID, command, args.len() as u8, args) {
        Ok(_) => println!("The dock accepted {command} with txid {DOCK_TXID:02X}. Did its light change?"),
        Err(e) => println!("The dock didn't take it: {e}"),
    }
    Ok(())
}

fn report_apply(results: Vec<secbird_core::profile::Applied>) -> Result<(), Error> {
    if results.is_empty() {
        println!("Nothing to apply: the profile has no recorded settings yet.");
    }
    let mut failed = 0;
    for r in &results {
        match &r.result {
            Ok(()) => println!("  ok    {}", r.setting),
            Err(e) => {
                failed += 1;
                println!("  FAIL  {}: {e}", r.setting);
            }
        }
    }
    if failed > 0 {
        return Err(Error::Unsupported(format!("{failed} setting(s) could not be applied")));
    }
    Ok(())
}

fn dimming_text(d: secbird_core::Dimming) -> &'static str {
    use secbird_core::Dimming::*;
    match d {
        StaysLit => "stays lit",
        Half => "dims to half",
        Quarter => "dims to a quarter",
        Dark => "turns off until a key is pressed",
    }
}

fn game_mode_text(g: secbird_core::GameMode) -> String {
    if g.is_off() {
        return "off".into();
    }
    let keys: Vec<&str> = [(g.win, "Windows key"), (g.alt_tab, "Alt+Tab"), (g.alt_f4, "Alt+F4")]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
    format!("on, blocks {}", keys.join(", "))
}

fn info(dev: &Device, store: &mut Store) -> Result<(), Error> {
    let (major, minor) = dev.firmware()?;
    println!("Device    {} ({:04X})", dev.name(), dev.pid);
    println!("Serial    {}", dev.serial()?);
    println!("Firmware  v{major}.{minor}");
    let has_sensor = dev.spec.as_ref().is_none_or(|s| s.dpi.is_some());
    if has_sensor {
        let (x, y) = dev.dpi()?;
        println!("DPI       {x}x{y}");
    }
    match dev.polling_rate() {
        Ok(hz) => println!("Polling   {hz} Hz"),
        Err(e) => println!("Polling   unavailable ({e})"),
    }
    if let Some(p) = dev.battery()? {
        println!("Battery   {p}%");
    }
    if let Ok(s) = dev.dpi_stages() {
        let list: Vec<String> = s.stages.iter().map(|(x, _)| x.to_string()).collect();
        println!("Stages    {} (stage {} in use)", list.join(", "), s.active);
    }
    if let Ok(Some(t)) = dev.sleep_timer() {
        let off = if t.auto_sleep == Some(false) { ", sleeping off" } else { "" };
        println!("Sleep     after {} min{off}", t.minutes);
    }
    if let Ok(Some(d)) = dev.dimming() {
        println!("Dimming   {}", dimming_text(d));
    }
    if let Ok(Some(g)) = dev.game_mode() {
        println!("Gaming    {}", game_mode_text(g));
    }
    if let Some(spec) = &dev.spec {
        if !spec.leds.is_empty() {
            let zones: Vec<&str> = spec.leds.iter().map(|l| l.name.as_str()).collect();
            println!("Lighting  {}", zones.join(", "));
        }
        if let Some(m) = spec.matrix {
            println!("Per-key   {} rows x {} columns", m.rows, m.cols);
        }
    }
    println!("Profile   {}", store.for_device(dev).active);
    Ok(())
}

fn parse_dpi(v: &str) -> Result<(u16, u16), Error> {
    let bad = || Error::InvalidArg(format!("bad DPI '{v}'"));
    let mut parts = v.split(['x', 'X']);
    let x: u16 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let y: u16 = match parts.next() {
        Some(p) => p.parse().map_err(|_| bad())?,
        None => x,
    };
    Ok((x, y))
}

fn two<'a>(rest: &'a [String], usage: &str) -> Result<(&'a str, &'a str), Error> {
    match rest {
        [a, b, ..] => Ok((a, b)),
        _ => Err(Error::InvalidArg(format!("usage: {usage}"))),
    }
}

/// A Razer `/dev/hidraw*` node this user may not open: the udev rule isn't installed.
#[cfg(target_os = "linux")]
fn hidraw_blocked(api: &HidApi) -> bool {
    api.device_list().filter(|d| d.vendor_id() == secbird_core::db::RAZER_VID).any(|d| {
        let path = std::path::Path::new(d.path().to_str().unwrap_or_default());
        std::fs::OpenOptions::new().read(true).write(true).open(path).is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
    })
}

#[cfg(windows)]
const NOT_ANSWERING_HINT: &str = "; is Synapse running?";
#[cfg(not(windows))]
const NOT_ANSWERING_HINT: &str = "";

#[cfg(not(windows))]
fn debloat_cmd(_: &[String]) -> Result<(), Error> {
    Err(Error::Unsupported("debloat is Windows-only".into()))
}

#[cfg(windows)]
fn debloat_cmd(rest: &[String]) -> Result<(), Error> {
    use secbird_core::debloat::{self, Backup};

    let yes = rest.iter().any(|a| a == "-y" || a == "--yes");
    let sub = rest.iter().find(|a| !a.starts_with('-')).map(String::as_str).unwrap_or("show");
    let backup_path = config_dir().join("debloat-backup.json");

    match sub {
        "show" => {
            let scan = debloat::scan()?;
            print_scan(&scan);
            let plan = debloat::plan(&scan);
            if plan.is_empty() {
                println!("
Nothing to turn off.");
                return Ok(());
            }
            println!("
`secbird debloat apply` would run these Windows commands:");
            print_plan(&plan);
            println!("
This needs administrator rights: open PowerShell with \"Run as administrator\".");
            println!("Original settings are saved first; `secbird debloat undo` puts them back.");
            Ok(())
        }
        "apply" => {
            let scan = debloat::scan()?;
            let plan = debloat::plan(&scan);
            print_scan(&scan);
            if plan.is_empty() {
                println!("
Nothing to turn off.");
                return Ok(());
            }
            println!("
Commands to run:");
            print_plan(&plan);
            require_admin(&plan)?;
            if !confirm(yes) {
                println!("Cancelled, nothing changed.");
                return Ok(());
            }
            let mut backup = Backup::load(&backup_path)?.unwrap_or_default();
            backup.record(&scan);
            backup.save(&backup_path)?;
            println!("Original settings saved to {}", backup_path.display());
            run_plan(&plan)?;
            println!("Done. Undo any time with `secbird debloat undo`.");
            Ok(())
        }
        "undo" => {
            let Some(backup) = Backup::load(&backup_path)? else {
                println!("Nothing to undo: `secbird debloat apply` has not been run on this PC (no backup at {}).", backup_path.display());
                println!("Services you stopped by hand can be started again in services.msc.");
                return Ok(());
            };
            let plan = debloat::undo_plan(&backup);
            println!("Commands to run:");
            print_plan(&plan);
            require_admin(&plan)?;
            if !confirm(yes) {
                println!("Cancelled, nothing changed.");
                return Ok(());
            }
            run_plan(&plan)?;
            std::fs::remove_file(&backup_path).map_err(|e| Error::Db(format!("{}: {e}", backup_path.display())))?;
            println!("Restored. Backup removed.");
            Ok(())
        }
        other => Err(Error::InvalidArg(format!("unknown debloat command '{other}' (show, apply, undo)"))),
    }
}

#[cfg(windows)]
fn print_scan(scan: &secbird_core::debloat::Scan) {
    println!("Razer background services:");
    if scan.services.is_empty() {
        println!("  (none)");
    }
    for s in &scan.services {
        let state = if s.running { "running" } else { "stopped" };
        let mem = s.memory.map(|b| format!("{:>5} MB", b / (1024 * 1024))).unwrap_or_default();
        println!("  {:<34} {state}, {:<20} {mem}", s.display_name, s.start.to_string());
        println!("    {}", s.image);
    }
    println!("Startup entries:");
    if scan.run_entries.is_empty() {
        println!("  (none)");
    }
    for r in &scan.run_entries {
        println!("  {}  ({})", r.name, r.key);
        println!("    {}", r.data);
    }
}

#[cfg(windows)]
fn print_plan(plan: &[secbird_core::debloat::Cmd]) {
    for c in plan {
        println!("  {c}");
    }
    println!(r"  (sc.exe and reg.exe are the built-in tools from C:\Windows\System32)");
}

#[cfg(windows)]
fn require_admin(plan: &[secbird_core::debloat::Cmd]) -> Result<(), Error> {
    if plan.iter().any(|c| c.needs_admin()) && !secbird_core::debloat::is_elevated() {
        return Err(Error::System(
            "these commands need administrator rights. Open PowerShell with \"Run as administrator\"              and run this again. Nothing was changed."
                .into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn confirm(yes: bool) -> bool {
    use std::io::Write;
    if yes {
        return true;
    }
    print!("Run these commands? [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).is_ok() && matches!(line.trim(), "y" | "Y" | "yes")
}

#[cfg(windows)]
fn run_plan(plan: &[secbird_core::debloat::Cmd]) -> Result<(), Error> {
    let mut failed = 0;
    for c in plan {
        match c.run() {
            Ok(()) => println!("  ok    {c}"),
            Err(e) => {
                failed += 1;
                println!("  FAIL  {c}
        {e}");
            }
        }
    }
    if failed > 0 {
        return Err(Error::System(format!("{failed} command(s) failed")));
    }
    Ok(())
}
