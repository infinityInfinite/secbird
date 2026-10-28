use crate::engine::{ApplyReport, Battery, Engine};
use secbird_core::log;
use std::collections::{HashMap, HashSet};
use crate::settings::{autostart_enabled, open_config_folder, set_autostart, TraySettings};
use crate::{apps, osd, server};
use std::ffi::CString;
use std::sync::mpsc::{channel, Receiver, Sender};
use secbird_core::update::Release;
use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{RegisterPowerSettingNotification, POWERBROADCAST_SETTING};
use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows_sys::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const APP_NAME: &str = "Secretarybird";
const WM_TRAY: u32 = WM_APP + 1;
/// Sent by a second launch: open the editor of the instance already running.
const WM_OPEN_EDITOR: u32 = WM_APP + 2;
/// Sent by a listener thread: device events are waiting in `App::events`.
const WM_DEVICE_EVENT: u32 = WM_APP + 3;
/// Sent by the firmware check thread: results are waiting in `App::firmware`.
const WM_FIRMWARE: u32 = WM_APP + 4;
/// Sent by the report thread: `wparam` is 1 when the report is on the clipboard.
const WM_REPORT_READY: u32 = WM_APP + 5;
/// Sent by the new-version check thread: the result is waiting in `App::updates`.
const WM_UPDATE: u32 = WM_APP + 6;
/// Sent by the update download thread: the result is waiting in `App::installs`.
const WM_UPDATE_INSTALL: u32 = WM_APP + 7;
/// Sent by the editor's "Update now": the release is waiting in `REQUESTED_UPDATE`.
const WM_START_UPDATE: u32 = WM_APP + 8;
const CLASS_NAME: &str = "SecretarybirdTray";
const TIMER_RESCAN: usize = 1;
/// Extra rescans after start and wake: devices often answer late at boot.
const TIMER_RETRY: usize = 2;
const RETRY_DELAYS_MS: [u32; 3] = [4_000, 15_000, 45_000];
/// After those, a wireless device that sleeps until its first keypress gets looked for
/// once a minute for half an hour, so it still gets its profile (#37).
const SLEEPER_RETRY_MS: u32 = 60_000;
const SLEEPER_RETRIES: usize = 30;
/// Battery check for wireless devices (cheap: two short USB requests per device).
const TIMER_BATTERY: usize = 3;
const BATTERY_MS: u32 = 5 * 60 * 1000;
/// Devices need a moment after arrival or resume before they answer reliably.
const SETTLE_MS: u32 = 1500;
/// Wait for the window in front to settle, so Alt+Tab through several windows sends
/// one profile instead of one per window passed.
const TIMER_FOREGROUND: usize = 4;
const FOREGROUND_MS: u32 = 300;
/// Weekly firmware check (opt-in) and daily new-version check (on unless turned off):
/// looked at every few hours, each runs when its time has passed.
const TIMER_FIRMWARE: usize = 5;
const FIRMWARE_TIMER_MS: u32 = 6 * 60 * 60 * 1000;
const FIRMWARE_FIRST_MS: u32 = 2 * 60 * 1000;
const FIRMWARE_EVERY_SECS: u64 = 7 * 24 * 60 * 60;
const UPDATE_EVERY_SECS: u64 = 24 * 60 * 60;
/// Wireless mice forget their button remaps when they sleep (#45): one button is read
/// back this often and the remaps re-sent if it changed, at most once a minute each.
const TIMER_BUTTONS: usize = 6;
const BUTTONS_MS: u32 = 20_000;
const BUTTONS_AGAIN_SECS: u64 = 60;
/// Per-key colors of wired keyboards sent again this often: quick adjustment mode drops
/// them without saying (#45), so this is how long they can stay dark after it. Laptops
/// set to stay lit get a read on the same beat (#34).
const TIMER_LIGHTS: usize = 7;
const LIGHTS_MS: u32 = 3_000;

const ID_REAPPLY: u32 = 1;
const ID_FOLDER: u32 = 2;
const ID_AUTOSTART: u32 = 3;
const ID_EXIT: u32 = 4;
const ID_EDITOR: u32 = 5;
const ID_FIRMWARE: u32 = 6;
const ID_REPORT: u32 = 7;
const ID_RESTORE_BUTTONS: u32 = 8;
const ID_CHECK_UPDATE: u32 = 9;
const ID_UPDATE_NOTIFY: u32 = 10;
const ID_GET_UPDATE: u32 = 11;
const ID_PROFILE_BASE: u32 = 1000;

const GUID_DEVINTERFACE_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

/// "TaskbarCreated" is broadcast when Explorer restarts; the icon must be re-added.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// The tray window, for the foreground hook (which gets no user data).
static TRAY_HWND: AtomicPtr<c_void> = AtomicPtr::new(null_mut());

struct App {
    hwnd: HWND,
    icon: HICON,
    /// Shared with the editor's server thread.
    engine: Arc<Mutex<Engine>>,
    /// Started on first use; holds the tokenized URL.
    editor_url: Option<String>,
    /// Menu id offset -> (device key, profile name).
    profile_items: Vec<(String, String)>,
    /// Apply to every device on the next rescan, not just newly arrived ones.
    apply_all_pending: bool,
    /// Index into RETRY_DELAYS_MS of the next retry.
    retry_step: usize,
    /// Warned about Synapse since it last started.
    synapse_warned: bool,
    /// Devices already warned about low battery (cleared once charging or recovered).
    low_battery: HashSet<String>,
    batteries: Vec<Battery>,
    /// When each mouse last got its lost remaps back (device key -> unix seconds).
    buttons_restored: HashMap<String, u64>,
    /// Event collections with a listener thread; a thread removes its path when the
    /// device goes away, so a later rescan starts a new one.
    listening: Arc<Mutex<HashSet<CString>>>,
    events_tx: Sender<DeviceEvent>,
    events: Receiver<DeviceEvent>,
    firmware_tx: Sender<Vec<FirmwareNews>>,
    firmware: Receiver<Vec<FirmwareNews>>,
    updates_tx: Sender<UpdateResult>,
    updates: Receiver<UpdateResult>,
    /// A newer Secretarybird, once a check found one: offered at the top of the menu.
    update: Option<Release>,
    /// What clicking the last notification does.
    balloon_click: RefCell<Option<Click>>,
    /// An update is downloading; a second request waits for it.
    updating: bool,
    installs_tx: Sender<Result<(), String>>,
    installs: Receiver<Result<(), String>>,
    /// The session is locked, or the screen is off: the lights go down if set (#48).
    locked: bool,
    screen_off: bool,
}

/// What clicking a notification does.
enum Click {
    Open(String),
    Update(Release),
}

/// The release the editor asked to update to, for the tray's window to pick up.
static REQUESTED_UPDATE: Mutex<Option<Release>> = Mutex::new(None);

/// The editor's "Update now": hand the release to the tray, which downloads and installs it
/// and shows its progress. `false` when no tray is running (the editor alone, `--serve`).
pub fn request_update(r: Release) -> bool {
    unsafe {
        let tray = FindWindowW(wide(CLASS_NAME).as_ptr(), null());
        if tray.is_null() {
            return false;
        }
        *REQUESTED_UPDATE.lock().unwrap_or_else(|p| p.into_inner()) = Some(r);
        PostMessageW(tray, WM_START_UPDATE, 0, 0) != 0
    }
}

/// A new-version check's answer, and whether it was asked for from the menu (then
/// "you're up to date" and errors are shown too).
struct UpdateResult {
    manual: bool,
    found: Result<Option<Release>, String>,
}


/// A device with newer firmware out, found by the weekly check.
struct FirmwareNews {
    key: String,
    name: String,
    version: String,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Something a device did by itself, reported on its event collection.
enum DeviceEvent {
    Dpi { key: String, x: u16, y: u16 },
    OnboardProfile(u8),
}

/// Listener threads only block on a read, so a small stack is plenty.
const LISTENER_STACK: usize = 64 * 1024;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Run `f` on the app state. Returns `None` while the state is borrowed, which only
/// happens if a message arrives re-entrantly.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

fn same_guid(a: &GUID, b: &GUID) -> bool {
    (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let src: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..src.len()].copy_from_slice(&src);
    dst[src.len()] = 0;
}

pub fn run() {
    unsafe {
        let _mutex = CreateMutexW(null(), 0, wide("Local\\Secretarybird.Tray").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let running = FindWindowW(wide(CLASS_NAME).as_ptr(), null());
            if std::env::var_os("SECBIRD_SIM").is_some() {
                let text = wide(
                    "Secretarybird is already running, so SECBIRD_SIM has no effect.

                     Quit it from the tray menu (Exit), then start it again from this terminal.",
                );
                MessageBoxW(null_mut(), text.as_ptr(), wide(APP_NAME).as_ptr(), MB_ICONINFORMATION);
            }
            if !running.is_null() {
                // Let the running instance bring the browser to the front.
                AllowSetForegroundWindow(ASFW_ANY);
                PostMessageW(running, WM_OPEN_EDITOR, 0, 0);
            }
            return;
        }

        let engine = match Engine::new() {
            Ok(e) => e,
            Err(e) => {
                let text = wide(&format!("Could not start HID access: {e}"));
                MessageBoxW(null_mut(), text.as_ptr(), wide(APP_NAME).as_ptr(), MB_ICONERROR);
                return;
            }
        };

        let hinstance = GetModuleHandleW(null());
        let class = wide(CLASS_NAME);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        RegisterClassW(&wc);
        // A hidden top-level window: message-only windows miss power broadcasts.
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            wide(APP_NAME).as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        );
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);
        register_hid_notifications(hwnd);
        // Locking and the screen turning off, for the lights while away (#48). Windows
        // sends the display state once right away.
        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        RegisterPowerSettingNotification(hwnd, &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE);
        // Event-driven, no polling: Windows calls back on this thread when the window
        // in front changes, and nothing runs in between.
        TRAY_HWND.store(hwnd, Ordering::Relaxed);
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            Some(on_foreground),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );

        let icon = load_icon();
        add_icon(hwnd, icon);
        let (events_tx, events) = channel();
        let (firmware_tx, firmware) = channel();
        let (updates_tx, updates) = channel();
        let (installs_tx, installs) = channel();
        APP.with(|cell| {
            *cell.borrow_mut() = Some(App {
                hwnd,
                icon,
                engine: Arc::new(Mutex::new(engine)),
                editor_url: None,
                profile_items: Vec::new(),
                apply_all_pending: true,
                retry_step: 0,
                synapse_warned: false,
                low_battery: HashSet::new(),
                buttons_restored: HashMap::new(),
                batteries: Vec::new(),
                listening: Arc::new(Mutex::new(HashSet::new())),
                events_tx,
                events,
                firmware_tx,
                firmware,
                updates_tx,
                updates,
                update: None,
                balloon_click: RefCell::new(None),
                updating: false,
                installs_tx,
                installs,
                locked: false,
                screen_off: false,
            })
        });
        log::append("tray started");
        with_app(|app| app.announce_updated());
        with_app(|app| app.rescan());
        start_retries(hwnd);
        SetTimer(hwnd, TIMER_BATTERY, BATTERY_MS, None);
        SetTimer(hwnd, TIMER_BUTTONS, BUTTONS_MS, None);
        SetTimer(hwnd, TIMER_LIGHTS, LIGHTS_MS, None);
        SetTimer(hwnd, TIMER_FIRMWARE, FIRMWARE_FIRST_MS, None);
        if std::env::args().any(|a| a == "--editor") {
            with_app(|app| app.open_editor());
        }

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = (lparam as u32) & 0xFFFF;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_menu(hwnd);
            } else if event == WM_LBUTTONDBLCLK {
                with_app(|app| app.open_editor());
            } else if event == NIN_BALLOONUSERCLICK {
                with_app(|app| match app.balloon_click.take() {
                    Some(Click::Open(url)) => open_url(&url),
                    Some(Click::Update(r)) => app.start_update(r),
                    None => {}
                });
            }
            0
        }
        WM_OPEN_EDITOR => {
            with_app(|app| app.open_editor());
            0
        }
        WM_DEVICE_EVENT => {
            with_app(|app| app.device_events());
            0
        }
        WM_FIRMWARE => {
            with_app(|app| app.firmware_news());
            0
        }
        WM_UPDATE => {
            with_app(|app| app.update_news());
            0
        }
        WM_UPDATE_INSTALL => {
            with_app(|app| app.install_news());
            0
        }
        WM_START_UPDATE => {
            if let Some(r) = REQUESTED_UPDATE.lock().unwrap_or_else(|p| p.into_inner()).take() {
                with_app(|app| app.start_update(r));
            }
            0
        }
        WM_REPORT_READY => {
            with_app(|app| {
                if wparam == 1 {
                    app.balloon(
                        "Device report copied",
                        "Paste it (Ctrl+V) into \"Output of secbird report\" on the page that just opened. It only holds read results; serial numbers are shortened.",
                    )
                } else {
                    app.balloon("Could not copy the device report", "Run `secbird report` in a terminal instead.")
                }
            });
            0
        }
        WM_DEVICECHANGE => {
            let event = wparam as u32;
            if (event == DBT_DEVICEARRIVAL || event == DBT_DEVICEREMOVECOMPLETE) && is_razer(lparam) {
                schedule_rescan(hwnd, false);
            }
            TRUE as LRESULT
        }
        WM_POWERBROADCAST => {
            if wparam as u32 == PBT_APMRESUMEAUTOMATIC {
                log::append("resumed from sleep");
                schedule_rescan(hwnd, true);
                start_retries(hwnd);
            } else if wparam as u32 == PBT_POWERSETTINGCHANGE {
                let setting = &*(lparam as *const POWERBROADCAST_SETTING);
                if same_guid(&setting.PowerSetting, &GUID_CONSOLE_DISPLAY_STATE) {
                    // 0 off, 1 on, 2 dimmed (still on).
                    let off = setting.Data[0] == 0;
                    with_app(|app| app.set_away(app.locked, off));
                }
            }
            TRUE as LRESULT
        }
        WM_WTSSESSION_CHANGE => {
            match wparam as u32 {
                WTS_SESSION_LOCK => with_app(|app| app.set_away(true, app.screen_off)),
                WTS_SESSION_UNLOCK => with_app(|app| app.set_away(false, app.screen_off)),
                _ => None,
            };
            0
        }
        WM_TIMER if wparam == TIMER_RESCAN => {
            KillTimer(hwnd, TIMER_RESCAN);
            if with_app(|app| app.rescan()).is_none() {
                SetTimer(hwnd, TIMER_RESCAN, SETTLE_MS, None);
            }
            0
        }
        WM_TIMER if wparam == TIMER_RETRY => {
            KillTimer(hwnd, TIMER_RETRY);
            let next = with_app(|app| {
                app.rescan();
                app.retry_step += 1;
                RETRY_DELAYS_MS.get(app.retry_step).copied().or_else(|| {
                    let more = app.retry_step < RETRY_DELAYS_MS.len() + SLEEPER_RETRIES;
                    (more && app.engine().has_sleeping()).then_some(SLEEPER_RETRY_MS)
                })
            });
            if let Some(Some(ms)) = next {
                SetTimer(hwnd, TIMER_RETRY, ms, None);
            }
            0
        }
        WM_TIMER if wparam == TIMER_FIRMWARE => {
            SetTimer(hwnd, TIMER_FIRMWARE, FIRMWARE_TIMER_MS, None);
            with_app(|app| {
                app.check_firmware(false);
                app.check_update(false);
            });
            0
        }
        WM_TIMER if wparam == TIMER_BATTERY => {
            with_app(|app| app.check_battery());
            0
        }
        WM_TIMER if wparam == TIMER_BUTTONS => {
            with_app(|app| app.restore_lost_buttons());
            0
        }
        WM_TIMER if wparam == TIMER_LIGHTS => {
            with_app(|app| {
                let mut engine = app.engine();
                engine.retry_lights_back();
                engine.refresh_frames();
                engine.keep_laptops_lit();
            });
            0
        }
        WM_TIMER if wparam == TIMER_FOREGROUND => {
            KillTimer(hwnd, TIMER_FOREGROUND);
            if with_app(|app| app.foreground_changed()).is_none() {
                SetTimer(hwnd, TIMER_FOREGROUND, FOREGROUND_MS, None);
            }
            0
        }
        WM_DESTROY => {
            with_app(|app| remove_icon(app.hwnd));
            PostQuitMessage(0);
            0
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            with_app(|app| {
                add_icon(app.hwnd, app.icon);
                app.update_tooltip();
            });
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// A window came to the front: (re)start the settle timer.
unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    let hwnd = TRAY_HWND.load(Ordering::Relaxed);
    if !hwnd.is_null() {
        SetTimer(hwnd, TIMER_FOREGROUND, FOREGROUND_MS, None);
    }
}

/// Rescan a few more times over the next minute (new devices get their profile).
fn start_retries(hwnd: HWND) {
    with_app(|app| app.retry_step = 0);
    unsafe { SetTimer(hwnd, TIMER_RETRY, RETRY_DELAYS_MS[0], None) };
}

/// Debounce: a device arrival fires one notification per HID interface.
fn schedule_rescan(hwnd: HWND, apply_all: bool) {
    with_app(|app| app.apply_all_pending |= apply_all);
    unsafe { SetTimer(hwnd, TIMER_RESCAN, SETTLE_MS, None) };
}

impl App {
    fn engine(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn rescan(&mut self) {
        let apply_all = std::mem::take(&mut self.apply_all_pending);
        let (reports, new_names, synapse) = {
            let mut engine = self.engine();
            let new = engine.rescan();
            let names: Vec<String> =
                engine.profile_devices().into_iter().filter(|d| new.contains(&d.id)).map(|d| d.label).collect();
            let targets = if apply_all { engine.all_keys() } else { new };
            (engine.apply(&targets), names, engine.synapse().to_vec())
        };
        if !new_names.is_empty() {
            log::append(&format!("found: {}", new_names.join(", ")));
        }
        self.notify_failures(&reports);
        self.warn_synapse(&synapse);
        self.check_battery();
        self.start_listeners();
        // A device that arrives (or wakes) while the PC is locked comes up lit.
        if let Some(percent) = TraySettings::load().away_percent().filter(|_| self.engine().is_away()) {
            self.engine().lights_away(percent);
        }
    }

    /// The lights go down while the PC is locked or its screen is off, if that's set (#48),
    /// and come back after.
    fn set_away(&mut self, locked: bool, screen_off: bool) {
        (self.locked, self.screen_off) = (locked, screen_off);
        let percent = TraySettings::load().away_percent().filter(|_| locked || screen_off);
        let mut engine = self.engine();
        match percent {
            Some(percent) if !engine.is_away() => {
                log::append(&format!("{}: lights to {percent}%", if locked { "locked" } else { "screen off" }));
                engine.lights_away(percent);
            }
            None if engine.is_away() => {
                log::append("back: lights as saved");
                engine.lights_back();
            }
            _ => {}
        }
    }

    /// One thread per event collection, blocked in a read until the device reports
    /// something: no polling, nothing runs in between.
    fn start_listeners(&self) {
        let paths = self.engine().event_paths();
        for (key, path) in paths {
            if !self.listening.lock().unwrap_or_else(|p| p.into_inner()).insert(path.clone()) {
                continue;
            }
            let Some(device) = self.engine().open_events(&path) else {
                self.listening.lock().unwrap_or_else(|p| p.into_inner()).remove(&path);
                continue;
            };
            let (tx, listening, hwnd) = (self.events_tx.clone(), self.listening.clone(), self.hwnd as usize);
            let spawned = std::thread::Builder::new().stack_size(LISTENER_STACK).spawn(move || {
                let mut buf = [0u8; 16];
                // Notification events: [0x05, kind, ...]; kind 2 = DPI (x, y big-endian),
                // kind 1 = on-board profile.
                while let Ok(n) = device.read(&mut buf) {
                    let event = match (n, buf[0], buf[1]) {
                        (6.., 0x05, 0x02) => DeviceEvent::Dpi {
                            key: key.clone(),
                            x: u16::from_be_bytes([buf[2], buf[3]]),
                            y: u16::from_be_bytes([buf[4], buf[5]]),
                        },
                        (3.., 0x05, 0x01) => DeviceEvent::OnboardProfile(buf[2]),
                        _ => continue,
                    };
                    if tx.send(event).is_err() {
                        break;
                    }
                    unsafe { PostMessageW(hwnd as HWND, WM_DEVICE_EVENT, 0, 0) };
                }
                listening.lock().unwrap_or_else(|p| p.into_inner()).remove(&path);
            });
            if spawned.is_err() {
                log::append("could not start a device event listener");
            }
        }
    }

    fn device_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                DeviceEvent::Dpi { key, x, y } => {
                    self.engine().dpi_changed(&key, x, y);
                    if !TraySettings::load().popups_off {
                        osd::show(&if x == y { format!("{x} DPI") } else { format!("{x} × {y} DPI") });
                    }
                }
                DeviceEvent::OnboardProfile(n) => {
                    if !TraySettings::load().popups_off {
                        osd::show(&format!("On-board profile {n}"));
                    }
                }
            }
        }
    }

    /// Switch devices to the profile tied to the program now in front, or back.
    fn foreground_changed(&mut self) {
        let settings = TraySettings::load();
        if settings.auto_switch_off {
            return;
        }
        let exe = apps::foreground();
        let reports = self.engine().set_foreground(exe.clone());
        if reports.is_empty() {
            return;
        }
        let what: Vec<String> = reports.iter().map(|r| format!("{} -> {}", r.device, r.profile)).collect();
        log::append(&format!("{} in front: {}", exe.as_deref().unwrap_or("?"), what.join(", ")));
        let mut names: Vec<&str> = reports.iter().map(|r| r.profile.as_str()).collect();
        names.dedup();
        if !settings.popups_off {
            osd::show(&format!("{} profile", names.join(", ")));
        }
        self.notify_failures(&reports);
        self.update_tooltip();
    }

    /// Synapse re-sends its own settings and holds devices open; say so once per run of it.
    fn warn_synapse(&mut self, running: &[String]) {
        if running.is_empty() {
            if self.synapse_warned {
                log::append("Synapse closed");
            }
            self.synapse_warned = false;
        } else if !self.synapse_warned {
            self.synapse_warned = true;
            log::append(&format!("Synapse running: {}", running.join(", ")));
            self.balloon(
                "Razer Synapse is running",
                "It can overwrite Secretarybird's settings. Close it, or run `secbird debloat` to stop it starting with Windows.",
            );
        }
    }

    fn restore_lost_buttons(&mut self) {
        let now = now_secs();
        self.buttons_restored.retain(|_, at| now.saturating_sub(*at) < BUTTONS_AGAIN_SECS);
        let skip: HashSet<String> = self.buttons_restored.keys().cloned().collect();
        let restored = self.engine().restore_lost_buttons(&skip);
        for (key, report) in &restored {
            self.buttons_restored.insert(key.clone(), now);
            log::append(&format!("{}: button remaps were gone (asleep?), sent {} again", report.device, report.profile));
        }
        let reports: Vec<ApplyReport> = restored.into_iter().map(|(_, r)| r).collect();
        self.notify_failures(&reports);
    }

    fn check_battery(&mut self) {
        self.engine().refresh_battery_lights();
        let batteries = self.engine().batteries();
        let settings = TraySettings::load();
        let low = settings.low_battery();
        for b in &batteries {
            if b.charging || b.percent >= low.saturating_add(10) {
                self.low_battery.remove(&b.key);
            } else if b.percent <= low && self.low_battery.insert(b.key.clone()) {
                log::append(&format!("low battery: {} {}%", b.name, b.percent));
                if !settings.battery_warning_off {
                    self.balloon("Battery low", &format!("{}: {}%", b.name, b.percent));
                }
            }
        }
        self.batteries = batteries;
        self.update_tooltip();
    }

    /// Start the local editor server on first use, then open it in the default browser.
    fn open_editor(&mut self) {
        if self.editor_url.is_none() {
            match server::start(self.engine.clone()) {
                Ok(editor) => self.editor_url = Some(editor.url),
                Err(e) => return self.balloon("Could not start the editor", &e.to_string()),
            }
        }
        if let Some(url) = &self.editor_url {
            #[cfg(debug_assertions)]
            eprintln!("editor: {url}");
            unsafe {
                ShellExecuteW(null_mut(), wide("open").as_ptr(), wide(url).as_ptr(), null(), null(), SW_SHOWNORMAL);
            }
        }
    }

    fn update_tooltip(&self) {
        let mut text = format!("{APP_NAME} {}", env!("CARGO_PKG_VERSION"));
        let engine = self.engine();
        if engine.all_keys().is_empty() {
            text.push_str("\nNo Razer devices");
        }
        for dev in engine.devices() {
            let (_, mut active) = Engine::profiles(dev);
            if let Some(exe) = engine.foreground() {
                let now = engine.effective_profile(dev);
                if now != active {
                    active = format!("{now} (auto: {exe})");
                }
            }
            let battery = match self.batteries.iter().find(|b| b.key == dev.key()) {
                Some(b) if b.charging => format!(" · {}% charging", b.percent),
                Some(b) => format!(" · {}%", b.percent),
                None => String::new(),
            };
            text.push_str(&format!("\n{}: {active}{battery}", engine.label(dev)));
        }
        for device in engine.profile_devices().into_iter().filter(|device| device.id.starts_with("camera:")) {
            text.push_str(&format!("\n{}: {}", device.label, device.active));
        }
        if !engine.synapse().is_empty() {
            text.push_str("\nSynapse is running");
        }
        unsafe {
            let mut nid = icon_data(self.hwnd, self.icon);
            nid.uFlags = NIF_TIP;
            copy_wide(&mut nid.szTip, &text);
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    fn notify_failures(&self, reports: &[ApplyReport]) {
        let lines: Vec<String> = reports
            .iter()
            .filter(|r| !r.failures.is_empty())
            .map(|r| format!("{} ({}): {}", r.device, r.profile, r.failures.join("; ")))
            .collect();
        if !lines.is_empty() {
            for line in &lines {
                log::append(&format!("apply failed: {line}"));
            }
            self.balloon("Some settings could not be applied", &lines.join("\n"));
        }
    }

    fn balloon(&self, title: &str, body: &str) {
        self.notify(title, body, NIIF_WARNING, None);
    }

    /// A tray notification; clicking it opens `url` when there is one.
    fn notify(&self, title: &str, body: &str, icon: NOTIFY_ICON_INFOTIP_FLAGS, url: Option<String>) {
        self.notify_click(title, body, icon, url.map(Click::Open));
    }

    fn notify_click(&self, title: &str, body: &str, icon: NOTIFY_ICON_INFOTIP_FLAGS, click: Option<Click>) {
        self.balloon_click.replace(click);
        unsafe {
            let mut nid = icon_data(self.hwnd, self.icon);
            nid.uFlags = NIF_INFO;
            nid.dwInfoFlags = icon;
            copy_wide(&mut nid.szInfoTitle, title);
            copy_wide(&mut nid.szInfo, body);
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    /// The weekly firmware check, if it's on and due (or `now`). Reads each device's version
    /// here, then asks Razer's servers on a background thread so the tray never waits.
    fn check_firmware(&mut self, now: bool) {
        let mut settings = TraySettings::load();
        if !settings.firmware_weekly || (!now && now_secs().saturating_sub(settings.firmware_checked) < FIRMWARE_EVERY_SECS) {
            return;
        }
        let devices: Vec<(String, String, u16, String)> = {
            let engine = self.engine();
            engine
                .devices()
                .iter()
                .filter_map(|d| d.firmware_version().ok().map(|v| (d.key(), engine.label(d), d.pid, v)))
                .collect()
        };
        if devices.is_empty() {
            return;
        }
        settings.firmware_checked = now_secs();
        settings.save();
        let connected: Vec<u16> = devices.iter().map(|d| d.2).collect();
        let tx = self.firmware_tx.clone();
        let hwnd = self.hwnd as usize;
        std::thread::spawn(move || {
            use secbird_core::firmware::{latest_for, status, Status};
            let news: Vec<FirmwareNews> = devices
                .into_iter()
                .filter_map(|(key, name, pid, current)| match status(current, latest_for(pid).ok().flatten(), &connected) {
                    Status::Update { latest, .. } => Some(FirmwareNews { key, name, version: latest.version }),
                    _ => None,
                })
                .collect();
            log::append(&format!("weekly firmware check: {} update(s)", news.len()));
            if !news.is_empty() && tx.send(news).is_ok() {
                unsafe { PostMessageW(hwnd as HWND, WM_FIRMWARE, 0, 0) };
            }
        });
    }

    /// The daily new-version check, unless turned off and unless asked from the menu
    /// (`manual`). Asks GitHub on a background thread so the tray never waits.
    fn check_update(&mut self, manual: bool) {
        let mut settings = TraySettings::load();
        if !manual && (settings.updates_off || now_secs().saturating_sub(settings.update_checked) < UPDATE_EVERY_SECS) {
            return;
        }
        settings.update_checked = now_secs();
        settings.save();
        let tx = self.updates_tx.clone();
        let hwnd = self.hwnd as usize;
        std::thread::spawn(move || {
            let found = secbird_core::update::check().map_err(|e| e.to_string());
            match &found {
                Ok(Some(r)) => log::append(&format!("new version check: {} is out", r.version)),
                Ok(None) => log::append("new version check: up to date"),
                Err(e) => log::append(&format!("new version check failed: {e}")),
            }
            if tx.send(UpdateResult { manual, found }).is_ok() {
                unsafe { PostMessageW(hwnd as HWND, WM_UPDATE, 0, 0) };
            }
        });
    }

    /// Announce a newer version once (or always, when asked from the menu).
    fn update_news(&mut self) {
        while let Ok(UpdateResult { manual, found }) = self.updates.try_recv() {
            let current = secbird_core::update::CURRENT;
            match found {
                Ok(Some(r)) => {
                    let mut settings = TraySettings::load();
                    if manual || settings.update_notified != r.version {
                        settings.update_notified = r.version.clone();
                        settings.save();
                        let title = format!("Secretarybird {} is out", r.version);
                        if secbird_core::update::installed() {
                            let body = format!("You have {current}. Click to update now; your settings and profiles are kept.");
                            self.notify_click(&title, &body, NIIF_INFO, Some(Click::Update(r.clone())));
                        } else {
                            let body = format!("You have {current}. Click here for the download; your settings and profiles are kept.");
                            self.notify(&title, &body, NIIF_INFO, Some(r.url.clone()));
                        }
                    }
                    self.update = Some(r);
                }
                Ok(None) => {
                    self.update = None;
                    if manual {
                        self.notify("You're up to date", &format!("Secretarybird {current} is the latest version."), NIIF_INFO, None);
                    }
                }
                Err(e) if manual => self.balloon("Could not check for updates", &e),
                Err(_) => {}
            }
        }
    }

    /// Download the update, check its signature and run its installer, which closes this
    /// tray and starts the new one. Copies that weren't installed get the release page.
    fn start_update(&mut self, r: Release) {
        if !secbird_core::update::installed() {
            return open_url(&r.url);
        }
        if self.updating {
            return;
        }
        self.updating = true;
        self.notify(
            &format!("Updating to Secretarybird {}", r.version),
            "Downloading. It installs by itself and starts again in a few seconds; your settings and profiles are kept.",
            NIIF_INFO,
            None,
        );
        let tx = self.installs_tx.clone();
        let hwnd = self.hwnd as usize;
        std::thread::spawn(move || {
            log::append(&format!("updating to {}", r.version));
            let result = secbird_core::update::download_installer(&r.version)
                .and_then(|path| secbird_core::update::run_installer(&path))
                .map_err(|e| format!("{e}\n{}", r.url));
            match &result {
                Ok(()) => log::append("update: installer started"),
                Err(e) => log::append(&format!("update failed: {e}")),
            }
            if tx.send(result).is_ok() {
                unsafe { PostMessageW(hwnd as HWND, WM_UPDATE_INSTALL, 0, 0) };
            }
        });
    }

    /// An update that couldn't be installed. A started installer closes the tray itself.
    fn install_news(&mut self) {
        while let Ok(result) = self.installs.try_recv() {
            if let Err(e) = result {
                self.updating = false;
                let (why, url) = e.rsplit_once('\n').unwrap_or((&e, ""));
                let body = format!("{why}\nClick for the download page.");
                self.notify("Couldn't update Secretarybird", &body, NIIF_WARNING, Some(url.to_string()).filter(|u| !u.is_empty()));
            }
        }
    }

    /// Once after an update: open the new version's release notes (a notification alone is
    /// easy to miss) and say which version is running now.
    fn announce_updated(&mut self) {
        let current = secbird_core::update::CURRENT;
        let mut settings = TraySettings::load();
        if settings.last_version == current {
            return;
        }
        let updated = !settings.last_version.is_empty() && secbird_core::firmware::is_newer(current, &settings.last_version);
        settings.last_version = current.to_string();
        settings.save();
        if updated {
            let url = format!("{}/releases/tag/v{current}", env!("CARGO_PKG_REPOSITORY"));
            open_url(&url);
            self.notify(&format!("Updated to Secretarybird {current}"), "What's new is open in your browser.", NIIF_INFO, None);
        }
    }

    /// Build `secbird report` (a few seconds of read-only USB requests) on a background
    /// thread, put it on the clipboard and open the GitHub device report form.
    fn make_report(&mut self) {
        let engine = self.engine.clone();
        let hwnd = self.hwnd as usize;
        std::thread::spawn(move || {
            let text = engine.lock().unwrap_or_else(|p| p.into_inner()).report_text();
            let copied = set_clipboard(&text);
            if copied {
                let url = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new?template=device-report.yml");
                unsafe {
                    ShellExecuteW(null_mut(), wide("open").as_ptr(), wide(url).as_ptr(), null(), null(), SW_SHOWNORMAL);
                }
            }
            unsafe { PostMessageW(hwnd as HWND, WM_REPORT_READY, copied as usize, 0) };
        });
    }

    /// Announce each newer firmware once.
    fn firmware_news(&mut self) {
        let mut settings = TraySettings::load();
        let mut lines = Vec::new();
        while let Ok(news) = self.firmware.try_recv() {
            for n in news {
                if settings.firmware_notified.get(&n.key) != Some(&n.version) {
                    lines.push(format!("{}: {}", n.name, n.version));
                    settings.firmware_notified.insert(n.key, n.version);
                }
            }
        }
        if !lines.is_empty() {
            settings.save();
            self.balloon("Firmware update available", &format!("{}\nOpen the editor for Razer's download link.", lines.join("\n")));
        }
    }

    /// Build the popup menu and remember which item maps to which profile.
    unsafe fn build_menu(&mut self) -> HMENU {
        let menu = CreatePopupMenu();
        self.profile_items.clear();
        if let Some(r) = &self.update {
            let label = if self.updating {
                format!("Updating to Secretarybird {}…", r.version)
            } else if secbird_core::update::installed() {
                format!("Update to Secretarybird {}", r.version)
            } else {
                format!("Get Secretarybird {}…", r.version)
            };
            let grey = if self.updating { MF_GRAYED } else { 0 };
            append(menu, MF_STRING | grey, ID_GET_UPDATE as usize, &label);
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
        }
        append(menu, MF_STRING, ID_EDITOR as usize, "Open editor");
        SetMenuDefaultItem(menu, ID_EDITOR, 0);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        let engine = self.engine.clone();
        let engine = engine.lock().unwrap_or_else(|p| p.into_inner());
        if engine.all_keys().is_empty() {
            append(menu, MF_STRING | MF_GRAYED, 0, "No Razer devices found");
        }
        for dev in engine.profile_devices() {
            append(menu, MF_STRING | MF_GRAYED, 0, &dev.label);
            for name in dev.profiles {
                let id = ID_PROFILE_BASE + self.profile_items.len() as u32;
                let check = if name == dev.active { MF_CHECKED } else { MF_UNCHECKED };
                append(menu, MF_STRING | check, id as usize, &format!("    {name}"));
                self.profile_items.push((dev.id.clone(), name));
            }
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        append(menu, MF_STRING, ID_REAPPLY as usize, "Re-apply profiles");
        append(menu, MF_STRING, ID_FOLDER as usize, "Open settings folder");
        append(menu, MF_STRING, ID_RESTORE_BUTTONS as usize, "Restore all mouse buttons");
        append(menu, MF_STRING, ID_REPORT as usize, "Make a device report…");
        let auto = if autostart_enabled() { MF_CHECKED } else { MF_UNCHECKED };
        append(menu, MF_STRING | auto, ID_AUTOSTART as usize, "Start with Windows");
        let fw = if TraySettings::load().firmware_weekly { MF_CHECKED } else { MF_UNCHECKED };
        append(menu, MF_STRING | fw, ID_FIRMWARE as usize, "Check for firmware updates weekly");
        let notify = if TraySettings::load().updates_off { MF_UNCHECKED } else { MF_CHECKED };
        append(menu, MF_STRING | notify, ID_UPDATE_NOTIFY as usize, "Tell me when a new version is out");
        append(menu, MF_STRING, ID_CHECK_UPDATE as usize, "Check for updates now");
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        append(menu, MF_STRING, ID_EXIT as usize, "Exit");
        menu
    }

    fn handle_command(&mut self, id: u32) {
        match id {
            ID_REAPPLY => {
                self.apply_all_pending = true;
                self.rescan();
            }
            ID_EDITOR => self.open_editor(),
            ID_FOLDER => open_config_folder(),
            ID_REPORT => self.make_report(),
            ID_RESTORE_BUTTONS => {
                let results = self.engine().restore_buttons();
                let lines: Vec<String> = results
                    .iter()
                    .map(|(name, r)| match r {
                        Ok(0) => format!("{name}: nothing was remapped"),
                        Ok(n) => format!("{name}: {n} button(s) restored"),
                        Err(e) => format!("{name}: {e}"),
                    })
                    .collect();
                let body = if lines.is_empty() { "No mouse with remappable buttons is connected.".to_string() } else { lines.join("\n") };
                self.balloon("Mouse buttons restored", &body);
            }
            ID_AUTOSTART => set_autostart(!autostart_enabled()),
            ID_CHECK_UPDATE => self.check_update(true),
            ID_GET_UPDATE => {
                if let Some(r) = self.update.clone() {
                    self.start_update(r);
                }
            }
            ID_UPDATE_NOTIFY => {
                let mut s = TraySettings::load();
                s.updates_off = !s.updates_off;
                s.save();
                if s.updates_off {
                    self.update = None;
                } else {
                    self.check_update(true);
                }
            }
            ID_FIRMWARE => {
                let mut s = TraySettings::load();
                s.firmware_weekly = !s.firmware_weekly;
                s.save();
                if s.firmware_weekly {
                    self.balloon(
                        "Weekly firmware check on",
                        "Once a week Secretarybird asks apps.razer.com whether newer firmware is out for your devices. It never installs anything.",
                    );
                    self.check_firmware(true);
                }
            }
            ID_EXIT => unsafe {
                DestroyWindow(self.hwnd);
            },
            id if id >= ID_PROFILE_BASE => {
                let idx = (id - ID_PROFILE_BASE) as usize;
                if let Some((key, name)) = self.profile_items.get(idx).cloned() {
                    let result = self.engine().switch_profile(&key, &name);
                    match result {
                        Ok(reports) => self.notify_failures(&reports),
                        Err(e) => self.balloon("Could not switch profile", &e.to_string()),
                    }
                    self.update_tooltip();
                }
            }
            _ => {}
        }
    }
}

/// Open a web page in the default browser.
fn open_url(url: &str) {
    unsafe { ShellExecuteW(null_mut(), wide("open").as_ptr(), wide(url).as_ptr(), null(), null(), SW_SHOWNORMAL) };
}

unsafe fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str) {
    AppendMenuW(menu, flags, id, wide(text).as_ptr());
}

/// TrackPopupMenu runs a modal loop that re-enters the window procedure, so the app
/// state is not borrowed while the menu is open.
unsafe fn show_menu(hwnd: HWND) {
    let Some(menu) = with_app(|app| app.build_menu()) else { return };
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if cmd > 0 {
        with_app(|app| app.handle_command(cmd as u32));
    }
}

unsafe fn icon_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = zeroed();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    nid
}

unsafe fn add_icon(hwnd: HWND, icon: HICON) {
    let mut nid = icon_data(hwnd, icon);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    copy_wide(&mut nid.szTip, APP_NAME);
    Shell_NotifyIconW(NIM_ADD, &nid);
}

unsafe fn remove_icon(hwnd: HWND) {
    let nid = icon_data(hwnd, null_mut());
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

unsafe fn register_hid_notifications(hwnd: HWND) {
    let mut filter: DEV_BROADCAST_DEVICEINTERFACE_W = zeroed();
    filter.dbcc_size = size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32;
    filter.dbcc_devicetype = DBT_DEVTYP_DEVICEINTERFACE;
    filter.dbcc_classguid = GUID_DEVINTERFACE_HID;
    RegisterDeviceNotificationW(hwnd, &filter as *const _ as *const c_void, DEVICE_NOTIFY_WINDOW_HANDLE);
    // UVC cameras do not necessarily expose any HID interface.
    filter.dbcc_classguid = windows_sys::core::GUID::from_u128(0x65e8773d_8f56_11d0_a3b9_00a0c9223196);
    RegisterDeviceNotificationW(hwnd, &filter as *const _ as *const c_void, DEVICE_NOTIFY_WINDOW_HANDLE);
    filter.dbcc_classguid = windows_sys::core::GUID::from_u128(0xe5323777_f976_4f5b_9b55_b94699c46e44);
    RegisterDeviceNotificationW(hwnd, &filter as *const _ as *const c_void, DEVICE_NOTIFY_WINDOW_HANDLE);
}

/// True when a device-change notification names a Razer (VID 1532) interface.
unsafe fn is_razer(lparam: LPARAM) -> bool {
    let hdr = lparam as *const DEV_BROADCAST_HDR;
    if hdr.is_null() || (*hdr).dbch_devicetype != DBT_DEVTYP_DEVICEINTERFACE {
        return false;
    }
    let iface = lparam as *const DEV_BROADCAST_DEVICEINTERFACE_W;
    let name_ptr = std::ptr::addr_of!((*iface).dbcc_name) as *const u16;
    let mut len = 0;
    while *name_ptr.add(len) != 0 && len < 512 {
        len += 1;
    }
    let name = String::from_utf16_lossy(std::slice::from_raw_parts(name_ptr, len));
    name.to_ascii_uppercase().contains("VID_1532")
}


/// Put text on the clipboard. False if another program holds it.
fn set_clipboard(text: &str) -> bool {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    const CF_UNICODETEXT: u32 = 13;
    let data = wide(&text.replace("\r\n", "\n").replace('\n', "\r\n"));
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2);
        if mem.is_null() {
            return false;
        }
        let dst = GlobalLock(mem) as *mut u16;
        if dst.is_null() {
            GlobalFree(mem);
            return false;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
        GlobalUnlock(mem);
        // Retry briefly: clipboard managers open it for a moment after every change.
        for _ in 0..10 {
            if OpenClipboard(null_mut()) != 0 {
                EmptyClipboard();
                let ok = !SetClipboardData(CF_UNICODETEXT, mem).is_null();
                CloseClipboard();
                if !ok {
                    GlobalFree(mem);
                }
                return ok;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        GlobalFree(mem);
        false
    }
}


/// The tray variant of the logo (white background, resource id 2 from build.rs), at the
/// small-icon size Windows uses for the notification area. Resource 1 is the app icon.
unsafe fn load_icon() -> HICON {
    let icon = LoadImageW(
        GetModuleHandleW(null()),
        2 as _,
        IMAGE_ICON,
        GetSystemMetrics(SM_CXSMICON),
        GetSystemMetrics(SM_CYSMICON),
        LR_DEFAULTCOLOR,
    );
    if icon.is_null() {
        LoadIconW(null_mut(), IDI_APPLICATION)
    } else {
        icon as HICON
    }
}
