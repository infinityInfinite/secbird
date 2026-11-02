# Tartarus V2 Pro (02EB): thumbstick, keys and thumb buttons

How Synapse 4 sets the thumbstick, found on one keypad (firmware 1.2) on 2026-11-02 from:

- Synapse's own log of what it sent (`%LOCALAPPDATA%\Razer\RazerAppEngine\User Data\Logs\
  products_747_mw {…}.log`, lines `rzDevice25.<command>() … "dataSend":[…]`), and
- the code of Synapse's product module for this keypad, as cached by Synapse
  (`apps.razer.com/synapse/products/747/mw/`: `rzDevice25.*.js` sends the commands,
  module `3159.*.js` holds their bytes, `6737.*.js` decides when to send them).

All are Protocol 2.5 feature reports on interface 3, transaction id 1F (Synapse used
other transaction ids in the log; the keypad answers 1F). Command arrays in Synapse's code
are `[argument size, class, id]`; bit 7 of the id marks the GET variant.

| Command | Class:id | Size | Arguments | Source |
|---|---|---|---|---|
| Get Thumbstick Output Option | 02:C4 | 2 | `[profile]` → `[profile, option]` | log + code |
| Set Thumbstick Output Option | 02:44 | 2 | `[profile, option]`; option 1 = left stick, 0 = right stick | log + code |
| Get Thumbstick Orientation Options | 02:C5 | 2 | `[profile]` → `[profile, option]` | log + code |
| Set Thumbstick Orientation Options | 02:45 | 2 | `[profile, option]`; option 1 = rotated (90°), 0 = default | log + code |
| Get Analog Joystick Report Mode | 02:B8 | 1 | → `[mode]` | code |
| Set Analog Joystick Report Mode | 02:38 | 1 | `[mode]`; 0 = analog, 1 = directions (4-way/8-way) | code; log shows the call, not the bytes |
| Get Analog Joystick Mapping Mode | 02:B9 | 2 | `[profile]` → `[profile, mode]` | code |
| Set Analog Joystick Mapping Mode | 02:39 | 2 | `[profile, mode]`; 0 = analog, 1 = 4-way, 2 = 8-way (3 = none) | code |

Logged examples (this keypad, profile 1 = the one in use, profiles 2-4 = stored, all 0):

```
Set Thumbstick Output Option       00 0F 00 00 00 02 02 44 01 01 … checksum 44
Set Thumbstick Orientation Options 00 13 00 00 00 02 02 45 01 01 … checksum 45
```

## Changing the input mode

Synapse's `taskRunnerHandleKeypadJoystickAnalogConfig` (in `6737.*.js`):

```
reportMode  = (mode == FOUR_WAY || mode == EIGHT_WAY) ? 1 : 0
setAnalogJoystickReportMode(reportMode)            // 02:38 [reportMode]
mappingMode = (mode == FOUR_WAY || mode == EIGHT_WAY) ? mode : 0
setAnalogJoystickMappingMode(1, mappingMode)       // 02:39 [1, mappingMode]
```

with `FOUR_WAY = 1, EIGHT_WAY = 2, NONE = 3`.

## What the modes do

- In 4-way/8-way mode the keypad reports the stick as direction inputs, which Synapse names
  `KEY_UP_ARROW`, `KEY_UP_RIGHT_ARROW`, … and maps to keys like any other key (a remap,
  e.g. `KEY_UP_RIGHT_ARROW` → W). The firmware decides the zones.
- In analog mode the stick becomes a game controller. Razer's own driver for this keypad
  (`RzDev_02eb.sys`, a lower filter on USB interface 1, installed with Synapse into
  `C:\Program Files (x86)\Razer\PID02ebDrv`) creates the controller Windows sees: a
  virtual HID joystick (`RZVIRTUAL\VID_1532&PID_02EB&MI_00&Col02`, "Gamepad (Razer Tartarus
  V2 Pro)"). Its INF also lists a virtual Xbox 360 endpoint, off by default.
- Synapse's settings for this keypad name two input reports: 4 (`razerKeyReportID`) and 11
  (`analogKeyboardV2ReportID`).

## The stick's position in 4-way and 8-way mode

Recorded with `secbird listen` on 2026-11-02 (a read-only log of every collection):

- In analog mode the only reports while the stick moves come from the controller Razer's
  driver adds (interface 0, usage 0001:0005).
- In 4-way and 8-way mode, and also in 8-way with every direction set to Disable in Synapse,
  the keypad sends report **0x0B** on interface 1 (usage 0001:0000, 48 bytes, about 250 a
  second while the stick moves). After the id come 3-byte entries `[input, value hi, value
  lo]`, signed big-endian, for each input away from rest; a report with no entries means
  everything is at rest. The stick is inputs **0xFB** and **0xFC**, each -32767..32767:
  pushing up raises 0xFC, pushing right raises 0xFB.
- With every direction disabled nothing else arrives: no keys, no controller. Windows does
  not treat usage 0001:0000 as a game controller, so games don't see the stick at all.

So Secretarybird reads the stick from report 0x0B in that mode and presses keys at any
angle, with no driver. Reads of the settings in each mode matched Synapse's code:

| Synapse mode | 02:B8 report mode | 02:B9 mapping mode (profile 1) |
|---|---|---|
| True Analog | 00 | 01 00 |
| 4-way | 01 | 01 01 |
| 8-way | 01 | 01 02 |

The report only comes while the keypad is in **driver mode** (`00:04` with `[03, 00]`), which
Synapse sets every time it starts (logged three times). With Synapse closed it stops. Setting
driver mode with Synapse closed (`secbird devmode driver`) brings the report back, and the
keypad's other keys keep typing (tested on the keypad). Unplugging returns it to normal
mode, so the tray sets driver mode with each profile that has the stick on.

The keypad does not keep 8-way mode by itself: after unplugging with Synapse closed the stick
was a controller again. Sending 02:38 `[01]` and 02:39 `[01 02]` with Synapse closed (tested
on the keypad) gives 8-way mode with no keys and no controller from the stick, only its
position in report 0x0B. So with the stick as keys the tray sends driver mode and 8-way mode
with the profile; a profile without it puts the stick back to analog, if another profile of
the keypad uses it.

## Tested

On one keypad (firmware 1.2), Synapse closed, after unplugging and replugging: with the stick
as keys on, Secretarybird reads it straight from the keypad, keys type, and Windows' controller
test shows no movement; turned off, the stick is a controller again and types nothing.

## Remapping keys

From a USB capture (USBPcap, 2026-11-02) of Synapse 4 setting key 15 (S) to type 2 and back.
Before each key write Synapse sends **Set OBM Keymap** (02:37, size 1) `[01]`, choosing on-board
keymap 1, then **Set Single Key Assignment** (02:0D, size **0A**):

```
02:37 size 01   01
02:0D size 0A   01 20 00 02 02 00 1F 00 00 00    profile 1, key 32 (S), normal layer,
                                                  function 2 (key), 2 bytes: no modifier, usage 1F ("2")
```

Back to default was the same 02:0D with usage 16 (S). Both times Synapse also rewrote every other
remapped key the same way (key ids 01, 31, 3B, D9, DA: grave, V, the HyperShift thumb key and
the two other thumb keys), and sent the changed key's actuation (02:19) and rapid trigger
(02:1A), which Secretarybird leaves alone. The keypad keeps the keymap on board, so remaps work
without any software running.

Writing 02:0D to the live slot 0 with size 50, as Secretarybird does for keyboards, is accepted
and reads back changed, but the keypad keeps typing the original key. So for this keypad
(`"onboard_keymap": true`) Secretarybird sends exactly the two commands above, only for a single
key without modifiers on the normal layer, and "default" writes the key's own usage, as Synapse
does.

### In driver mode

The keypad uses its on-board keymap only in normal mode: in driver mode (which the stick as keys
needs) it types its factory keys (tested: key 6 set to G typed 5 in driver mode, G after
`devmode normal`). In driver mode it does list every key it has down in report 0x0B, next to the
stick, as `[key id, value hi, value lo]` with the value 0-65535 for how far the key is pushed
(`secbird listen keys`, 2026-11-02: key 6 -> id 06, keys 1-3 -> 01-03, S -> 20; a thumb button
sent report 04 `[01]` instead). So with the stick as keys the tray remaps in software: a
low-level keyboard hook swaps a keystroke for its remap when the keypad has that key down right
now (or had, within 60 ms), and lets the same key from any other keyboard through. Nothing more
is sent to the keypad for this.

Turning the stick as keys off puts the stick back to analog and the keypad back to normal mode
(00:04 `[00 00]`, what Synapse sends when it closes), so the on-board remaps apply again; a
profile without the stick does the same when another profile of the keypad uses it.

### Thumb buttons and the stick press

From Synapse's product files and a USB capture (2026-11-02) of Synapse with the keypad in driver
mode: each thumb input pressed and let go, the stick press set to 2, then all four set back to
their defaults.

| Input | Default | Key id (02:0D) | Driver mode, report 04 |
|---|---|---|---|
| Top thumb button | HyperShift | 59 (3B) | `04 01` |
| Middle thumb button | Cycle down keymap | 218 (DA) | `04 67` |
| Bottom thumb button | Cycle up profile | 217 (D9) | `04 68` |
| Stick press | Disabled | 219 (DB) | `04 69` |

`04 00` when let go. Set to a key, Synapse writes them like any key (`01 DB 00 02 02 00 1F`:
the stick press to 2). Back to default it wrote `01 3B 00 0C 01 01` (HyperShift) and
`01 D9 00 07 01 04` (cycle up profile); for the middle button and the stick press it wrote
nothing, so Secretarybird can set those to a key but not put them back. In driver mode these
inputs type nothing; with the stick as keys the tray presses a remapped one's key itself.

In driver mode the top thumb button always reports itself as HyperShift (report 04 `[01]`), and
the keypad flashes its lights as its own HyperShift cue when it is pressed, whatever the button
is set to: seen with Synapse running and the button set to M in Synapse (USB capture: Synapse sent
nothing at the press, only the keypad's `04 01`), and with the stick as keys in Secretarybird. In
normal mode, set to M, it types m and doesn't flash. Nothing found turns the cue off; the editor
says so on that button.

## Thumbstick orientation (analog mode)

Secretarybird sets Synapse's Thumbstick Orientation with the same command: 02:45 `[01 01]` turns
the stick a quarter turn clockwise in the keypad (as logged from Synapse and seen in the USB
captures), `[01 00]` puts it back; 02:C5 `[01]` reads it. It is offered in analog mode only.
The keypad turns the position it reports in key mode (report 0x0B) too (tested on the keypad),
so key mode sets it back to `[01 00]` and keeps its own Forward angle; going back to analog mode
puts the profile's analog orientation back (the one found on the keypad, the first time). Tested on the
keypad: key mode's Forward stays put whichever analog orientation is chosen.
