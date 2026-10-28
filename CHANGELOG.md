# Changelog

## [Unreleased]

### Added
- Scroll wheel settings on mice with a HyperScroll wheel: the wheel mode (tactile, free-spin,
  precision tactile), which modes the wheel-mode button skips, scroll acceleration and Smart-Reel,
  each off or at a level 0-4 like Synapse's sliders (the new Scroll wheel section, or
  `secbird scroll mode free-spin`, `secbird scroll smart-reel medium`). Saved per profile. Read
  and level ranges checked on a Naga V3 Pro on a Mouse Dock Pro.

## [0.5.0] - 2026-10-27

### Added
- Lights when away: while the PC is locked or its screen is off, the lights dim to a quarter or
  turn off, and come back when you return (Settings → Lights when away; #48, thanks a3k7dotmd).
  Nothing is saved on the devices, so a replug brings the normal lights back. Devices with the
  older lighting commands stay lit. Tested on the Basilisk V3.
- Gaming mode on keyboards and keypads: block the Windows key, Alt+Tab and Alt+F4 (the new Gaming
  section, or `secbird gaming win,alt-tab`). It's saved per profile, so a profile tied to a game
  turns it on while you play. The keyboard does the blocking, the way Synapse asks it to; the
  section shows only on keyboards that answer for it. Not tested on a keyboard yet.
- Save to mouse: Buttons → Saved on the mouse writes a profile's button remaps into the mouse's
  on-board memory, so they work without Secretarybird (another PC, Linux, macOS, before the tray
  starts) and survive sleep. What the mouse held before is kept, and "Put back what the mouse
  had" restores it; Restore all puts it back too. Remaps stay live-only until you click, and
  switching profiles never writes the mouse's memory (`secbird button save-to-mouse`,
  `secbird button put-back`). Tested on the Basilisk V3.
- Keyboard key remapping: the Keys section in the editor (click a key on the drawing, Normal and
  HyperShift layers), or `secbird key capslock key:esc` (`secbird key` lists them; `reset`,
  `restore-all`, `--hypershift`). Same actions as mouse buttons, saved per profile, and the tray
  sends them on a profile switch. It uses the keyboard's own remap commands, so it works on boards
  that take them: confirmed on the Huntsman V3 Pro and BlackWidow V4 75% (#45, #39). Older boards
  such as the BlackWidow Chroma V2, Ornata V3 X and Reclusa X Mini don't, and the editor says
  remapping isn't supported there yet; a remap done by Secretarybird itself is planned (#49, #58,
  #59). `secbird report` shows whether a keyboard answers.
- Boards whose lights aren't mapped are drawn by size in Keys: 75% and 65% boards get their own
  drawing instead of a full-size one (#39).
- A Tab button next to Ctrl, Shift, Alt and Win when setting a key, for shortcuts the page can't
  catch: click Tab, then Alt, for Alt+Tab (#51, thanks LukeSz03).

### Fixed
- Effects on older keyboards (the BlackWidow Chroma V2 and others on the older lighting commands)
  fell back to a static colour. Spectrum, wave, breathing, reactive and starlight are now sent the
  way openrazer sends them (#49, thanks liuzhong-xin). Confirmed on the Chroma V2.
- With no device the editor could control, every Settings and Help button failed with "invalid
  argument" (the device report included) (#51).
- A mouse or keyboard connected over Bluetooth shows as "DeathAdder V2 Pro (Bluetooth)" and so
  on, with "use the cable or the USB receiver", instead of "Unknown model". Bluetooth can't be
  controlled yet (#51, #10).
- The Basilisk V3 Pro and V4 Pro were missing their underglow. It's now a lighting zone, on the
  Mouse Dock Pro too (#57, thanks Xnmk029; #54, thanks Vojtasanko).

### Changed
- The Reclusa X Mini 65% (02E3) is known by name (#53).
- 02EA is the Huntsman V3 HE TKL (#50, thanks Toru1475).
- The Basilisk V4 Pro's button 128 is the extra side button, not a top button (#54, thanks
  Vojtasanko). It's now `side-extra`; remaps saved as `top-extra` move over by themselves.

## [0.4.12] - 2026-10-24

### Added
- Mouse Dock Pro: the dock's own light is a lighting zone, "Dock" (#39, thanks esperianterra).
  Static colour, spectrum, wave and breathing are confirmed on a real dock; reactive and starlight
  showed nothing there, so they aren't offered. The dock can also show the docked mouse's
  battery, green to red, updated every 5 minutes; the dock doesn't have Synapse's battery effect
  built in.

### Fixed
- Per-key colours still went dark after quick adjustment mode on the Huntsman V3 Pro (#45). The
  keyboard doesn't say when it leaves that mode, so the tray now sends the per-key colours of
  wired keyboards and keypads again every 3 seconds, as Synapse keeps doing. It pauses while the
  light finder is open. Wireless boards are left out, so they can still sleep. The 0.4.11 change
  that saved the per-key effect on the keyboard is undone: it didn't help, and without the tray
  it could leave the keyboard dark after a power cycle.
- Huntsman V3 Pro per-key layout from the light-finder list (#45, #27, thanks Skillprofi10).
  Painting numpad `.` lit the volume dial. The media keys and the dial are now in the editor.
- Blade laptops set to "Keyboard when idle: stays lit" still faded by themselves (#34, thanks
  garyh-2). Any read restarts that fade, so the tray reads the setting back every 3 seconds. This
  only works while the tray runs.
- `secbird report` dropped zero bytes at the end of short replies, so smart dimming's `01 00 00`
  showed as `01` (#34).

## [0.4.11] - 2026-10-23

### Added
- Per-key colours: a "Find which light is which key" mode. It lights one light at a time; you type
  the key that lit up and copy the list into an issue, so boards whose layout is wrong or unknown
  can be mapped (#45, #27, #41). Nothing is saved on the device, and your lighting comes back when
  you leave.

### Fixed
- Per-key colours went dark after leaving quick adjustment mode on the Huntsman V3 Pro. The per-key
  effect is now also saved to the keyboard, as the single backlight colour already was (#45, #27,
  thanks Skillprofi10). Not tested on the keyboard yet.
- DeathStalker V2: Numpad 0 lit nothing after the 0.4.9 layout shift (#41, thanks a3k7dotmd).

## [0.4.10] - 2026-10-22

### Added
- Naga V2 Pro: the side grid and the profile light underneath are their own zones (#13, thanks
  Wastelander121).
- Per-key colours: a layout picker with UK (ISO) and German (QWERTZ) beside US. ISO boards get
  their key left of Z and the one left of Enter, and lose the backslash key they don't have (#45,
  thanks Skillprofi10).

### Fixed
- Wireless mice lost their button remaps after sleeping until the profile was switched (#45).
  The tray checks one remapped button every 20 seconds and sends the remaps again if they're gone.
- Tartarus V2: colour and brightness changes did nothing. It uses the newer lighting commands, not
  the older ones the device list had (#46, thanks fen0z).

## [0.4.9] - 2026-10-21

### Added
- Mouse Dock Pro test (#39): `secbird report` also asks the dock itself for its firmware,
  lighting and pairing settings, and `secbird dock-test red|spectrum|battery|off` tries setting
  the dock's own light once. Both follow Synapse's dock code and haven't been tried on a dock yet.
- `secbird report` shows whether a mouse has sensor rotation in its firmware.

### Fixed
- DeathStalker V2 (Pro, wired and wireless): per-key colours lit the key to the right of the one
  painted, and the leftmost key of each row couldn't be coloured (#41, thanks a3k7dotmd). The
  DeathStalker V2 Pro TKL gets the same correction and now shows a keyboard layout; not confirmed
  on that board yet.

## [0.4.8] - 2026-10-20

### Added
- Viper V4 Pro and Huntsman V3 HE (#36, thanks Toru1475).
- Naga V3 Pro: the 12-button side plate's light is its own zone, "Side plate" (#26, thanks
  Skillprofi10).
- `secbird report` shows which on-board slot is active.

### Fixed
- Colours, effects and brightness now go to the device's active on-board slot. A keyboard or
  mouse that Synapse left on slot 2 read the changes back but never showed them (#37, thanks
  a3k7dotmd).
- Turning sleep off now lasts past a power cycle: it is saved on the device too (#37).
- A wireless device that sleeps until its first keypress after Windows starts still gets its
  profile: the tray keeps looking for it once a minute for half an hour (#37). Until then it's
  listed as "Asleep? Press a key or move it".
- Restoring a backup on another PC puts the profiles on the mouse connected there. They went to
  the other PC's mouse (matched by serial number) and never showed up (#40, thanks Rubyurek).
- Laptops that answer a blank serial number show "none" in the report instead of garbage.

## [0.4.7] - 2026-10-19

### Added
- BlackWidow V4 Low-Profile HyperSpeed on the cable (#31, thanks lukethehawk). The wireless one
  is now named "(Wireless)" so the two read apart when both are connected.

### Fixed
- Keyboards no longer show a "0 DPI" control (#32, thanks lukethehawk). Only mice have DPI.

## [0.4.6] - 2026-10-18

### Added
- Pick your own low battery warning level: Settings → Low battery warning takes any level from
  1% to 95% (it was always 15%). Type it, or use − and +.
- All devices is easier to read: bigger cards grouped by kind (mice, keyboards and keypads,
  headsets, everything else), each with an icon for its kind, and DPI, polling rate and battery
  at a glance. Long device names no longer get cut off.
- BlackWidow V4 Low-Profile HyperSpeed (#28, thanks lukethehawk). Verified over 2.4 GHz.

## [0.4.5] - 2026-10-17

### Added
- Linux (beta): the `secbird` command line, as one static binary for any x86_64 distribution,
  with a udev rule and `install.sh` so it works without root. DPI, polling, lighting, remaps,
  macros, profiles and reports work as on Windows; the tray and editor are Windows-only for now.
  When Linux blocks access to a Razer device, `secbird` says how to fix it.

## [0.4.4] - 2026-10-17

### Added
- Update from the tray: click the new-version notice, **Update to X** in the tray menu, or
  Settings → Update now in the editor. Secretarybird downloads the installer, checks it carries
  the release signature, installs it quietly and starts again with your settings. A download
  without a valid signature is never run. The unzipped (portable) copy still opens the release
  page instead.
- Key remaps have Ctrl, Shift, Alt and Win toggles next to the key field. Windows keeps some
  shortcuts for itself (Win+D, Win+L) and the editor never sees them: press the key, then click
  the modifier (#25).
- `secbird report` reads a laptop's keyboard backlight brightness three ways (#24).

### Fixed
- Naga V3 Pro: the scroll wheel light can be coloured (#26).

## [0.4.3] - 2026-10-17

### Added
- Smart Dimming on Blade laptops (#20): the editor's Power section has "Keyboard when idle"
  (stays lit, half, a quarter, or off), and the CLI has `secbird dimming`. Not yet tried on a
  real laptop; please tell us if it works on yours.
- Turn off lighting control per device (#19, thanks Monratis): Lighting → Control lighting, or
  `secbird lighting off`. Secretarybird then never touches that device's lights, so another RGB
  app can run them. DPI, buttons and the rest keep working.
- HyperPolling dongle (#17, thanks Monratis): the editor names the mouse paired to it, and polling
  rate changes stick after the mouse wakes. Verified on a DeathAdder V3 Pro.
- Numpad keys and the Menu key for button remaps and macros.
- The Basilisk V4 Pro's extra top button has its name (#16). Tartarus V2 Pro (#21).
- Blade, tenkeyless and mini keyboards are drawn with their own layout in the per-key editor.
  When a keyboard's layout isn't known, the editor says so.

### Fixed
- Macros: `tap:` and `click:` hold the key for 50 ms, so games that check keys once per frame
  see every press (#22). Macro writes that the device reads back empty are retried.
- Lone modifier keys (Left Ctrl, Right Alt and so on) show their names instead of a code (#16).
- `secbird report` reads Smart Dimming the way Blade laptops answer it.
- Editor: focus outlines are no longer cut off in the device and profile rows, long device
  names wrap, and in a narrow window tile values wrap and the per-key keyboard scrolls sideways
  instead of shrinking.

## [0.4.2] - 2026-10-15

### Added
- A Settings page (the gear in the top bar): theme and motion; start with Windows; switching
  profiles with games; DPI and profile popups, the low battery warning, new version notices and
  the weekly firmware check, each with an on/off switch; back up every profile to a file and
  restore from one; open the settings folder; copy a device report and open a GitHub issue; put
  every mouse button back to what the mouse stores.
- Use a zone's lighting elsewhere: a zone's panel has "Use this lighting on: All zones · Every
  device", which copies its colour or effect (with its options, like wave direction or the
  breathing colour). Effects start together on every device, so a mouse, keyboard, mousepad and
  dock cycle in step; each device keeps its own timer, so they can drift apart over time, and
  copying again lines them up. A device that can't run the effect (older lighting, or firmware
  without it, like the Basilisk V3 and breathing) is named and left as it is.
- Basilisk V4 Pro, Basilisk V4 HyperSpeed, Naga V3 Pro and the Basilisk V3 Pro 35K Xbox Edition
  (wired and wireless), so they are named and set up right, also on the Mouse Dock Pro (#16).
- `secbird report` reads the mouse's own button list.

### Changed
- DPI panel: one readout card with − and + (steps of 50) and the value typed in place, a
  full-width slider with the range at its ends, and presets in an even row with the current one
  filled. Sliders everywhere take the theme's colours instead of the browser's default bar.
- Themes: each has its own colour for what is active (the live profile, current section,
  selected tile and selected option), filled solid as Bridge already did; the profile underline
  is gone. Light is a soft light blue, Workshop forest green, Roast latte cream and Classic
  azure; Bridge and Neon stay as they were. New: Acid, black with vivid green.

### Fixed
- BlackWidow 2019: the device list had the wrong transaction id (3F; it answers on 1F). It
  worked anyway because Secretarybird tries the others, but it now finds it first time (#12).
- "Same color on all zones" copied the first zone's colour, not the one just changed. It is now
  "Same on all zones" and copies the colour or effect of the zone you changed last.
- Buttons showed every button Secretarybird knows, Naga side grids included, on a mouse behind
  the Mouse Dock Pro (#16). The editor now asks the mouse which buttons it has, as Synapse does,
  and shows only those; buttons without a name yet appear as "Button 120", and functions only
  Synapse knows as "Set by Synapse".

## [0.4.1] - 2026-10-14

### Added
- New-version notice: once a day the tray asks GitHub whether a newer Secretarybird is out and
  shows a notification once per version; click it, or "Get Secretarybird …" at the top of the
  tray menu, for the download. On by default; turn it off with "Tell me when a new version is
  out" in the tray menu. "Check for updates now" checks right away, and `secbird update` does
  the same from the command line. It sends nothing but the request and installs nothing.

### Fixed
- Compare showed buttons that play a macro as "Macro #1"; it now shows the macro's name.

## [0.4.0] - 2026-10-14

### Changed
- Redesigned editor, built to be quick mid-game: it opens on the live profile, with a profile
  switcher on top, settings in sections (Lighting, Sensitivity, Buttons, Macros, Power, Profile,
  Device), each shown as tiles, and changes made in a fixed side panel. Nothing scrolls on a
  desktop window, even a Naga with all its buttons; the Naga's side grid is drawn as a keypad.
  Compare shows every profile side by side, as before.
- Themes: Bridge (charcoal with the logo's red-orange; the new default dark), Roast (warm brown), Classic
  (the previous look), Light, Workshop and Neon, picked from the header; Auto follows Windows.
- Many devices or profiles: the device tabs and the profile switcher show arrows and a fade
  where more are hidden, scroll with the mouse wheel, and keep the selected one in view.
- Faster to open: buttons and macros are read from the device only when their section is opened,
  and the editor no longer re-reads the serial number on every refresh.

### Added
- Button safety: a change that would leave the mouse without any left click is refused; every
  button change offers Undo for 10 seconds; "Restore all" (editor Buttons section, tray menu
  "Restore all mouse buttons", `secbird button restore-all`) puts every remapped button back
  to what the mouse stores and forgets the remaps, so they aren't applied again.
- Naga side buttons: the 12-button grid (`grid-1` to `grid-12`) and the 6-button side plate
  (`hex-1` to `hex-7`) can be remapped on both layers (issues #4, #8). Button ids come from
  Synapse; untested on real hardware. They only appear on mice that report having them.
- DPI stages: set the 1-5 DPI values the mouse's DPI button cycles through, and which one is in
  use (`secbird stages`, editor row "DPI stages"). Stored on the mouse and in profiles. Verified
  on a Basilisk V3.
- Macros, stored on the device so they play without any software running: keys and
  shortcuts, mouse buttons, wheel and delays. Make them by typing steps
  (`tap:ctrl+c wait:50 click:left`) or by recording in the editor, and play one from any
  button: once, several times, while held, or on/off (`macro:3`, `macro:3@5`, `macro:3@hold`,
  `macro:3@toggle`). Macros made in Synapse show up too. `secbird macro`, and the editor's
  "Macros" group. Storing, reading back, playing from a button and deleting verified on a
  Basilisk V3.
- Lighting effects the device runs by itself, per zone: spectrum cycling, wave (direction and
  speed 1-5), breathing, reactive and starlight (`secbird effect logo spectrum`, editor "Effect"
  in each lighting zone). Saved in profiles. Devices only run the effects their firmware has:
  the Basilisk V3 runs spectrum and wave (verified) but not breathing, reactive or starlight,
  which Synapse animates from the PC; Secretarybird says so instead of failing silently.
- Firmware update check: `secbird firmware`, a "Check for firmware update" link in the editor,
  and an opt-in weekly check from the tray menu that notifies once per new version. It reads
  the public product page Synapse uses (apps.razer.com) and links to Razer's own updater;
  Secretarybird never installs firmware. The editor now shows the full firmware version.
- Sleep timer for wireless devices: how long the device waits before it sleeps, 1-15 minutes
  (`secbird sleep`, editor group "Power"). Devices with an auto-sleep switch allow up to 60
  minutes and can stop sleeping altogether. Untested on real hardware.

### Fixed
- Mamba Tournament Edition and Diamondback Chroma: the polling rate shows again (reported on
  Reddit). Their device-list entries had none, so v0.3.1 hid it.
- Strider Chroma and Blade 15 Advanced (Early 2021) use the transaction id their owners'
  reports show (1F).
- Mouse Dock Pro: a mouse on the dock can now be controlled (issues #3, #6). Untested on real
  hardware; built from the reporters' `secbird report` output. The dock is the
  mouse's receiver, so it now shows as a wireless mouse with DPI, polling rate (the dock's
  high-speed command), battery and button remapping. Secretarybird asks the dock which mouse is
  paired and uses that mouse's name, DPI range and lighting zones; if it can't tell, it uses
  generic settings that work for any of them.

### Changed
- Device reports without a terminal: tray menu → "Make a device report…" copies the report and
  opens the GitHub form.
- `secbird report` on Blade laptops (and devices not in the list) also reads fan, performance,
  GPU and battery-limit settings, so owners' reports show what fan and power controls need.
  Still read-only.
- Blade 15 Advanced (Early 2021): the lid logo is a lighting zone (on/off, brightness), from a
  user's report (#7). Untested; other Blades get it as reports come in.
- `secbird report` also shows which mice a receiver or dock says are paired (`00:BF`).

## [0.3.3] - 2026-10-14

### Changed
- Device reports without a terminal: tray menu → "Make a device report…" copies the report and
  opens the GitHub form; paste it in.
- `secbird report` on Razer Blade laptops (and devices not in the list) also reads fan,
  performance, GPU and battery-limit settings, so owners' reports show what fan and power
  controls need. Still read-only: a report changes nothing on the device.

## [0.3.2] - 2026-10-14

### Fixed
- Cobra (wired): the device list now has its real transaction id (1F, was FF), from a user's
  `secbird report`. It worked before only because discovery falls back to the other ids.

### Changed
- Cobra (wired) is verified on real hardware: logo lighting, DPI and polling rate (issue #1).

## [0.3.1] - 2026-10-14

### Fixed
- Mousepads and docks no longer show a polling rate in the editor; it only appears where the
  device list says the device has one.

### Changed
- README lists devices users reported working (Cobra wired, Basilisk V3 Pro 35K Phantom,
  BlackWidow V3 Mini HyperSpeed, Goliathus Chroma, Thunderbolt 4 Dock Chroma), separate from
  the ones verified on our own hardware.

## [0.3.0] - 2026-10-13

### Added
- Button remapping for mice with on-board button memory, in the editor and with `secbird button`:
  mouse buttons, keys and shortcuts (captured by pressing them), turbo for keys and clicks,
  DPI up/down/cycle and sniper clutch, on-board profile switching, media keys, double click,
  disable, and a HyperShift layer. Written to the mouse's live slot; the saved slots keep
  Synapse's or the factory setting and Reset copies it back. Remaps live in profiles, so
  per-app switching changes them too. Verified on the Basilisk V3 (key and button remaps).
- Popup above the clock when the mouse's DPI button or on-board profile button is used, and
  when a per-app profile switches. Read from the mouse's own event reports: no polling.
- DPI changed with the mouse's button is saved to the profile in use and shown in the editor.

### Changed
- Redesigned editor: each device is one sheet with settings as rows and profiles as columns,
  every value visible and editable in place; the active profile is lit and the light moves when
  it changes. Games are managed in each profile's header. Works in narrow windows.

### Fixed
- A turbo setting only Synapse can make (wheel tilt) showed the wrong rate in the editor.

## [0.2.0] - 2026-10-13

### Added
- Per-app profiles: tie a profile to games or apps and it switches while that program is
  in front, then goes back to the active profile. Set it in the editor's Profile card (with
  suggestions from programs that have a window open) or with `secbird profile apps`.
  Event-driven (Windows foreground notifications, no polling); adds about 27 KB to the tray.

## [0.1.0] - 2026-10-13

First public beta. Tested on real hardware with the Basilisk V3 only; other devices are
listed from public protocol facts and need your reports (`secbird report`).

### Added
- Tray app that applies each device's profile at sign-in, on plug-in and after sleep, and
  retries devices that are slow to appear (4, 15 and 45 seconds after start or wake).
- Editor in your browser: lighting per zone, brightness, DPI, polling rate, profiles,
  per-key keyboard colors, an overview of all devices, and "Sync color everywhere".
- `secbird` command line for the same settings, plus `report`, `export` and `import`.
- Profiles per physical device, keyed by serial number, so two identical mice keep
  separate settings.
- Battery level in the tray tooltip and a low-battery alert at 15%.
- Warning when Razer Synapse is running and may overwrite settings.
- `secbird debloat`: lists Razer's background services and startup entries, turns them
  off on request (administrator, every command printed first) and can undo it.
- Log file at `%APPDATA%\Secretarybird\secbird.log`.
- Installer for the current user (no administrator needed) and a portable zip.

### Known limits
- Windows 10/11 only.
- Kraken headsets and laptop cooling pads are not supported yet.
- A wireless mouse connected by cable may show up twice.
- The installer is not code-signed, so Windows SmartScreen asks for confirmation.
