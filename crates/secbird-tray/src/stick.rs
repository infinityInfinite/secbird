//! The thumbstick as keys (see [`secbird_core::stick`]): a thread reads the stick and presses
//! keys with `SendInput`, as a keyboard would (scan codes, which games read). The stick is
//! read through the Windows joystick API (WinMM), which names each controller's USB ids, and
//! through XInput. Neither has a change notification, so the thread polls, but only while a
//! profile has the stick on (or the editor shows it live) and the PC isn't locked.
//! Otherwise it waits on a condition variable and costs nothing.
//!
//! Which stick: the one picked in the editor, or by default the Razer stick that moved last.
//! Windows can list the keypad more than once, and only one stick of one listing moves; the
//! others sit still, some at the end of their travel (an axis a device doesn't really have
//! reads 0), so "pushed furthest" can't tell them apart.
//!
//! On the Tartarus V2 Pro the stick usually comes from the keypad's own report (8-way and
//! driver mode, which the profile sets; the tray's event listener hands it over), so games
//! see no controller. A stick read as a controller is the fallback.

use secbird_core::db::RAZER_VID;
use secbird_core::stick::{Presser, Reading, Stick};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use windows_sys::Win32::Media::Multimedia::{joyGetDevCapsW, joyGetNumDevs, joyGetPosEx, JOYCAPSW, JOYINFOEX};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};

/// About 250 readings a second while active: Rust's sleep uses a high-resolution timer.
const POLL: Duration = Duration::from_millis(4);
/// How often to look again for controllers when the one needed isn't there.
const LOOK_AGAIN: Duration = Duration::from_secs(2);
/// The editor's live view keeps the stick read this long after each request.
const WATCH_FOR: Duration = Duration::from_secs(2);
/// `JOY_RETURNALL`: every axis, the buttons and the hat.
const JOY_RETURN_ALL: u32 = 0xFF;
const STICKS: [&str; 2] = ["left", "right"];

#[derive(Default)]
struct State {
    /// The settings in force (the keypad's current profile); `None` leaves the stick alone.
    stick: Option<Stick>,
    /// Product ids of connected keypads with a stick, to tell their controller from others.
    pids: Vec<u16>,
    /// The PC is locked or its screen is off: hold no keys.
    paused: bool,
    watch_until: Option<Instant>,
    /// The stick's position from the keypad's own analog report (4-way and 8-way mode, see
    /// [`secbird_core::stick::raw_report`]), once one has arrived.
    raw: Option<(f32, f32)>,
    live: Live,
}

const RAW_LABEL: &str = "Tartarus V2 Pro, straight from the keypad (4-way and 8-way mode)";

/// A stick the thread can read: one of a controller's two, or the keypad's own report.
#[derive(Clone, Copy, PartialEq)]
enum Src {
    Pad(usize, usize),
    Raw,
}

/// What the stick does right now, for the editor.
#[derive(Default, Clone, serde::Serialize)]
pub struct Live {
    /// The stick to read was found.
    pub found: bool,
    /// Keys are pressed from it (a profile has the stick on and the PC isn't locked).
    pub active: bool,
    pub reading: Option<Reading>,
    /// The stick read, e.g. "Joystick 1: Tartarus V2 Pro (1532:02EB), right stick".
    pub source: Option<String>,
}

#[derive(Default)]
pub struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The settings to use and the keypads they belong to; wakes the thread.
    pub fn set(&self, stick: Option<Stick>, pids: Vec<u16>) {
        let mut s = self.lock();
        if s.stick != stick || s.pids != pids {
            s.stick = stick;
            s.pids = pids;
            self.wake.notify_all();
        }
    }

    /// A position from the keypad's own analog report (the tray's event listener reads it).
    pub fn set_raw(&self, pos: (f32, f32)) {
        self.lock().raw = Some(pos);
    }

    pub fn set_paused(&self, paused: bool) {
        let mut s = self.lock();
        if s.paused != paused {
            s.paused = paused;
            self.wake.notify_all();
        }
    }

    /// The latest reading, and keep reading for a moment even when the stick is off, so the
    /// editor can show where it points while settings are chosen.
    pub fn watch(&self) -> Live {
        let mut s = self.lock();
        s.watch_until = Some(Instant::now() + WATCH_FOR);
        self.wake.notify_all();
        s.live.clone()
    }
}

pub fn start(shared: Arc<Shared>) {
    let _ = std::thread::Builder::new().name("stick".into()).stack_size(128 * 1024).spawn(move || run(&shared));
}

fn run(shared: &Shared) {
    let mut presser = Presser::default();
    let mut pads: Vec<Pad> = Vec::new();
    let mut tracks: Vec<[Track; 2]> = Vec::new();
    let mut raw_track = Track::default();
    let mut looked: Option<Instant> = None;
    loop {
        // Sleep until there is something to do; let go of any keys first.
        let (stick, pids, raw) = {
            let mut s = shared.lock();
            loop {
                let watching = s.watch_until.is_some_and(|t| t > Instant::now());
                let active = s.stick.is_some() && !s.paused && !s.pids.is_empty();
                if active || watching {
                    break (s.stick.clone().filter(|_| active), s.pids.clone(), s.raw);
                }
                if presser.holding() {
                    presser.release(&mut press);
                }
                s.live.reading = None;
                s.live.active = false;
                s = shared.wake.wait(s).unwrap_or_else(|p| p.into_inner());
            }
        };

        // Which sticks may be the keypad's: every Razer stick (only the keypad's product ids,
        // when one of those is listed).
        let wanted = |pads: &[Pad]| -> Vec<(usize, usize)> {
            let all = (0..pads.len()).flat_map(|p| [(p, 0), (p, 1)]);
            let keypad = pads.iter().any(|p| p.vid == RAZER_VID && pids.contains(&p.pid));
            all.filter(|&(p, _)| pads[p].vid == RAZER_VID && (!keypad || pids.contains(&pads[p].pid))).collect()
        };
        // List again when the stick isn't there, and now and then while the editor shows the
        // list (a controller plugged in since), but not while keys are pressed from it.
        let missing = looked.is_none() || (wanted(&pads).is_empty() && raw.is_none());
        let stale = stick.is_none() && looked.is_some_and(|t| t.elapsed() >= LOOK_AGAIN * 3);
        if (missing && looked.is_none_or(|t| t.elapsed() >= LOOK_AGAIN)) || stale {
            pads = list_pads();
            tracks = vec![[Track::default(); 2]; pads.len()];
            looked = Some(Instant::now());
        }

        let now = Instant::now();
        let mut ok = Vec::with_capacity(pads.len());
        for (pad, track) in pads.iter().zip(tracks.iter_mut()) {
            let sticks = pad.read();
            if let Some(sticks) = sticks {
                track.iter_mut().zip(sticks).for_each(|(t, pos)| t.see(pos, now));
            }
            ok.push(sticks.is_some());
        }
        // Unplugged, or another mode: drop it; it is listed again when it is back.
        let mut keep = ok.iter().copied();
        pads.retain(|_| keep.next().unwrap_or(false));
        let mut keep = ok.iter().copied();
        tracks.retain(|_| keep.next().unwrap_or(false));

        if let Some(pos) = raw {
            raw_track.see(pos, now);
        }
        let use_raw = raw.is_some();

        // The stick that moved last; before any has moved, the one nearest its centre.
        let best = wanted(&pads)
            .into_iter()
            .map(|(p, s)| (Src::Pad(p, s), tracks[p][s]))
            .chain(use_raw.then_some((Src::Raw, raw_track)))
            .filter_map(|(src, t)| Some((src, t.at?, t.moved)))
            .max_by(|a, b| match (a.2, b.2) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (None, None) => {
                    let push = |(x, y): (f32, f32)| x.hypot(y);
                    push(b.1).total_cmp(&push(a.1))
                }
            });
        let reading = match (best.map(|(_, pos, _)| pos), &stick) {
            (Some((x, y)), Some(stick)) => Some(presser.update(stick, x, y, &mut press)),
            (Some((x, y)), None) => {
                presser.release(&mut press);
                Some(Reading { held: [false; 4], ..Stick::default().read(x, y, [false; 4]) })
            }
            (None, _) => {
                presser.release(&mut press);
                None
            }
        };
        shared.lock().live = Live {
            found: best.is_some(),
            active: stick.is_some(),
            reading,
            source: best.map(|(src, _, _)| match src {
                Src::Pad(p, s) => pads[p].label(s),
                Src::Raw => RAW_LABEL.to_string(),
            }),
        };
        std::thread::sleep(if best.is_some() { POLL } else { Duration::from_millis(250) });
    }
}

/// One stick of one controller: where it is and when it last moved.
#[derive(Clone, Copy, Default)]
struct Track {
    at: Option<(f32, f32)>,
    /// Where it was when it last moved, to measure the next move from.
    anchor: Option<(f32, f32)>,
    moved: Option<Instant>,
}

impl Track {
    /// Noise below this doesn't count as moving.
    const MOVE: f32 = 0.05;

    fn see(&mut self, pos: (f32, f32), now: Instant) {
        let (ax, ay) = *self.anchor.get_or_insert(pos);
        if (pos.0 - ax).hypot(pos.1 - ay) > Self::MOVE {
            self.moved = Some(now);
            self.anchor = Some(pos);
        }
        self.at = Some(pos);
    }
}

/// Press or release one key by scan code; 0xE0 in the high byte marks an extended key.
pub(crate) fn press(code: u16, down: bool) {
    let mut flags = KEYEVENTF_SCANCODE;
    if code & 0xFF00 == 0xE000 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: 0, wScan: code & 0xFF, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
}

/// A game controller Windows lists.
struct Pad {
    api: Api,
    vid: u16,
    pid: u16,
    name: String,
}

enum Api {
    /// A WinMM joystick id, its axis ranges (X, Y, R, U as min, max) and whether it has the
    /// R and U axes (a second stick) at all.
    Joystick { id: u32, ranges: [(u32, u32); 4], has_ru: bool },
    /// An XInput slot.
    XInput { slot: u32 },
}

impl Pad {
    fn label(&self, stick: usize) -> String {
        format!("{} ({:04X}:{:04X}), {} stick", self.name, self.vid, self.pid, STICKS[stick])
    }

    /// Left and right stick positions, -1 to 1 with right and up positive. The Tartarus has
    /// one stick, which Synapse can assign to either.
    fn read(&self) -> Option<[(f32, f32); 2]> {
        Some(match &self.api {
            Api::Joystick { id, ranges, has_ru } => {
                let info = joystick_position(*id)?;
                let norm = |v: u32, (min, max): (u32, u32)| match max > min {
                    true => (v.saturating_sub(min) as f32 / (max - min) as f32 * 2.0 - 1.0).clamp(-1.0, 1.0),
                    false => 0.0,
                };
                // Joystick Y and R grow downwards. An axis the joystick doesn't have reads 0,
                // which would look like a full push.
                let left = (norm(info.dwXpos, ranges[0]), -norm(info.dwYpos, ranges[1]));
                let right = match has_ru {
                    true => (norm(info.dwUpos, ranges[3]), -norm(info.dwRpos, ranges[2])),
                    false => (0.0, 0.0),
                };
                [left, right]
            }
            Api::XInput { slot } => {
                let g = xinput_state(*slot)?.Gamepad;
                let norm = |v: i16| (f32::from(v) / 32767.0).clamp(-1.0, 1.0);
                [(norm(g.sThumbLX), norm(g.sThumbLY)), (norm(g.sThumbRX), norm(g.sThumbRY))]
            }
        })
    }
}

fn joystick_position(id: u32) -> Option<JOYINFOEX> {
    let mut info: JOYINFOEX = unsafe { std::mem::zeroed() };
    info.dwSize = std::mem::size_of::<JOYINFOEX>() as u32;
    info.dwFlags = JOY_RETURN_ALL;
    (unsafe { joyGetPosEx(id, &mut info) } == 0).then_some(info)
}

fn xinput_state(slot: u32) -> Option<XINPUT_STATE> {
    let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };
    (unsafe { XInputGetState(slot, &mut state) } == 0).then_some(state)
}

/// Every connected game controller, as WinMM joysticks and as XInput pads.
fn list_pads() -> Vec<Pad> {
    let mut pads: Vec<Pad> = Vec::new();
    let mut add = |api: Api, vid: u16, pid: u16, name: String| pads.push(Pad { api, vid, pid, name });
    for id in 0..unsafe { joyGetNumDevs() } {
        let mut caps: JOYCAPSW = unsafe { std::mem::zeroed() };
        if unsafe { joyGetDevCapsW(id as usize, &mut caps, std::mem::size_of::<JOYCAPSW>() as u32) } != 0 {
            continue;
        }
        if joystick_position(id).is_none() {
            continue; // listed but not connected
        }
        let name = String::from_utf16_lossy(&{ caps.szPname }).trim_end_matches('\0').trim().to_string();
        let ranges = [(caps.wXmin, caps.wXmax), (caps.wYmin, caps.wYmax), (caps.wRmin, caps.wRmax), (caps.wUmin, caps.wUmax)];
        // JOYCAPS_HASR and JOYCAPS_HASU.
        let has_ru = caps.wCaps & 0x6 == 0x6;
        let label = if name.is_empty() { format!("Joystick {}", id + 1) } else { format!("Joystick {}: {name}", id + 1) };
        add(Api::Joystick { id, ranges, has_ru }, caps.wMid, caps.wPid, label);
    }
    for slot in 0..4 {
        if xinput_state(slot).is_none() {
            continue;
        }
        let (vid, pid) = capabilities_ex()
            .and_then(|get| {
                let mut caps: CapabilitiesEx = unsafe { std::mem::zeroed() };
                (unsafe { get(1, slot, 0, &mut caps) } == 0).then_some((caps.vendor, caps.product))
            })
            .unwrap_or((0, 0));
        add(Api::XInput { slot }, vid, pid, format!("Xbox controller {}", slot + 1));
    }
    pads
}

/// `XINPUT_CAPABILITIES` followed by the USB ids, as `XInputGetCapabilitiesEx` fills it.
#[repr(C)]
struct CapabilitiesEx {
    capabilities: [u8; 20],
    vendor: u16,
    product: u16,
    version: u16,
    reserved: [u16; 3],
}

type GetCapabilitiesEx = unsafe extern "system" fn(u32, u32, u32, *mut CapabilitiesEx) -> u32;

/// `XInputGetCapabilitiesEx`: exported by xinput1_4.dll (Windows 8 and later) only by
/// ordinal 108, the way SDL and browsers find a controller's USB ids.
fn capabilities_ex() -> Option<GetCapabilitiesEx> {
    static FOUND: std::sync::OnceLock<Option<GetCapabilitiesEx>> = std::sync::OnceLock::new();
    *FOUND.get_or_init(load_capabilities_ex)
}

fn load_capabilities_ex() -> Option<GetCapabilitiesEx> {
    let name: Vec<u16> = "xinput1_4.dll".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let module = LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if module.is_null() {
            return None;
        }
        let f = GetProcAddress(module, 108usize as *const u8)?;
        Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, GetCapabilitiesEx>(f))
    }
}
