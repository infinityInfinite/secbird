//! Hardware on-board profile slots (class `05:*`, from Synapse 4's `obmEngine`).
//!
//! Mice with on-board memory keep up to 5 hardware profile slots (1-based). Out of the box
//! only slot 1 exists, so the mouse's underside profile button has nothing to cycle to until
//! additional slots are created with `05:02`. Each slot stores its own 250-byte metadata
//! header (GUID, UTF-8 name, owner, hash) via `05:08`/`05:88` and its own button assignments
//! via `02:0C` with `args[0] = slot`.

use crate::commands as cmd;
use crate::device::Device;
use crate::error::{Error, Result};
use crate::features::Rgb;
use serde::Serialize;

/// One on-board profile slot stored in the device's flash memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnboardSlot {
    /// 1-based slot number (1..=5).
    pub id: u8,
    /// UTF-8 name from the slot's 250-byte header (empty if unnamed or unreadable).
    pub name: String,
    /// Whether this slot is currently active on the mouse.
    pub active: bool,
    /// Indicator LED color, when the device reports `05:8C`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

fn check_slot(dev: &Device, slot: u8) -> Result<()> {
    let max = dev.max_onboard_slots();
    if !(1..=max).contains(&slot) {
        return Err(Error::InvalidArg(format!("on-board slot {slot}: use 1 to {max}")));
    }
    Ok(())
}

/// 250-byte profile header: 16-byte GUID, 100-byte UTF-8 name, 128-byte owner, 6-byte hash
/// (matching Synapse 4's `obmEngine.setProfileNameStructure`).
fn encode_header(name: &str) -> Vec<u8> {
    let mut h = vec![0u8; cmd::PROFILE_HEADER_LEN];
    h[..16].copy_from_slice(&guid());
    let mut end = name.len().min(99);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    h[16..16 + end].copy_from_slice(&name.as_bytes()[..end]);
    h
}

fn decode_name(header: &[u8]) -> String {
    if header.len() < 116 {
        return String::new();
    }
    let name = &header[16..116];
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    String::from_utf8_lossy(&name[..end]).trim().to_string()
}

fn guid() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 16];
    for (i, half) in out.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_usize(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        half.copy_from_slice(&h.finish().to_le_bytes());
    }
    out
}

impl Device {
    /// Maximum number of on-board profile slots this device supports (default 5).
    pub fn max_onboard_slots(&self) -> u8 {
        match self.spec.as_ref().map(|s| s.onboard_profiles).unwrap_or(0) {
            0 => cmd::MAX_ONBOARD_SLOTS,
            n => n.min(cmd::MAX_ONBOARD_SLOTS),
        }
    }

    /// Active on-board profile slot (`05:84`), 1-based.
    pub fn active_onboard_slot(&self) -> Result<u8> {
        let r = self.get(cmd::ACTIVE_SLOT, cmd::PROFILE_SLOT_SIZE, &[])?;
        let slot = r.args()[0];
        if slot == 0 {
            return Err(Error::Unsupported("device reported no active on-board slot".into()));
        }
        Ok(slot)
    }

    /// Switch the active on-board profile slot (`05:04`).
    pub fn set_active_onboard_slot(&self, slot: u8) -> Result<()> {
        check_slot(self, slot)?;
        self.transact_slow(cmd::ACTIVE_SLOT, cmd::PROFILE_SLOT_SIZE, &[slot])?;
        Ok(())
    }

    /// Number of on-board profile slots currently created on the device (`05:80`).
    pub fn onboard_slot_count(&self) -> Result<u8> {
        let r = self.get(cmd::PROFILE_COUNT, cmd::PROFILE_COUNT_SIZE, &[])?;
        Ok(r.args()[0])
    }

    /// Sorted list of on-board profile slot ids currently created on the device (`05:81`).
    pub fn onboard_slot_ids(&self) -> Result<Vec<u8>> {
        let r = self.get(cmd::PROFILE_IDS, cmd::PROFILE_IDS_SIZE, &[])?;
        let a = r.args();
        let count = (a[0] as usize).min(a.len().saturating_sub(1));
        let mut ids: Vec<u8> = a[1..1 + count]
            .iter()
            .copied()
            .filter(|&id| (1..=cmd::MAX_ONBOARD_SLOTS).contains(&id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }

    /// Read the 250-byte profile header for `slot` (`05:88`) and return its UTF-8 name.
    pub fn onboard_slot_name(&self, slot: u8) -> Result<String> {
        check_slot(self, slot)?;
        let mut out = Vec::with_capacity(cmd::PROFILE_HEADER_LEN);
        while out.len() < cmd::PROFILE_HEADER_LEN {
            let n = (cmd::PROFILE_HEADER_LEN - out.len()).min(cmd::PROFILE_NAME_CHUNK);
            let [oh, ol] = (out.len() as u16).to_be_bytes();
            let r = self.transact_slow(cmd::PROFILE_NAME.get(), (n + 5) as u8, &[slot, oh, ol, 0, 0])?;
            out.extend_from_slice(&r.args()[5..5 + n]);
        }
        Ok(decode_name(&out))
    }

    /// Write the 250-byte profile header for `slot` (`05:08`) with the given UTF-8 name.
    pub fn set_onboard_slot_name(&self, slot: u8, name: &str) -> Result<()> {
        check_slot(self, slot)?;
        let head = encode_header(name.trim());
        let [th, tl] = (cmd::PROFILE_HEADER_LEN as u16).to_be_bytes();
        for (i, chunk) in head.chunks(cmd::PROFILE_NAME_CHUNK).enumerate() {
            let [oh, ol] = ((i * cmd::PROFILE_NAME_CHUNK) as u16).to_be_bytes();
            let mut args = Vec::with_capacity(5 + chunk.len());
            args.extend_from_slice(&[slot, oh, ol, th, tl]);
            args.extend_from_slice(chunk);
            self.transact_slow(cmd::PROFILE_NAME, args.len() as u8, &args)?;
        }
        Ok(())
    }

    /// Indicator LED color of on-board slot `slot` (`05:8C`).
    pub fn onboard_slot_color(&self, slot: u8) -> Result<Rgb> {
        check_slot(self, slot)?;
        let r = self.get(cmd::PROFILE_COLOR, cmd::PROFILE_COLOR_SIZE, &[slot])?;
        let a = r.args();
        Ok(Rgb(a[1], a[2], a[3]))
    }

    /// Set the indicator LED color of on-board slot `slot` (`05:0C`).
    pub fn set_onboard_slot_color(&self, slot: u8, color: Rgb) -> Result<()> {
        check_slot(self, slot)?;
        self.transact_slow(cmd::PROFILE_COLOR, cmd::PROFILE_COLOR_SIZE, &[slot, color.0, color.1, color.2])?;
        Ok(())
    }

    /// Every on-board profile slot currently on the device, with its name and active state.
    pub fn onboard_slots(&self) -> Result<Vec<OnboardSlot>> {
        let ids = self.onboard_slot_ids()?;
        let active = self.active_onboard_slot().unwrap_or(1);
        Ok(ids
            .into_iter()
            .map(|id| OnboardSlot {
                id,
                name: self.onboard_slot_name(id).unwrap_or_default(),
                active: id == active,
                color: self.onboard_slot_color(id).ok().map(|c| c.to_string()),
            })
            .collect())
    }

    /// Create a new on-board profile slot (`05:02`) and optionally write its name (`05:08`).
    pub fn create_onboard_slot(&self, slot: u8, name: Option<&str>) -> Result<()> {
        check_slot(self, slot)?;
        self.transact_slow(cmd::PROFILE_CREATE, cmd::PROFILE_SLOT_SIZE, &[slot])?;
        if let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) {
            // Some firmware builds don't store profile names; keep the created slot anyway.
            if let Err(Error::Device { .. }) = self.set_onboard_slot_name(slot, name) {}
        }
        Ok(())
    }

    /// Delete an on-board profile slot (`05:03`). Refuses to delete the last remaining slot.
    /// If `slot` is currently active, switches to another existing slot first.
    pub fn delete_onboard_slot(&self, slot: u8) -> Result<()> {
        check_slot(self, slot)?;
        let ids = self.onboard_slot_ids()?;
        if !ids.contains(&slot) {
            return Err(Error::NotFound(format!("on-board slot {slot}")));
        }
        if ids.len() <= 1 {
            return Err(Error::InvalidArg("cannot delete the last on-board profile slot".into()));
        }
        if self.active_onboard_slot().ok() == Some(slot) {
            if let Some(&fallback) = ids.iter().find(|&&id| id != slot) {
                self.set_active_onboard_slot(fallback)?;
            }
        }
        self.transact_slow(cmd::PROFILE_DELETE, cmd::PROFILE_SLOT_SIZE, &[slot])?;
        Ok(())
    }

    /// Reset an on-board profile slot to factory defaults (`05:0B`).
    pub fn reset_onboard_slot(&self, slot: u8) -> Result<()> {
        check_slot(self, slot)?;
        self.transact_slow(cmd::PROFILE_RESET, cmd::PROFILE_SLOT_SIZE, &[slot])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_header_round_trip() {
        let h = encode_header("MMO Raid");
        assert_eq!(h.len(), cmd::PROFILE_HEADER_LEN);
        assert_eq!(decode_name(&h), "MMO Raid");
    }

    #[test]
    fn long_utf8_name_truncates_on_char_boundary() {
        let long = "é".repeat(60); // 120 bytes > 99
        let h = encode_header(&long);
        let decoded = decode_name(&h);
        assert!(decoded.len() <= 99);
        assert!(decoded.chars().all(|c| c == 'é'));
    }
}
