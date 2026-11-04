//! Just enough of a HID report descriptor to find a device's feature reports.
//!
//! Over USB, Razer's 90-byte feature report has no report id. Over Bluetooth the
//! descriptor comes from the GATT report map, which numbers every report, and Windows
//! refuses a feature report sent under an id the collection doesn't declare (#10, #51).

/// Feature reports declared in `desc`: (report id, length in bytes, id byte not counted).
/// Report id 0 means the descriptor uses no ids.
pub fn feature_reports(desc: &[u8]) -> Vec<(u8, usize)> {
    #[derive(Clone, Copy, Default)]
    struct Globals {
        size: u32,
        count: u32,
        id: u8,
    }
    let mut g = Globals::default();
    let mut stack = Vec::new();
    let mut bits: Vec<(u8, u32)> = Vec::new();
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            // Long item: size, tag, data. None of them matter here.
            let len = desc.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + len;
            continue;
        }
        let len = [0, 1, 2, 4][(prefix & 0x03) as usize];
        let Some(data) = desc.get(i + 1..i + 1 + len) else { break };
        let value = data.iter().rev().fold(0u32, |v, &b| v << 8 | b as u32);
        match prefix & 0xFC {
            0x74 => g.size = value,        // Report Size
            0x94 => g.count = value,       // Report Count
            0x84 => g.id = value as u8,    // Report ID
            0xA4 => stack.push(g),         // Push
            0xB4 => g = stack.pop().unwrap_or(g), // Pop
            0xB0 => {
                // Feature
                let add = g.size * g.count;
                match bits.iter_mut().find(|b| b.0 == g.id) {
                    Some(b) => b.1 += add,
                    None => bits.push((g.id, add)),
                }
            }
            _ => {}
        }
        i += 1 + len;
    }
    bits.into_iter().map(|(id, b)| (id, b.div_ceil(8) as usize)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_razer_report_has_no_id() {
        // Vendor collection as a wired Razer mouse declares it: 90 bytes, no report id.
        let desc = [
            0x06, 0x00, 0xFF, 0x09, 0x02, 0xA1, 0x01, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08,
            0x95, 0x5A, 0x09, 0x02, 0xB1, 0x02, 0xC0,
        ];
        assert_eq!(feature_reports(&desc), [(0, 90)]);
    }

    #[test]
    fn numbered_reports_are_counted_per_id() {
        let desc = [
            0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, // mouse
            0x85, 0x01, 0x75, 0x08, 0x95, 0x04, 0x81, 0x02, // input report 1: not a feature
            0x85, 0x05, 0x75, 0x08, 0x95, 0x5A, 0xB1, 0x02, // feature report 5: 90 bytes
            0xA4, 0x85, 0x06, 0x75, 0x01, 0x95, 0x08, 0xB1, 0x02, 0xB4, // feature 6: 8 bits
            0x95, 0x01, 0x75, 0x08, 0xB1, 0x02, // back to id 5 after Pop: one more byte
            0xC0,
        ];
        assert_eq!(feature_reports(&desc), [(5, 91), (6, 1)]);
    }

    #[test]
    fn truncated_descriptor_does_not_panic() {
        assert!(feature_reports(&[0x75]).is_empty());
        assert!(feature_reports(&[0xFE, 0x40]).is_empty());
    }
}
