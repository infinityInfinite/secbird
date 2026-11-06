//! Kiyo Pro controls, separate from the HID peripheral protocol.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub mod actions;
pub mod controls;

#[cfg(any(windows, test))]
mod protocol;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_controls;
#[cfg(any(windows, test))]
mod worker;

pub const KIYO_PRO_PID: u16 = 0x0E05;

#[derive(Debug, Clone, Serialize)]
pub struct CameraInfo {
    pub id: String,
    pub pid: u16,
    pub name: String,
    pub control_error: Option<String>,
}

pub trait CameraBackend: Send {
    fn discover(&self) -> Result<Vec<CameraInfo>>;
    fn set(&self, id: &str, setting: Setting) -> Result<()>;
    fn controls(&self, _id: &str) -> Result<Vec<controls::ControlInfo>> {
        Err(Error::Unsupported(
            "standard controls require Windows".into(),
        ))
    }
    fn set_control(
        &self,
        _id: &str,
        _control: controls::Control,
        _value: controls::ControlValue,
    ) -> Result<controls::ControlState> {
        Err(Error::Unsupported(
            "standard controls require Windows".into(),
        ))
    }
}

pub fn backend() -> Result<Box<dyn CameraBackend>> {
    #[cfg(windows)]
    {
        windows::start()
    }
    #[cfg(not(windows))]
    {
        Ok(Box::new(Unsupported))
    }
}

#[cfg(not(windows))]
struct Unsupported;
#[cfg(not(windows))]
impl CameraBackend for Unsupported {
    fn discover(&self) -> Result<Vec<CameraInfo>> {
        Ok(Vec::new())
    }
    fn set(&self, _: &str, _: Setting) -> Result<()> {
        Err(Error::Unsupported(
            "camera controls require Windows 10/11".into(),
        ))
    }
}

#[cfg(any(windows, test))]
fn camera_id(path: &str) -> String {
    format!("camera:{}", path.to_ascii_lowercase())
}

#[cfg(any(windows, test))]
fn is_kiyo_pro_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let parts: Vec<_> = lower.split(['#', '&', '\\', '?']).collect();
    parts.contains(&"vid_1532") && parts.contains(&"pid_0e05")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HdrMode {
    Dark,
    Bright,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldOfView {
    Wide,
    Medium,
    Narrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutofocusMode {
    Responsive,
    Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Hdr(bool),
    HdrMode(HdrMode),
    FieldOfView(FieldOfView),
    AutofocusMode(AutofocusMode),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraSettings {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub controls: std::collections::BTreeMap<controls::Control, controls::ControlValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr_mode: Option<HdrMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fov: Option<FieldOfView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub af_mode: Option<AutofocusMode>,
}

pub fn parse_setting(control: &str, value: &str) -> Result<Setting> {
    match (control, value) {
        ("hdr", "on") => Ok(Setting::Hdr(true)),
        ("hdr", "off") => Ok(Setting::Hdr(false)),
        ("hdr-mode", "dark") => Ok(Setting::HdrMode(HdrMode::Dark)),
        ("hdr-mode", "bright") => Ok(Setting::HdrMode(HdrMode::Bright)),
        ("fov", "wide") => Ok(Setting::FieldOfView(FieldOfView::Wide)),
        ("fov", "medium") => Ok(Setting::FieldOfView(FieldOfView::Medium)),
        ("fov", "narrow") => Ok(Setting::FieldOfView(FieldOfView::Narrow)),
        ("af-mode", "responsive") => Ok(Setting::AutofocusMode(AutofocusMode::Responsive)),
        ("af-mode", "passive") => Ok(Setting::AutofocusMode(AutofocusMode::Passive)),
        _ => Err(Error::InvalidArg(format!("camera {control} {value}: expected hdr on|off, hdr-mode dark|bright, fov wide|medium|narrow, or af-mode responsive|passive"))),
    }
}

#[cfg(test)]
mod tests;
