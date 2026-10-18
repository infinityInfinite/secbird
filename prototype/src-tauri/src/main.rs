#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_autostart::ManagerExt;
use serde::{Deserialize, Serialize};
mod discovery;

#[derive(Clone, Serialize, Deserialize)]
struct Preferences { close_to_tray: bool, minimize_to_tray: bool }
impl Default for Preferences {
    fn default() -> Self { Self { close_to_tray: true, minimize_to_tray: false } }
}
struct Desktop(Mutex<Preferences>);

#[tauri::command]
async fn detect_devices(app: tauri::AppHandle) -> Result<Vec<discovery::DetectedDevice>,String> {
    let rows=tauri::async_runtime::spawn_blocking(discovery::enumerate).await.map_err(|e|e.to_string())??;
    update_device_menu(&app,&rows).map_err(|e|e.to_string())?;
    Ok(rows)
}
fn update_device_menu(app:&tauri::AppHandle,rows:&[discovery::DetectedDevice])->tauri::Result<()> {
    let open=MenuItem::with_id(app,"open","Open Secbird",true,None::<&str>)?;
    let items=rows.iter().map(|d|MenuItem::with_id(app,format!("device:{}",d.id),&d.name,true,None::<&str>)).collect::<tauri::Result<Vec<_>>>()?;
    let refs:Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>>=items.iter().map(|i|i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let devices=Submenu::with_items(app,"Detected devices",!rows.is_empty(),&refs)?;
    let release=MenuItem::with_id(app,"release","Close interface (release WebView)",true,None::<&str>)?;
    let quit=MenuItem::with_id(app,"quit","Quit",true,None::<&str>)?;
    let menu=Menu::with_items(app,&[&open,&devices,&release,&quit])?;
    if let Some(tray)=app.tray_by_id("secbird"){tray.set_menu(Some(menu))?;}
    Ok(())
}

fn open_window(app: &tauri::AppHandle, device: Option<&str>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
        if let Some(device) = device { window.emit("demo-device", device)?; }
    } else {
        let path = device
            .filter(|id| id.starts_with("razer-")&&id.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'))
            .map(|id| format!("index.html#{id}"))
            .unwrap_or_else(|| "index.html".to_string());
        WebviewWindowBuilder::new(app, "main", WebviewUrl::App(path.into()))
            .title("Secretarybird")
            .inner_size(1180.0, 780.0).min_inner_size(820.0, 600.0)
            .center().resizable(true).decorations(false).build()?;
    }
    Ok(())
}

#[tauri::command]
fn desktop_preferences(app: tauri::AppHandle, state: tauri::State<Desktop>) -> Result<serde_json::Value, String> {
    let prefs = state.0.lock().map_err(|e| e.to_string())?.clone();
    let autostart = app.autolaunch().is_enabled().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"closeToTray":prefs.close_to_tray,"minimizeToTray":prefs.minimize_to_tray,"autostart":autostart}))
}

#[tauri::command]
fn set_desktop_preferences(app: tauri::AppHandle, state: tauri::State<Desktop>, close_to_tray: bool, minimize_to_tray: bool, autostart: bool) -> Result<(), String> {
    if autostart { app.autolaunch().enable() } else { app.autolaunch().disable() }.map_err(|e| e.to_string())?;
    let prefs = Preferences { close_to_tray, minimize_to_tray };
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(&prefs).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("desktop.json"), bytes).map_err(|e| e.to_string())?;
    *state.0.lock().map_err(|e| e.to_string())? = prefs;
    Ok(())
}

#[tauri::command]
fn release_interface(window: tauri::WebviewWindow) -> Result<(), String> {
    // Only destroys the view. The app/tray remains alive. No hardware exists in this build.
    window.destroy().map_err(|e| e.to_string())
}

fn main() {
    if std::env::args().any(|a|a=="--list-devices") {
        match discovery::enumerate(){Ok(rows)=>println!("{}",serde_json::to_string(&rows).unwrap()),Err(error)=>{eprintln!("{error}");std::process::exit(1);}}
        return;
    }
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| { let _ = open_window(app, None); }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .manage(Desktop(Mutex::new(Preferences::default())))
        .invoke_handler(tauri::generate_handler![desktop_preferences, set_desktop_preferences, release_interface, detect_devices])
        .setup(|app| {
            if let Ok(dir) = app.path().app_config_dir() {
                if let Ok(text) = std::fs::read_to_string(dir.join("desktop.json")) {
                    if let Ok(prefs) = serde_json::from_str(&text) { *app.state::<Desktop>().0.lock().unwrap() = prefs; }
                }
            }
            // Keep Secretarybird's original bundled icon in the tray.
            TrayIconBuilder::with_id("secbird").icon(app.default_window_icon().expect("bundled Secbird icon").clone())
                .tooltip("Secretarybird").show_menu_on_left_click(false)
                .on_menu_event(|app, event| { match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "release" => { if let Some(w) = app.get_webview_window("main") { let _ = w.destroy(); } },
                    id if id.starts_with("device:") => { let _ = open_window(app, id.strip_prefix("device:")); },
                    _ => { let _ = open_window(app, None); }
                } })
                .on_tray_icon_event(|tray, event| {
                    if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }) {
                        let _ = open_window(tray.app_handle(), None);
                    }
                }).build(app)?;
            update_device_menu(app.handle(),&[])?;
            open_window(app.handle(), None)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let state = window.app_handle().state::<Desktop>();
            let Ok(prefs) = state.0.lock() else { return };
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } if prefs.close_to_tray => {
                    api.prevent_close(); let _ = window.hide();
                }
                tauri::WindowEvent::Resized(_) if prefs.minimize_to_tray => {
                    if window.is_minimized().unwrap_or(false) { let _ = window.hide(); }
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!()).expect("Secbird desktop demo initialization failed");
    app.run(|_, event| {
        // Closing/destroying the last view must not terminate the tray/service lifetime.
        if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event { api.prevent_exit(); }
    });
}
