//! The byte pipe a [`Device`](crate::Device) talks through: real USB HID, or a simulator.

use crate::error::{Error, Result};
use std::time::Duration;

pub trait Transport: Send {
    /// Write one feature report (byte 0 is the HID report id).
    fn send_feature_report(&self, data: &[u8]) -> Result<()>;
    /// Read one feature report into `buf`; returns the number of bytes read.
    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize>;
    /// Output/input reports, used by newer audio devices instead of feature reports.
    fn send_output_report(&self, _data: &[u8]) -> Result<()> {
        Err(Error::Unsupported("HID output reports on this transport".into()))
    }
    fn read_timeout(&self, _buf: &mut [u8], _timeout_ms: i32) -> Result<usize> {
        Err(Error::Unsupported("HID input reports on this transport".into()))
    }
    /// Wait between write and each read. Synapse uses 5 ms; simulators need none.
    fn read_delay(&self) -> Duration {
        Duration::from_millis(5)
    }
}

impl Transport for hidapi::HidDevice {
    fn send_output_report(&self, data: &[u8]) -> Result<()> {
        // hidapi 2.6's windows-native send_output_report writes the feature-report buffer,
        // which is empty on these devices. write() uses the output report they have.
        if hidapi::HidDevice::write(self, data)? != data.len() {
            return Err(Error::Unsupported("incomplete HID output report".into()));
        }
        Ok(())
    }

    fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        Ok(hidapi::HidDevice::read_timeout(self, buf, timeout_ms)?)
    }

    fn send_feature_report(&self, data: &[u8]) -> Result<()> {
        Ok(hidapi::HidDevice::send_feature_report(self, data)?)
    }

    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize> {
        Ok(hidapi::HidDevice::get_feature_report(self, buf)?)
    }
}
