//! Secretarybird tray app: re-applies each device's profile when it is plugged in or the
//! PC wakes, and switches profiles from the tray menu. Plain Win32, no GUI toolkit.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod apps;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod osd;
#[cfg(windows)]
mod server;
#[cfg(windows)]
mod sensors;
#[cfg(windows)]
mod settings;
#[cfg(windows)]
mod studio;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    // Development: serve the editor only (no tray icon, no single-instance lock, nothing
    // applied), print its URL and keep running. Used to look at the editor beside a running
    // tray, e.g. with SECBIRD_SIM devices. Debug builds have a console for the URL.
    if std::env::args().any(|a| a == "--serve") {
        return serve();
    }
    win::run();
}

#[cfg(windows)]
fn serve() {
    let mut engine = match engine::Engine::new() {
        Ok(e) => e,
        Err(e) => return eprintln!("could not start HID access: {e}"),
    };
    engine.rescan();
    let engine = std::sync::Arc::new(std::sync::Mutex::new(engine));
    match server::start(engine.clone()) {
        Ok(editor) => {
            println!("{}", editor.url);
            // Layered lighting runs here too, so Studio can be tried on SECBIRD_SIM devices.
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1000 / secbird_core::studio::FPS as u64));
                engine.lock().unwrap_or_else(|p| p.into_inner()).studio_tick();
            }
        }
        Err(e) => eprintln!("could not start the editor: {e}"),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("secbird-tray only runs on Windows");
}
