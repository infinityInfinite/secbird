//! What the Audio and Screen effects follow: the sound level of what the PC plays (the
//! default output's peak meter, which Windows keeps anyway, so nothing is recorded) and the
//! screen scaled down to a few pixels. Each runs on its own thread, only while a scene uses it.

use crate::studio::Senses;
use secbird_core::Rgb;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows_sys::core::GUID;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

struct Worker {
    stop: Arc<AtomicBool>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct Sensors {
    audio: Option<Worker>,
    screen: Option<Worker>,
}

impl Sensors {
    /// Start or stop each sensor as the running scenes need it.
    pub fn want(&mut self, audio: bool, screen: bool, senses: &Arc<Mutex<Senses>>) {
        if audio != self.audio.is_some() {
            self.audio = audio.then(|| spawn(senses.clone(), audio_loop));
            if !audio {
                lock(senses).audio = (0.0, 0.0, 0.0);
            }
        }
        if screen != self.screen.is_some() {
            self.screen = screen.then(|| spawn(senses.clone(), screen_loop));
            if !screen {
                lock(senses).screen = None;
            }
        }
    }
}

fn lock(s: &Mutex<Senses>) -> std::sync::MutexGuard<'_, Senses> {
    s.lock().unwrap_or_else(|p| p.into_inner())
}

fn spawn(senses: Arc<Mutex<Senses>>, body: fn(&AtomicBool, &Mutex<Senses>)) -> Worker {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || body(&flag, &senses));
    Worker { stop }
}

// --- Sound: IAudioMeterInformation on the default output, through raw COM vtables ---

const CLSID_MM_DEVICE_ENUMERATOR: GUID = GUID::from_u128(0xBCDE0395_E52F_467C_8E3D_C4579291692E);
const IID_IMM_DEVICE_ENUMERATOR: GUID = GUID::from_u128(0xA95664D2_9614_4F35_A746_DE8DB63617E6);
const IID_IAUDIO_METER_INFORMATION: GUID = GUID::from_u128(0xC02216F6_8C67_4B5B_9D00_D008E73E0064);
const E_RENDER: i32 = 0;
const E_CONSOLE: i32 = 0;

/// The function at `index` in a COM object's vtable.
unsafe fn method(obj: *mut c_void, index: usize) -> *const c_void {
    let vtbl = *(obj as *const *const *const c_void);
    *vtbl.add(index)
}

unsafe fn release(obj: *mut c_void) {
    if !obj.is_null() {
        let f: unsafe extern "system" fn(*mut c_void) -> u32 = std::mem::transmute(method(obj, 2));
        f(obj);
    }
}

/// The default output's peak meter, or null when there is no output.
unsafe fn open_meter() -> *mut c_void {
    let mut enumerator: *mut c_void = null_mut();
    if CoCreateInstance(&CLSID_MM_DEVICE_ENUMERATOR, null_mut(), CLSCTX_ALL, &IID_IMM_DEVICE_ENUMERATOR, &mut enumerator) < 0 {
        return null_mut();
    }
    let get_default: unsafe extern "system" fn(*mut c_void, i32, i32, *mut *mut c_void) -> i32 =
        std::mem::transmute(method(enumerator, 4));
    let mut device: *mut c_void = null_mut();
    let hr = get_default(enumerator, E_RENDER, E_CONSOLE, &mut device);
    release(enumerator);
    if hr < 0 || device.is_null() {
        return null_mut();
    }
    let activate: unsafe extern "system" fn(*mut c_void, *const GUID, u32, *const c_void, *mut *mut c_void) -> i32 =
        std::mem::transmute(method(device, 3));
    let mut meter: *mut c_void = null_mut();
    let hr = activate(device, &IID_IAUDIO_METER_INFORMATION, CLSCTX_ALL, null(), &mut meter);
    release(device);
    if hr < 0 { null_mut() } else { meter }
}

fn audio_loop(stop: &AtomicBool, senses: &Mutex<Senses>) {
    unsafe {
        CoInitializeEx(null(), COINIT_MULTITHREADED as u32);
        let mut meter = null_mut();
        let mut opened = Instant::now() - Duration::from_secs(10);
        let (mut level, mut left, mut right) = (0f32, 0f32, 0f32);
        while !stop.load(Ordering::Relaxed) {
            // The default output can change (headphones plugged in): look again now and then.
            if opened.elapsed() > Duration::from_secs(3) {
                release(meter);
                meter = open_meter();
                opened = Instant::now();
            }
            let (mut peak, mut ch) = (0f32, [0f32; 2]);
            if !meter.is_null() {
                let get_peak: unsafe extern "system" fn(*mut c_void, *mut f32) -> i32 = std::mem::transmute(method(meter, 3));
                let get_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32 = std::mem::transmute(method(meter, 4));
                let get_channels: unsafe extern "system" fn(*mut c_void, u32, *mut f32) -> i32 =
                    std::mem::transmute(method(meter, 5));
                if get_peak(meter, &mut peak) < 0 {
                    opened = Instant::now() - Duration::from_secs(10);
                }
                let mut n = 0u32;
                if get_count(meter, &mut n) >= 0 && n >= 2 {
                    let mut all = vec![0f32; n as usize];
                    if get_channels(meter, n, all.as_mut_ptr()) >= 0 {
                        ch = [all[0], all[1]];
                    }
                } else {
                    ch = [peak, peak];
                }
            }
            // Fast up, slower down, and a curve so quiet music still moves the lights.
            let shape = |v: f32| v.clamp(0.0, 1.0).sqrt();
            let follow = |old: f32, new: f32| if new > old { new } else { old * 0.88 + new * 0.12 };
            level = follow(level, shape(peak));
            left = follow(left, shape(ch[0]));
            right = follow(right, shape(ch[1]));
            lock(senses).audio = (level, left, right);
            std::thread::sleep(Duration::from_millis(16));
        }
        release(meter);
        CoUninitialize();
    }
}

// --- Screen: the desktop scaled down with GDI ---

const SCREEN_W: i32 = 32;
const SCREEN_H: i32 = 18;

fn screen_loop(stop: &AtomicBool, senses: &Mutex<Senses>) {
    unsafe {
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, SCREEN_W, SCREEN_H);
        let old = SelectObject(mem, bmp);
        SetStretchBltMode(mem, HALFTONE);
        SetBrushOrgEx(mem, 0, 0, null_mut());
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = SCREEN_W;
        info.bmiHeader.biHeight = -SCREEN_H; // top row first
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut buf = vec![0u8; (SCREEN_W * SCREEN_H * 4) as usize];
        while !stop.load(Ordering::Relaxed) {
            let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            let ok = StretchBlt(mem, 0, 0, SCREEN_W, SCREEN_H, screen, 0, 0, w, h, SRCCOPY) != 0
                && GetDIBits(mem, bmp, 0, SCREEN_H as u32, buf.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS) > 0;
            if ok {
                let px: Vec<Rgb> = buf.chunks(4).map(|b| Rgb(b[2], b[1], b[0])).collect();
                lock(senses).screen = Some((SCREEN_W as usize, SCREEN_H as usize, px));
            }
            std::thread::sleep(Duration::from_millis(66));
        }
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs both sensors on this PC for two seconds: `cargo test -p secbird-tray sensors_run
    /// -- --ignored --nocapture`. Play some sound to see the level move.
    #[test]
    #[ignore]
    fn sensors_run() {
        let senses = Arc::new(Mutex::new(Senses::default()));
        let mut s = Sensors::default();
        s.want(true, true, &senses);
        // SENSORS_SECS=20 to measure what they cost from outside.
        let secs: u64 = std::env::var("SENSORS_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(2);
        for _ in 0..secs * 4 {
            std::thread::sleep(Duration::from_millis(250));
            let g = lock(&senses);
            let screen = g.screen.as_ref().map(|(w, h, px)| (*w, *h, px[px.len() / 2]));
            if secs <= 2 {
                println!("audio {:?} screen {:?}", g.audio, screen);
            }
        }
        assert!(lock(&senses).screen.is_some(), "screen capture works");
        s.want(false, false, &senses);
        std::thread::sleep(Duration::from_millis(200));
        assert!(lock(&senses).screen.is_none());
    }
}
