//! Secretarybird tray app: re-applies each device's profile when it is plugged in or the
//! PC wakes, and switches profiles from the tray menu. Plain Win32, no GUI toolkit.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod apps;
#[cfg(any(windows, test))]
mod camera;
#[cfg(any(windows, test))]
#[cfg_attr(not(windows), allow(dead_code))]
mod engine;
#[cfg(windows)]
mod osd;
#[cfg(windows)]
mod server;
#[cfg(windows)]
mod settings;
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
    match server::start(std::sync::Arc::new(std::sync::Mutex::new(engine))) {
        Ok(editor) => {
            println!("{}", editor.url);
            loop {
                std::thread::park();
            }
        }
        Err(e) => eprintln!("could not start the editor: {e}"),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("secbird-tray only runs on Windows");
}
