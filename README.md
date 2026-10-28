<picture>
  <source media="(prefers-color-scheme: light)" srcset="docs/banner-light.png">
  <img src="docs/banner.png" alt="Secretarybird: your Razer gear, minus the bloat." width="100%">
</picture>

<div align="center">
  <a href="https://github.com/infinityInfinite/secbird/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/infinityInfinite/secbird?style=flat"></a>
  <a href="https://github.com/infinityInfinite/secbird/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/infinityInfinite/secbird/total.svg?style=flat"></a>
  <a href="https://github.com/infinityInfinite/secbird/commits"><img alt="Commit activity" src="https://img.shields.io/github/commit-activity/m/infinityInfinite/secbird?style=flat"></a>
  <a href="https://github.com/infinityInfinite/secbird/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/infinityInfinite/secbird?style=flat"></a>
  <a href="#legal"><img alt="License" src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=flat"></a>
</div>

> [!NOTE]
> Secretarybird is in beta and changes often. **Star** ⭐ and **Watch** 👀 the repo to hear when a
> new release is out.

A small, lightweight replacement for Razer Synapse on Windows. Set your Razer mouse,
keyboard and other peripherals: lighting, DPI, polling rate, button remapping and profiles, without
Synapse's background services, account, ads, pop-ups or restart prompts.

- **Light:** a 1 MB tray app using about 12 MB of RAM, next to the hundreds of MB Synapse keeps running.
- **Quiet:** no services, no account, no telemetry. It sits in the tray and
  applies your settings when you sign in, plug a device in or wake the PC. The only time it goes
  online is a daily check for a new Secretarybird version, which you can turn off.
- **Built for many devices:** one screen for all of them, and one click to put a color or effect on every device, in step.

> **Beta.** We test on a Basilisk V3, and users have confirmed nine more devices, from Naga and
> Viper mice to BlackWidow keyboards and a Tartarus keypad (see below). Other devices are
> supported from public protocol facts and may not work yet. [Tell us how yours does](#help-us-support-your-device).

### [⬇ Download for Windows](https://github.com/infinityInfinite/secbird/releases/latest) · [⬇ Download for Linux](#linux-command-line-beta)

Free · Windows 10 and 11 · on the page that opens, click `Secretarybird-Setup-<version>.exe`.
On Linux it's the command line, in beta: see [Linux](#linux-command-line-beta).

![All devices](docs/overview.png)

## Beyond Synapse

Things Secretarybird does that Synapse won't:

- **Run on Linux.** The command line works on any x86_64 distribution.
- **Compare profiles.** Every profile of a device side by side on one screen.
- **Script it.** The [`secbird` command line](#command-line) does what the editor does: DPI,
  lighting, remaps, macros, profiles.
- **Plain files.** Your settings are one `profiles.json`; back it up or copy it to another PC.
- **Share the lights.** Hand a device's RGB to another app and keep using Secretarybird for
  everything else.
- **Clean up after Razer.** [`secbird debloat`](#debloat) turns off Razer's background services,
  and `debloat undo` puts them back.

## Install

1. Go to the [**latest release**](https://github.com/infinityInfinite/secbird/releases/latest) and download
   `Secretarybird-Setup-<version>.exe`.
2. Run it. It installs for your Windows account only, so it doesn't ask for administrator rights.
3. The bird appears in the tray (click `^` next to the clock if it's hidden).
   Click it and choose **Open editor**.

**"Windows protected your PC"?** The installer isn't code-signed yet (signing costs money),
so Windows SmartScreen doesn't know it. Click **More info → Run anyway**. To check your download
matches the release, compare `Get-FileHash Secretarybird-Setup-<version>.exe` in PowerShell
with `SHA256SUMS.txt` on the release page.

Prefer no installer? Download `Secretarybird-<version>-portable.zip` instead, unzip it anywhere
and run `secbird-tray.exe`.

**Updating:** run the newer installer; your settings are kept.
**Uninstalling:** Windows Settings → Apps → Secretarybird. It asks whether to keep your profiles.

### Linux (command line, beta)

The `secbird` command line runs on Linux: DPI, polling rate, lighting and effects, button
remaps, macros, profiles and reports, the same as on Windows. The tray and the editor are
Windows-only for now.

1. Download and unpack the latest version. It's one static binary for any x86_64 distribution:
   ```sh
   url=$(curl -s https://api.github.com/repos/infinityInfinite/secbird/releases/latest | grep -o 'https://[^"]*linux-x86_64[.]tar[.]gz')
   curl -sL "$url" | tar xz
   ```
   Or download `secbird-<version>-linux-x86_64.tar.gz` from the
   [latest release](https://github.com/infinityInfinite/secbird/releases/latest) and run
   `tar xf secbird-*-linux-x86_64.tar.gz`.
2. Install the binary and the udev rule, which lets you use Razer devices without root:
   ```sh
   cd secbird-*-linux-x86_64
   sudo ./install.sh
   ```
3. Unplug the device and plug it back in, then run `secbird list`.

Settings live in `~/.config/secretarybird`. `sudo ./install.sh --uninstall` removes it again.
If OpenRazer is installed, its daemon and Secretarybird can both change the same device; use
one at a time. Bug reports from Linux are very welcome, with `secbird report` output.

## Using it

Mouse DPI and DPI stages can be edited independently for the X (horizontal) and Y
(vertical) axes. **Link X/Y axes** makes subsequent edits apply to both; toggling
it alone keeps the current values. DPI presets explicitly apply to both axes.

![A Naga with six profiles side by side in Compare](docs/mouse.png)

| Naga side grid, a button playing a macro | Keyboard, per-key |
|---|---|
| ![Button editor](docs/buttons.png) | ![Keyboard page](docs/keyboard.png) |

<sub>Screenshots: the simulated devices built into the app, next to a real Basilisk V3.</sub>

- **Tray icon:** switch profiles per device, re-apply settings, Start with Windows. Hover it to see
  each device's profile and battery.
- **Editor:** opens in your browser but runs only on your PC (`127.0.0.1`), protected by a
  one-time key. Made to be quick mid-game: it opens on the live profile, with your profiles in a
  switcher on top; pick a section (Lighting, Sensitivity, Buttons, Macros, Power, Profile,
  Device), click a tile, change it in the side panel. Nothing scrolls on a desktop window, even
  a Naga with all its side buttons. Compare shows every profile side by side. Seven themes
  (Bridge, Roast, Classic, Light, Workshop, Neon, Acid), or Auto to follow Windows.
- **Lighting:** colors and brightness per zone, per-key painting on per-key keyboards, and the
  effects the device runs itself (spectrum, wave, breathing, reactive, starlight; each device
  only has some of them). Verified on the Basilisk V3 (spectrum, wave).
- **Leave RGB to another app:** open a device's **Lighting → Control lighting** tile and
  turn off **Secretarybird controls lighting** (CLI: `secbird -d SERIAL lighting off`).
  It leaves the lights as they are, not off: no colors, effects, brightness or per-key writes,
  including Sync, profile switches, startup, replug and wake. DPI, buttons and other settings
  still work. This choice is saved per device, across all profiles; saved colors are kept.
  Turn it back on, then edit lighting or apply a profile to use them again. Other apps may
  still conflict over non-lighting settings; this is not an exclusive device lock.
- **Sensitivity:** DPI, the DPI stages the DPI button cycles through, and polling rate. DPI stages
  verified on the Basilisk V3.
- **Button remapping** (mice with on-board button memory): keys and shortcuts, turbo, sniper/DPI
  clutch, DPI and profile buttons, media keys, double click, disable, and a **HyperShift** layer for
  a second function while a HyperShift button is held. Changes go to the mouse's live memory, so
  what Synapse or the factory saved stays put. Every change has Undo, **Restore all** puts every
  button back (also in the tray menu, which works from the keyboard), and the app won't let you
  remap away your last left click. Verified on the Basilisk V3, and by users on the Naga V2
  HyperSpeed's 12-button side grid and HyperShift. The Naga's 6-button plate is included too.
- **Macros** stored on the device: keys, shortcuts, mouse buttons, wheel and delays, typed as steps
  or recorded in the editor. Play one from any button once, several times, while held, or as a
  toggle; it runs without Secretarybird open. Macros made in Synapse show up too. Verified on
  the Basilisk V3.
- **Power:** sleep timer and battery level for wireless mice. On Blade laptops, whether the
  keyboard dims or turns off when idle (Smart Dimming; not yet tested on a real laptop).
- **Firmware check:** asks Razer's public product page whether newer firmware is out and links to
  Razer's updater (on request, or weekly if you turn it on in the tray). Nothing is installed.
- **New versions:** once a day the tray asks GitHub whether a newer Secretarybird is out and tells
  you once per version. Click the notice or **Update to X** in the tray menu and it downloads the
  installer, checks its signature and installs it quietly; nothing is installed without that click.
  Turn the check off with "Tell me when a new version is out" in the tray menu.
- **Popups:** a small popup above the clock when you change DPI with the mouse's button, switch
  its on-board profile, or when a per-app profile kicks in. The editor follows DPI changes too.
- **Profiles** are saved per physical device, so two identical mice keep their own settings.
  Back them up with `secbird export backup.json`.
- **Per-app profiles:** tie a profile to games or apps (the Profile section in the editor, or
  `secbird profile apps Gaming cs2.exe`). It switches while that program is in front and goes
  back when you leave it, button remaps included. Windows tells the tray when the window in front
  changes, so nothing polls.

## Moving from Synapse

1. Close Synapse (tray icon → Exit). If both run, Synapse overwrites your settings, and
   Secretarybird warns you about that.
2. Optional: turn off Razer's background services and startup entries with `secbird debloat`
   (below), or uninstall Synapse completely. Your devices keep working: Secretarybird talks to
   them directly and doesn't need Razer's services.

## Command line

The installer can add `secbird` to PATH (tick the box); otherwise run it from the install folder,
`%LOCALAPPDATA%\Programs\Secretarybird`.

```
secbird list                    connected devices
secbird info                    firmware, serial, DPI, polling rate, battery
secbird dpi 1600                set DPI
secbird poll 1000               set polling rate
secbird stages 400 800 1600 --use 2
                                DPI stages the DPI button cycles through
secbird sleep 10                wireless: sleep after 10 minutes idle
secbird dimming quarter         Blade laptops: dim the keyboard to a quarter when idle
secbird macro new Copy "tap:ctrl+c"
                                store a macro on the device
secbird button side-back macro:1
                                play it from a button
secbird color logo FF8800       static color for a zone
secbird effect logo spectrum    built-in effect: spectrum, wave, breathing, reactive, starlight
secbird keys 00FFAA             every key one color (per-key keyboards)
secbird firmware                is newer firmware out? (links to Razer's updater)
secbird update                  is a newer Secretarybird out?
secbird sync 00FFAA             same color on every device
secbird profile new gaming      profiles per device
secbird profile apps gaming cs2.exe   switch to a profile while a program is in front
secbird button                  what each mouse button does
secbird button side-forward key:ctrl+c   remap a button (add --hypershift for that layer)
secbird button side-forward reset        back to what is saved in the mouse
secbird export backup.json      save all profiles to a file
secbird report                  read-only description of your hardware, for issues
secbird --version
```

Run `secbird help` for everything. With several devices connected, pick one with `-d PID` or
`-d SERIAL`.

### Debloat

- `secbird debloat` only **lists** what Razer runs in the background.
- `secbird debloat apply` turns it off. It needs a terminal opened with **Run as administrator**,
  prints every Windows command before running it, and saves a backup.
- `secbird debloat undo` puts back exactly what `apply` changed.

Drivers are never touched.

## Help us support your device

Right-click the tray icon → **Make a device report…** (it copies the report and opens the form), or
run `secbird report` and [open a device report](https://github.com/infinityInfinite/secbird/issues/new?template=device-report.yml),
saying what works and what doesn't. The report only reads from your devices and shortens serial
numbers. Reports are how devices move from "listed" to "verified".

Please send a report rather than a pull request that edits `devices.json`: the report shows every
value the device answers with, so each entry can be checked against it before it ships.

### Verified on real hardware

**Mice**
- Basilisk V3 (ours)
- Cobra, wired ([#1](https://github.com/infinityInfinite/secbird/issues/1))
- Naga V2 Pro ([#4](https://github.com/infinityInfinite/secbird/issues/4), [#13](https://github.com/infinityInfinite/secbird/issues/13))
- Naga V2 HyperSpeed, with side grid, battery and HyperShift ([#8](https://github.com/infinityInfinite/secbird/issues/8), [#11](https://github.com/infinityInfinite/secbird/issues/11))
- Viper Ultimate, wired and wireless ([#14](https://github.com/infinityInfinite/secbird/issues/14))
- DeathAdder V3 Pro, through a HyperPolling dongle ([#17](https://github.com/infinityInfinite/secbird/pull/17))

**Keyboards and keypads**
- BlackWidow 2019 ([#12](https://github.com/infinityInfinite/secbird/issues/12))
- BlackWidow V4 Low-Profile HyperSpeed, wireless and wired ([#28](https://github.com/infinityInfinite/secbird/pull/28), [#31](https://github.com/infinityInfinite/secbird/pull/31))
- Cynosa V2
- Tartarus V2 ([#14](https://github.com/infinityInfinite/secbird/issues/14))

**Accessories**
- Mouse Dock ([#14](https://github.com/infinityInfinite/secbird/issues/14))

### Reported working by users

A user said it works, but there's no `secbird report` yet. One from them moves it to verified.

- Basilisk X HyperSpeed, on the 2.4 GHz dongle ([#10](https://github.com/infinityInfinite/secbird/issues/10))
- Basilisk V3 Pro 35K Phantom Green Edition
- BlackWidow V3 Mini HyperSpeed
- Goliathus Chroma
- Thunderbolt 4 Dock Chroma

### Partly working

- Basilisk V4 Pro on the Mouse Dock Pro: DPI, polling, sleep and most buttons work ([#16](https://github.com/infinityInfinite/secbird/issues/16))

### Listed, waiting for reports

About 260 more mice, keyboards, keypads, mousepads, laptops and accessories, including the
Basilisk V4 Pro, Basilisk V4 HyperSpeed and Naga V3 Pro.

### Not supported yet

- Headsets: Kraken, BlackShark V3 ([#15](https://github.com/infinityInfinite/secbird/issues/15))
- Bluetooth connections ([#10](https://github.com/infinityInfinite/secbird/issues/10))
- Laptop cooling pads
- Chroma Addressable RGB Controller (shows up, but has no setup screen yet)

## Files

Settings live in `%APPDATA%\Secretarybird` (on Linux, `~/.config/secretarybird`):
`profiles.json`, `secbird.log` (helpful for bug reports) and an optional `devices.json`
override. The tray menu has **Open settings folder**.

## Build from source

The editor tests use Node.js's built-in test runner (no npm packages):

```sh
node --test crates/secbird-tray/tests/*.test.cjs
```

Needs Rust (stable, MSVC) on Windows 10/11.

```
cargo build --release
```

The installer is built with [Inno Setup 6](https://jrsoftware.org/isinfo.php):
`ISCC.exe /DAppVersion=0.1.0 installer\secbird.iss`. Pushing a `v*` tag builds the installer,
portable zip and checksums on GitHub into a draft release.

To try the editor without hardware, start the tray with simulated devices:

```
$env:SECBIRD_SIM="all"   # or a list, e.g. "naga,blackwidow,blade,firefly,dock"
.\target\release\secbird-tray.exe
```

## Legal

Secretarybird contains no code or assets from Razer. Device facts are restated from the OpenRazer
project; see [NOTICE](NOTICE). Razer, Synapse and Chroma are trademarks of Razer Inc.; this project
is not affiliated with or endorsed by Razer.

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
