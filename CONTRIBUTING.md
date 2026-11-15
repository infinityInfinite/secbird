# Contributing to Secretarybird

Thanks for wanting to help. Secretarybird replaces Razer Synapse with something small,
open and honest, and it only gets there with people who own devices we don't. This guide
says what we are building, the rules every change keeps, how the code is laid out, and
how to get a pull request merged.

If you only want to add a device, skip to [Adding a device](#adding-a-device): usually
that is a device report, not code.

---

## What we believe

Five rules. Every change is judged by them, in this order. When a change is good for one
and bad for another, the earlier one wins.

### 1. Visible and safe: the user can see everything we do

People install this because they don't trust a vendor app with their PC. We earn that by
having nothing to hide.

- **No telemetry, no account, no analytics, ever.** Not opt-in, not anonymous.
- **The network is used for three things only**, all HTTPS GETs that send nothing but the
  request ([`net.rs`](crates/secbird-core/src/net.rs)):
  the new-version check (api.github.com), the firmware check (apps.razer.com, only when the
  user turns it on or clicks), and downloading an update the user asked for. A new network
  call needs a very good reason, a setting, and a line in the README.
- **Updates are signed.** An installer is run only if it carries the maintainer's signature
  ([`update.rs`](crates/secbird-core/src/update.rs)). A taken-over GitHub account can replace
  files but can't sign them.
- **The editor is local.** It is served on `127.0.0.1` with a per-session token, checks the
  Host and Origin headers, and has a strict Content-Security-Policy
  ([`server.rs`](crates/secbird-tray/src/server.rs)). It loads nothing from the internet: no
  fonts, no scripts, no images.
- **Every change to the system is shown first.** Turning off Razer's services prints the
  exact `sc.exe` / `reg.exe` commands before running them
  ([`debloat.rs`](crates/secbird-core/src/debloat.rs)). The only other system change is the
  "start with Windows" entry under the user's own `Run` key, which is a visible setting.
- **Reads are reads.** `secbird report` sends only GET commands and says so. Commands that
  write to a device are named as writes.
- **Nothing leaves the PC.** Settings live in `%APPDATA%\Secretarybird` as readable JSON;
  the log is a plain capped text file. Serial numbers in reports are shortened.
- **Say what it costs.** If a feature uses CPU, memory, battery or the network, the UI says
  how much, measured (see the RGB Studio note).

A pull request that adds a dependency with network access, a background service, a driver,
or anything that phones home will not be merged.

### 2. Easy to use: a minute, no reading

The user opens the editor between games, changes one thing, closes it.

- The common jobs (a colour, DPI, a button) need no reading and no scrolling.
- Changes apply at once and are saved; there are no Save buttons. Every change can be
  undone or reset to what the device or Synapse had.
- Plain words. "Free-spin", not "SCROLL_MODE 1". Errors say what to do next.
- Honest by default: if something is untested, simulated or a guess, the UI says so.
- Calm: no upsells, no badges, nothing blinking for attention.

The design lives in the editor itself: colours are tokens on `:root`, one set per theme, and
the same parts (rows, tiles, pills, notes) are reused on every screen. Build new screens from
those parts and see [Editor rules](#editor-rules).

### 3. Light: low memory, low CPU, small files

Synapse runs several services and hundreds of megabytes. We don't.

| Today (0.6, Windows) | Size | Memory |
|---|---|---|
| Tray app (`secbird-tray.exe`) | 1.5 MB | about 13 MB working set, 2 MB private |
| Command line (`secbird.exe`) | 1.0 MB | runs and exits |

Keep it there:

- **Few dependencies.** The core uses `serde`, `serde_json` and `hidapi`; Windows APIs go
  through `windows-sys`. No async runtime, no web framework, no GUI toolkit, no Electron.
  Adding a crate needs a reason in the PR, and "it saves twenty lines" isn't one.
- **Idle means idle.** The tray sleeps on Windows messages and timers. Nothing polls faster
  than it must; anything that runs often (RGB Studio at 30 frames a second) stops when it
  isn't needed and slows down for wireless devices.
- **The editor is one self-contained HTML file** embedded in the exe
  ([`editor.html`](crates/secbird-tray/src/editor.html)): no build step, no npm, no framework.
- **Measure.** If your change can cost CPU or memory, measure before and after on a real
  device and put the numbers in the PR.

### 4. New devices are easy to add: data first, code second

Razer releases new products every few months, and we can't buy them. The day someone posts
a report for a mouse that came out last week, adding it should take minutes, not a release
of new code. So the code grows by facts, not by special cases.

- **A device is data.** What a product supports lives in
  [`devices.json`](crates/secbird-core/devices.json), keyed by USB product id. A new device
  that speaks the same protocol as its family needs only an entry there: one file, one
  small PR, easy to review. See [Adding a device](#adding-a-device).
- **Users don't have to wait for us.** A `devices.json` in `%APPDATA%\Secretarybird` is
  merged on top of the built-in list ([`db.rs`](crates/secbird-core/src/db.rs)), so someone
  with a brand-new device can add it and try it the same day, without rebuilding.
- **When a new device needs code, add a capability, not a case.** If the new mouse has a
  setting nobody had before, add it as a feature the spec can switch on (`features`,
  `matrix`, `leds`…), so the next device with the same thing gets it from one line of data.
- **Never design for one device.** The Basilisk V3 is what the maintainer owns; it is not
  the template. Think of Naga grids, keyboards with and without per-key light, keypads,
  mousepads, docks, laptops, headsets and wireless receivers. Test with `SECBIRD_SIM=all`.
- **Features are capabilities.** Ask the device ("does it answer `02:30`?") or read the
  spec; don't check product names in code.
- **Fail soft.** A device that doesn't answer a command shows that setting as unavailable;
  it never breaks the rest.

### 5. Checkable: every fact can be verified and rebuilt

Adding devices fast is only safe if what we add is true. Everything we know about the
protocol should be something the next person can verify.

- **Every protocol fact has a source** in a comment: a device report and its issue number, a
  Synapse capture, OpenRazer, or a test on named hardware. "Seen on a Naga V3 Pro on a Mouse
  Dock Pro (#62)" beats "should work".
- **Every device entry is checked against a `secbird report`** before it ships.
- **Every feature has a simulator test.** The simulator ([`sim.rs`](crates/secbird-core/src/sim.rs))
  answers the protocol the way the real device did, so behaviour is pinned down without
  hardware, and anyone can run the editor against it.
- **Builds are plain `cargo build`**, and CI runs the same clippy and tests on Windows and
  Linux that you run locally.
- **Say what was and wasn't tested** in the PR, the CHANGELOG and, when it matters, the UI.

---

## How the code is laid out

```
crates/
  secbird-core/   the library: protocol, devices, profiles. No UI. Windows + Linux.
  secbird/        the command line (secbird.exe). Thin: parses arguments, calls the core.
  secbird-tray/   the Windows tray app, which also serves the editor.
```

### secbird-core

| File | What it does |
|---|---|
| `proto.rs` | The 90-byte Protocol 2.5 report: build, checksum, parse, status codes |
| `transport.rs` | Talking to HID (feature reports; output/input reports for headsets) |
| `device.rs` | Finding devices, transaction ids, sending a command and reading its reply |
| `commands.rs` | Every command as a named constant (`class:id`), with where it came from |
| `db.rs` + `devices.json` | The device database |
| `features.rs` | Lighting, DPI, polling, battery, sleep and the other device settings |
| `effects.rs` | Built-in lighting effects |
| `buttons.rs`, `macros.rs` | Button remapping and on-device macros |
| `scroll.rs` | HyperScroll wheel modes, acceleration and Smart-Reel |
| `headset.rs` | Headsets on their own 64-byte protocol |
| `studio.rs` | RGB Studio: layered effects rendered on the PC and streamed |
| `profile.rs` | Profiles: what is saved, and applying them to a device |
| `actions.rs` | "Change a setting and save it to the profile", shared by the CLI and tray |
| `report.rs` | `secbird report`: the read-only diagnostic people paste into issues |
| `sim.rs` | Simulated devices for tests and for running the editor without hardware |
| `update.rs`, `firmware.rs`, `net.rs` | The only network code |
| `debloat.rs` | Finding and turning off Razer's services, shown before it runs |
| `log.rs` | The capped log file |

### secbird-tray

| File | What it does |
|---|---|
| `win.rs` | The window, tray icon, menu, timers and Windows messages |
| `engine.rs` | Devices and profiles in memory; applies profiles on plug-in, wake, app switch |
| `server.rs` | The local editor server and its JSON API |
| `editor.html` | The whole editor: markup, CSS and script in one file |
| `apps.rs` | Which program is in front, for per-game profiles |
| `osd.rs` | The small DPI and profile popups |
| `studio.rs`, `sensors.rs`, `input.rs` | RGB Studio's frame timer, audio/screen sensors, key presses |
| `settings.rs` | The tray's own settings |

### How a setting travels

```
editor.html  --POST /api/...-->  server.rs  -->  actions.rs  -->  Device method (features.rs ...)
                                                     |                  |
                                                     v                  v
                                               profile.rs         device.rs -> transport -> USB
                                          (saved, re-applied later)
```

The CLI calls the same `actions.rs` functions, so the command line and the editor always
behave the same way.

---

## Getting started

You need Rust (stable) and, for the editor check, Node.js.

```sh
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
```

Run the editor against simulated devices, without touching your real settings:

```powershell
$env:APPDATA = "$PWD\scratch-appdata"   # keep your real profiles.json out of it
$env:SECBIRD_SIM = "all"                 # or a list: "mouse,keyboard,headset"
.\target\release\secbird-tray.exe --serve
```

It prints a local URL; open it in a browser. `secbird list` and `secbird report` see the
same simulated devices. The presets are listed in `sim.rs` (`PRESETS`).

If you edit `editor.html`, check its script still parses, because a single duplicate name
blanks the whole page:

```sh
python -c "import re;s=open('crates/secbird-tray/src/editor.html',encoding='utf-8').read();open('ed.js','w',encoding='utf-8').write(max(re.findall(r'<script>(.*?)</script>',s,re.S),key=len))"
node --check ed.js
```

---

## Adding a device

1. Plug it in and run `secbird report` (or tray menu → **Make a device report…**). It only
   reads.
2. Open a [device report](https://github.com/infinityInfinite/secbird/issues/new?template=device-report.yml)
   with the output, and say what works and what doesn't.
3. The entry in `devices.json` is made from that report. If you open a PR with the entry
   yourself, link the report; entries without one aren't merged.

An entry looks like:

```json
{"pid":"00AA","name":"Basilisk V3 Pro (Wired)","kind":"mouse","txid":"1F",
 "dpi":{"min":100,"max":30000,"step":50},"polling":"standard","lighting":"extended",
 "leds":[{"name":"logo","id":4},{"name":"scroll","id":1},{"name":"underglow","id":10}],
 "wireless":true,"source":"openrazer; underglow LED 10 hardware-tested on 00AB (#57)"}
```

`source` says where the facts came from. Use the names the editor already knows for zones
(`logo`, `scroll`, `underglow`, `side`, `profile`, `backlight`).

## Adding a feature

A new setting usually touches these, in this order:

1. **`commands.rs`**: the command constant, with a comment saying where it came from.
2. **A `Device` method** in the right core file. Read and write; fail with
   `Error::Unsupported` when the device doesn't answer.
3. **`sim.rs`**: make a simulated device answer it like the real one, and **a test** that
   reads, writes and reads back. Include the awkward cases you saw on hardware.
4. **`profile.rs` + `actions.rs`**: save it to the profile and re-apply it, if it's a
   setting the user expects to keep.
5. **`report.rs`**: add the GET to the report, so the next device can be checked.
6. **The CLI** (`secbird/src/main.rs`) and **the editor** (`server.rs` route +
   `editor.html`), built from the editor's existing parts.
7. **`CHANGELOG.md`**: what it does for the user, what it was tested on, and who helped.

Small and finished beats large and half-done. One feature per pull request.

## Editor rules

- One file, no external anything (rule 1 and 3).
- Use the colour tokens on `:root`; never hard-code a colour a theme should change. Check
  every theme, light and dark.
- Check every kind of device with `SECBIRD_SIM=all`: a mouse with zones, a per-key keyboard,
  a Naga, a keypad, a mousepad, a dock, a headset.
- Phone-width matters less than a 1366 x 768 laptop, but nothing may overflow sideways.
- Keyboard works everywhere: every control is reachable and labelled.
- Text is short, plain and honest. No "Oops!", no "Sorry", no exclamation marks.

## Security checklist for every PR

- [ ] No new network calls, or each one is an HTTPS GET, documented, and behind a setting.
- [ ] No new dependency, or the PR says why and what it pulls in.
- [ ] Nothing new is written outside `%APPDATA%\Secretarybird` (or `~/.config/Secretarybird`).
- [ ] Any change to the system (services, registry, files elsewhere) is shown to the user
      before it runs, as a command they could run themselves.
- [ ] New editor API routes go through the existing token, Host and Origin checks.
- [ ] Nothing personal (serials, paths with user names) goes into logs or reports in full.
- [ ] No device write that could brick a device: no firmware flashing, no bootloader commands.

## Pull request checklist

- [ ] `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
- [ ] The editor script passes `node --check` (if you touched `editor.html`).
- [ ] New device behaviour has a simulator test.
- [ ] Every protocol fact has a source in a comment.
- [ ] The PR says what you tested, on which device, and what you couldn't test.
- [ ] `CHANGELOG.md` has a line under **[Unreleased]**.
- [ ] Screenshots for UI changes, dark and light theme.

## What won't be merged

- Telemetry, analytics, crash uploads, accounts, ads, anything that phones home.
- Firmware flashing or other writes that could brick a device.
- Device-name special cases where a capability check would do.
- Large frameworks or build steps for the editor.
- Device entries without a report behind them.
- Code nobody has run: if you couldn't test on hardware, say so, and test on the simulator.

## Reviews and credit

Pull requests are read in full, built and tested on the simulator, and on real hardware
when we have the device. If we change your PR before merging (to fit the rules above, or
because the code moved on), we say what and why, and you stay credited in the CHANGELOG and
the release notes. Contributions are licensed under the project's license, MIT OR
Apache-2.0.

Questions are welcome in an issue. Reports of security problems: please open an issue that
says only that you found one, and we'll find a private way to talk.
