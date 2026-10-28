//! Command ids with their argument sizes. Each SET command's GET variant is `cmd.get()`.
//!
//! Only commands that agree between Synapse's middleware and public OpenRazer docs live
//! here; `verified` notes the ones confirmed on real hardware.

use crate::proto::Command;

/// Storage byte used as the first argument by many SET commands.
pub const NOSTORE: u8 = 0x00;
pub const VARSTORE: u8 = 0x01;

/// Whole-keyboard backlight LED id.
pub const BACKLIGHT_LED: u8 = 0x05;

/// Firmware version: args `[major, minor]`. Verified (Basilisk V3).
pub const FIRMWARE: Command = Command::new(0x00, 0x01);
pub const FIRMWARE_SIZE: u8 = 0x02;

/// Full firmware version: `[major, minor, internal, reserved]`, the form Razer's firmware
/// manifests use (2.50.3.0). GET only.
pub const FIRMWARE_EXT: Command = Command::new(0x00, 0x07);
pub const FIRMWARE_EXT_SIZE: u8 = 0x04;

/// Serial number: 22 ASCII bytes. Verified (Basilisk V3).
pub const SERIAL: Command = Command::new(0x00, 0x02);
pub const SERIAL_SIZE: u8 = 0x16;

/// Mice paired to a receiver or dock: `[count, (connected, pid_hi, pid_lo) * count]`.
/// Layout from PR #2 (verified on a HyperPolling dongle); not yet seen on a Mouse Dock Pro.
pub const WIRELESS_DEVICES: Command = Command::new(0x00, 0x3F);
pub const WIRELESS_DEVICES_SIZE: u8 = 0x50;

/// On-device macros (class 06, from Synapse's obmEngine). Ids, offsets and sizes are big-endian.
/// Storage: GET only, `[max_id u16, max_memory u32, available u32, recyclable u32]`.
pub const MACRO_STORAGE: Command = Command::new(0x06, 0x0E);
pub const MACRO_STORAGE_SIZE: u8 = 0x0E;
/// Number of macros on the device: GET only, `[count u16]`.
pub const MACRO_COUNT: Command = Command::new(0x06, 0x00);
pub const MACRO_COUNT_SIZE: u8 = 0x02;
/// Macro ids: GET `[offset u16, count u16, ids u16...]`, asked with `2 * n + 4` bytes.
pub const MACRO_IDS: Command = Command::new(0x06, 0x0B);
pub const MACRO_IDS_SIZE: u8 = 0x50;
/// Reserve room for a macro, which also creates it: SET `[id u16, size u32]`.
pub const MACRO_ALLOCATE: Command = Command::new(0x06, 0x08);
pub const MACRO_ALLOCATE_SIZE: u8 = 0x06;
/// Macro events: `[id u16, offset u32, len, data...]`, at most 72 bytes per report.
pub const MACRO_DATA: Command = Command::new(0x06, 0x09);
pub const MACRO_DATA_SIZE: u8 = 0x50;
pub const MACRO_DATA_CHUNK: usize = 72;
/// 250-byte macro header (guid, item count, name): `[id u16, offset u16, 250 u16, data...]`,
/// at most 64 bytes per report.
pub const MACRO_NAME: Command = Command::new(0x06, 0x0C);
pub const MACRO_NAME_SIZE: u8 = 0x50;
pub const MACRO_NAME_CHUNK: usize = 64;
pub const MACRO_HEADER_LEN: usize = 250;
/// Delete a macro: SET `[id u16]`.
pub const MACRO_DELETE: Command = Command::new(0x06, 0x03);
pub const MACRO_DELETE_SIZE: u8 = 0x02;

/// Device mode: `[mode, param]`; 0x00 = normal, 0x03 = driver mode.
pub const DEVICE_MODE: Command = Command::new(0x00, 0x04);
pub const DEVICE_MODE_SIZE: u8 = 0x02;

/// Polling rate (up to 1000 Hz): `[code]`, see [`polling_code`].
pub const POLLING: Command = Command::new(0x00, 0x05);
pub const POLLING_SIZE: u8 = 0x01;

/// Polling rate for 2000-8000 Hz devices: `[0x00, code]`.
pub const POLLING_HS: Command = Command::new(0x00, 0x40);
pub const POLLING_HS_SIZE: u8 = 0x02;

/// Active DPI: `[store, x_hi, x_lo, y_hi, y_lo, 0, 0]`. GET verified (Basilisk V3).
pub const DPI: Command = Command::new(0x04, 0x05);
pub const DPI_SIZE: u8 = 0x07;

/// DPI stages: `[store, active (1-based), count, count x (id, x_hi, x_lo, y_hi, y_lo, 0, 0)]`.
/// SET sends `7 * count + 3` bytes with ids 0-based; GET answers them 1-based (Synapse
/// checks `sent + 1 == read`). GET verified (Basilisk V3 and user reports).
pub const DPI_STAGES: Command = Command::new(0x04, 0x06);
pub const DPI_STAGES_SIZE: u8 = 0x26;
pub const MAX_DPI_STAGES: usize = 5;

/// Seconds idle before a wireless device sleeps: `[hi, lo]`. GET seen in user reports.
pub const IDLE_TIME: Command = Command::new(0x07, 0x03);
pub const IDLE_TIME_SIZE: u8 = 0x02;

/// Auto-sleep switch: `[0, enabled]`. Only some devices have it; on those Synapse allows
/// idle times up to 60 minutes.
pub const AUTO_SLEEP: Command = Command::new(0x07, 0x08);
pub const AUTO_SLEEP_SIZE: u8 = 0x02;
/// Laptops: what the keyboard backlight does when idle (Synapse's "Smart Dimming",
/// `[profile 1, enabled, level]`).
pub const SMART_DIMMING: Command = Command::new(0x07, 0x13);
pub const SMART_DIMMING_SIZE: u8 = 0x03;
/// Keyboards: gaming mode, `[bits, ...]` (Synapse's "Game Mode State"; reply `[bits, key
/// cover, lifted, 0]`). Bits from Synapse's `getGameModeBit`: 1 on, 4 Windows key, 8 Alt+Tab,
/// 16 Alt+F4 blocked.
pub const GAME_MODE: Command = Command::new(0x00, 0x08);
pub const GAME_MODE_SIZE: u8 = 0x04;

/// Battery level: reply `[_, level 0..=255]`.
pub const BATTERY: Command = Command::new(0x07, 0x00);
/// Charging state: reply `[_, 0|1]`.
pub const CHARGING: Command = Command::new(0x07, 0x04);
pub const POWER_SIZE: u8 = 0x02;

/// Extended matrix brightness: `[store, led, 0..=255]`.
pub const MATRIX_BRIGHTNESS: Command = Command::new(0x0F, 0x04);
pub const MATRIX_BRIGHTNESS_SIZE: u8 = 0x03;

/// Extended matrix effect: `[store, led, effect, flags, rate, colors, r, g, b, ...]`.
pub const MATRIX_EFFECT: Command = Command::new(0x0F, 0x02);
pub const EFFECT_OFF: u8 = 0x00;
pub const EFFECT_STATIC: u8 = 0x01;
/// Show the colors last sent with [`MATRIX_FRAME`]. Not yet verified on hardware.
pub const EFFECT_CUSTOM_FRAME: u8 = 0x08;

/// One row of per-key colors: `[profile, region, row, start_col, end_col, r, g, b, ...]`,
/// size `5 + 3 * keys`. Profile 0 is the live (unsaved) frame. Layout from Synapse's
/// `setChromaFrame`; not yet verified on hardware.
pub const MATRIX_FRAME: Command = Command::new(0x0F, 0x03);
/// Colors per frame packet: (80 argument bytes - 5 header bytes) / 3.
pub const MATRIX_FRAME_MAX_KEYS: usize = 25;

// Older ("standard") lighting commands, used by devices from before ~2019.
// Packet layouts as documented by OpenRazer.

/// LED on/off: `[store, led, 0|1]`.
pub const LED_STATE: Command = Command::new(0x03, 0x00);
/// LED color: `[store, led, r, g, b]`.
pub const LED_RGB: Command = Command::new(0x03, 0x01);
/// LED effect: `[store, led, effect]`, effect 0 = static.
pub const LED_EFFECT: Command = Command::new(0x03, 0x02);
/// LED brightness: `[store, led, 0..=255]`.
pub const LED_BRIGHTNESS: Command = Command::new(0x03, 0x03);
pub const CLASSIC_STATIC: u8 = 0x00;
/// Whole-grid effect: `[effect, ...]`: off `[0x00]`, static `[0x06, r, g, b]`,
/// custom frame `[0x05, store]`.
pub const GRID_EFFECT: Command = Command::new(0x03, 0x0A);
pub const GRID_OFF: u8 = 0x00;
pub const GRID_CUSTOM_FRAME: u8 = 0x05;
pub const GRID_STATIC: u8 = 0x06;
/// One row of a custom frame: `[0xFF, row, start_col, end_col, r, g, b, ...]`, size 0x46.
pub const GRID_FRAME: Command = Command::new(0x03, 0x0B);
pub const GRID_FRAME_SIZE: u8 = 0x46;
/// Per-LED effect on a few Naga models: off `[store, led, 0x00]`, static `[store, led, 0x06, r, g, b]`.
pub const MOUSE_LED_EFFECT: Command = Command::new(0x03, 0x0D);

/// What one button does: `[slot, button, layer, function, size, data0..data4]`, size 0x50.
/// Layout from Synapse's `setSingleButtonAssignment`; GET and SET verified (Basilisk V3).
/// Replies always carry 5 data bytes but report size 1, so readers take all 5.
pub const BUTTON: Command = Command::new(0x02, 0x0C);
pub const BUTTON_SIZE: u8 = 0x50;
/// The button ids the mouse has (Synapse "Get Button ID list"): `[count, id...]`.
pub const BUTTON_IDS: Command = Command::new(0x02, 0x04);
pub const BUTTON_IDS_SIZE: u8 = 0x50;
/// What one keyboard key does: `[slot, key, layer, function, size, data0..data4]`, size 0x50,
/// the same layout as [`BUTTON`] (Synapse's `setSingleKeyAssignment`). Key ids are the
/// position numbers in [`crate::buttons::KEYS`]. Synapse 4 defines it but remaps keyboards
/// in software, so no keyboard has been seen to take it yet.
pub const KEY: Command = Command::new(0x02, 0x0D);
pub const KEY_SIZE: u8 = 0x50;
/// The key ids a keyboard has (Synapse "Get Key Assignment List"): request `[slot, layer,
/// offset]`, reply `[slot, layer, ids...]`. GET only; what the request bytes mean is a guess.
pub const KEY_IDS: Command = Command::new(0x02, 0x8F);
/// Sensor rotation in firmware (Synapse "Proximity Sensor Angle Tune"): `[store, enabled, angle]`.
/// Only newer mice have it; Synapse rotates in software on the rest.
pub const SENSOR_ANGLE: Command = Command::new(0x0B, 0x14);
pub const SENSOR_ANGLE_SIZE: u8 = 0x03;
/// Scroll wheel (Synapse's `rzDevice25`). Each is `[store, value]`, size 2; store 0 is
/// live, 1-5 the saved slots. GETs seen on a Naga V3 Pro on a Mouse Dock Pro, matching
/// what Synapse showed.
///
/// Wheel mode: 0 tactile, 1 free-spin, 2 precision tactile (Synapse's "Micro Tactile").
pub const SCROLL_MODE: Command = Command::new(0x02, 0x30);
/// The older wheel mode command, tactile and free-spin only (OpenRazer's scroll mode).
pub const SCROLL_MODE_OLD: Command = Command::new(0x02, 0x14);
/// Scroll acceleration in free-spin: on/off, and its level 0-4 (Synapse's
/// slider; 2 is medium). The Naga V3 Pro refuses 5 and up.
pub const SCROLL_ACCELERATION: Command = Command::new(0x02, 0x16);
pub const SCROLL_ACCELERATION_LEVEL: Command = Command::new(0x02, 0x22);
/// Smart-Reel (Synapse's "Scroll Mode Auto Switch"): on/off, and its speed 0-4,
/// as for acceleration.
pub const SMART_REEL: Command = Command::new(0x02, 0x17);
pub const SMART_REEL_LEVEL: Command = Command::new(0x02, 0x23);
/// The wheel modes the wheel-mode button cycles through, a bit per mode (`1 << mode`):
/// Synapse's "Scroll Wheel Mode Selected Information". All three on reads 7.
pub const SCROLL_MODES_ENABLED: Command = Command::new(0x15, 0x0B);
pub const SCROLL_SIZE: u8 = 0x02;
/// Button slot 0 is live and unsaved, and wins over the saved profile slots 1-5.
/// Verified (Basilisk V3): a remap in slot 0 took effect while slot 1 was active.
pub const BUTTON_LIVE_SLOT: u8 = 0x00;
/// Active saved on-board profile slot: `[slot]`. GET verified (Basilisk V3: 1 of 5).
pub const ACTIVE_SLOT: Command = Command::new(0x05, 0x04);

/// Polling rate code (sent to the device) for a rate in Hz.
pub fn polling_code(hz: u32) -> Option<u8> {
    match hz {
        1000 => Some(0x01),
        500 => Some(0x02),
        125 => Some(0x08),
        _ => None,
    }
}

pub fn polling_hz(code: u8) -> Option<u32> {
    match code {
        0x01 => Some(1000),
        0x02 => Some(500),
        0x08 => Some(125),
        _ => None,
    }
}

/// High-speed polling codes (8 kHz-capable devices).
pub fn polling_hs_code(hz: u32) -> Option<u8> {
    match hz {
        8000 => Some(0x01),
        4000 => Some(0x02),
        2000 => Some(0x04),
        1000 => Some(0x08),
        500 => Some(0x10),
        250 => Some(0x20),
        125 => Some(0x40),
        _ => None,
    }
}

pub fn polling_hs_hz(code: u8) -> Option<u32> {
    match code {
        0x01 => Some(8000),
        0x02 => Some(4000),
        0x04 => Some(2000),
        0x08 => Some(1000),
        0x10 => Some(500),
        0x20 => Some(250),
        0x40 => Some(125),
        _ => None,
    }
}
