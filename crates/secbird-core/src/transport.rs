//! The byte pipe a [`Device`](crate::Device) talks through: real USB HID, or a simulator.

use crate::error::Result;
use std::time::Duration;

pub trait Transport: Send {
    /// Write one feature report (byte 0 is the HID report id).
    fn send_feature_report(&self, data: &[u8]) -> Result<()>;
    /// Read one feature report into `buf`; returns the number of bytes read.
    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize>;
    /// Wait between write and each read. Synapse uses 5 ms; simulators need none.
    fn read_delay(&self) -> Duration {
        Duration::from_millis(5)
    }
    /// The feature report id the Razer report goes out under; 0 (none) over USB.
    fn report_id(&self) -> u8 {
        0
    }
}

/// A Razer feature report the HID descriptor numbers, as over Bluetooth (see
/// [`crate::hid_descriptor`]). Callers keep framing reports the USB way, byte 0 = 0 and
/// 90 bytes after it; this sends them under `id`, sized to the declared `len` (at least 90).
pub struct NumberedReport<T: Transport> {
    pub inner: T,
    pub id: u8,
    pub len: usize,
}

impl<T: Transport> Transport for NumberedReport<T> {
    fn send_feature_report(&self, data: &[u8]) -> Result<()> {
        let mut buf = vec![0u8; self.len + 1];
        buf[0] = self.id;
        let n = data.len().saturating_sub(1).min(self.len);
        buf[1..=n].copy_from_slice(&data[1..=n]);
        self.inner.send_feature_report(&buf)
    }

    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize> {
        let mut raw = vec![0u8; self.len + 1];
        raw[0] = self.id;
        let n = self.inner.get_feature_report(&mut raw)?.min(buf.len());
        buf[..n].copy_from_slice(&raw[..n]);
        buf[0] = 0;
        Ok(n)
    }

    fn read_delay(&self) -> Duration {
        self.inner.read_delay()
    }

    fn report_id(&self) -> u8 {
        self.id
    }
}

impl Transport for hidapi::HidDevice {
    fn send_feature_report(&self, data: &[u8]) -> Result<()> {
        Ok(hidapi::HidDevice::send_feature_report(self, data)?)
    }

    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(hidapi::HidDevice::get_feature_report(self, buf)?)
    }
}

/// The feature reports the HID interface at `path` declares: (report id, bytes). Empty when
/// it declares none or can't be read.
///
/// Windows: from the driver's caps. hidapi rebuilds a descriptor from them, but the rebuilt
/// one leaves the Razer report out (Basilisk V3 interface 3 shows an empty collection).
/// Elsewhere: parsed from the real descriptor. Every id gets the longest feature length,
/// which is all Windows tells.
pub fn feature_reports(api: &hidapi::HidApi, path: &std::ffi::CStr) -> Vec<(u8, usize)> {
    #[cfg(windows)]
    {
        let _ = api;
        windows_feature_reports(path)
    }
    #[cfg(not(windows))]
    {
        let Ok(hid) = api.open_path(path) else { return Vec::new() };
        let mut desc = [0u8; 4096];
        match hid.get_report_descriptor(&mut desc) {
            Ok(n) => crate::hid_descriptor::feature_reports(&desc[..n]),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(windows)]
fn windows_feature_reports(path: &std::ffi::CStr) -> Vec<(u8, usize)> {
    use windows_sys::Win32::Devices::HumanInterfaceDevice::*;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileA, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};

    // SAFETY: the handle and preparsed data are released before returning; buffers are
    // sized from the counts the driver gives.
    unsafe {
        // Access 0: enough to read caps, even for collections Windows keeps for itself.
        let handle = CreateFileA(path.as_ptr().cast(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut());
        if handle == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut data: PHIDP_PREPARSED_DATA = 0;
        if HidD_GetPreparsedData(handle, &mut data) != 0 {
            let mut caps: HIDP_CAPS = std::mem::zeroed();
            if HidP_GetCaps(data, &mut caps) == HIDP_STATUS_SUCCESS && caps.FeatureReportByteLength > 1 {
                let len = caps.FeatureReportByteLength as usize - 1;
                let mut ids = Vec::new();
                let mut n = caps.NumberFeatureValueCaps;
                if n > 0 {
                    let mut list: Vec<HIDP_VALUE_CAPS> = vec![std::mem::zeroed(); n as usize];
                    if HidP_GetValueCaps(HidP_Feature, list.as_mut_ptr(), &mut n, data) == HIDP_STATUS_SUCCESS {
                        ids.extend(list[..n as usize].iter().map(|c| c.ReportID));
                    }
                }
                let mut n = caps.NumberFeatureButtonCaps;
                if n > 0 {
                    let mut list: Vec<HIDP_BUTTON_CAPS> = vec![std::mem::zeroed(); n as usize];
                    if HidP_GetButtonCaps(HidP_Feature, list.as_mut_ptr(), &mut n, data) == HIDP_STATUS_SUCCESS {
                        ids.extend(list[..n as usize].iter().map(|c| c.ReportID));
                    }
                }
                ids.sort_unstable();
                ids.dedup();
                if ids.is_empty() {
                    ids.push(0);
                }
                out = ids.into_iter().map(|id| (id, len)).collect();
            }
            HidD_FreePreparsedData(data);
        }
        CloseHandle(handle);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::REPORT_LEN;
    use crate::sim::{Sim, MOUSE_PID};
    use std::sync::Mutex;

    /// A Bluetooth-style collection: takes the Razer report only as id 5 with 91 bytes
    /// after it, and hands the simulator's reply back the same way.
    struct Bluetooth {
        sim: Sim,
        sent: Mutex<Vec<usize>>,
    }

    impl Transport for Bluetooth {
        fn send_feature_report(&self, data: &[u8]) -> Result<()> {
            self.sent.lock().unwrap().push(data.len());
            if data[0] != 5 || data.len() != 92 {
                return Err(crate::Error::InvalidArg("report id 0 isn't declared".into()));
            }
            let mut usb = [0u8; REPORT_LEN + 1];
            usb[1..].copy_from_slice(&data[1..=REPORT_LEN]);
            self.sim.send_feature_report(&usb)
        }

        fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize> {
            assert_eq!((buf[0], buf.len()), (5, 92));
            let mut usb = [0u8; REPORT_LEN + 1];
            self.sim.get_feature_report(&mut usb)?;
            buf[1..=REPORT_LEN].copy_from_slice(&usb[1..]);
            Ok(buf.len())
        }

        fn read_delay(&self) -> Duration {
            Duration::ZERO
        }
    }

    #[test]
    fn numbered_report_reaches_a_bluetooth_collection() {
        let open = |t: Box<dyn Transport>| {
            crate::Device::open(t, MOUSE_PID, -1, c"bt".into(), &crate::device::TXID_CANDIDATES, Sim::spec_for(MOUSE_PID))
        };
        let plain = Bluetooth { sim: Sim::mouse(), sent: Mutex::new(Vec::new()) };
        assert!(open(Box::new(plain)).is_none(), "report id 0 is refused, as on Windows");

        let bt = NumberedReport { inner: Bluetooth { sim: Sim::mouse(), sent: Mutex::new(Vec::new()) }, id: 5, len: 91 };
        let dev = open(Box::new(bt)).expect("answers under report id 5");
        assert_eq!(dev.report_id(), 5);
        assert_eq!(dev.dpi().unwrap(), (800, 800));
    }
}
