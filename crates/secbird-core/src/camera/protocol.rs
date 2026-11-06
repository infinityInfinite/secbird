//! Independently encoded protocol facts from the public Kiyo Pro controls reference.

use super::{AutofocusMode, FieldOfView, HdrMode, Setting};
use crate::Result;

fn commands(setting: Setting) -> Vec<[u8; 8]> {
    let (register, value) = match setting {
        Setting::Hdr(on) => (2, u8::from(on)),
        Setting::HdrMode(mode) => (7, u8::from(mode == HdrMode::Bright)),
        Setting::AutofocusMode(mode) => (6, u8::from(mode == AutofocusMode::Passive)),
        Setting::FieldOfView(fov) => {
            let index = match fov {
                FieldOfView::Wide => 0,
                FieldOfView::Medium => 1,
                FieldOfView::Narrow => 2,
            };
            let mut out = vec![[0xff, 1, 0, 3, index, 0, 0, 0]];
            if index != 0 {
                out.push([0xff, 1, 1, 3, index, 0, 0, 0]);
            }
            return out;
        }
    };
    vec![[0xff, register, value, 0, 0, 0, 0, 0]]
}

pub(super) fn send_setting(
    setting: Setting,
    mut send: impl FnMut(&[u8; 8]) -> Result<()>,
) -> Result<()> {
    for command in commands(setting) {
        send(&command)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{AutofocusMode as Af, FieldOfView as Fov, HdrMode, Setting};

    #[test]
    fn camera_payloads_match_documented_registers() {
        for (setting, expected) in [
            (Setting::Hdr(false), vec![[0xff, 2, 0, 0, 0, 0, 0, 0]]),
            (Setting::Hdr(true), vec![[0xff, 2, 1, 0, 0, 0, 0, 0]]),
            (
                Setting::HdrMode(HdrMode::Dark),
                vec![[0xff, 7, 0, 0, 0, 0, 0, 0]],
            ),
            (
                Setting::HdrMode(HdrMode::Bright),
                vec![[0xff, 7, 1, 0, 0, 0, 0, 0]],
            ),
            (
                Setting::AutofocusMode(Af::Responsive),
                vec![[0xff, 6, 0, 0, 0, 0, 0, 0]],
            ),
            (
                Setting::AutofocusMode(Af::Passive),
                vec![[0xff, 6, 1, 0, 0, 0, 0, 0]],
            ),
            (
                Setting::FieldOfView(Fov::Wide),
                vec![[0xff, 1, 0, 3, 0, 0, 0, 0]],
            ),
            (
                Setting::FieldOfView(Fov::Medium),
                vec![[0xff, 1, 0, 3, 1, 0, 0, 0], [0xff, 1, 1, 3, 1, 0, 0, 0]],
            ),
            (
                Setting::FieldOfView(Fov::Narrow),
                vec![[0xff, 1, 0, 3, 2, 0, 0, 0], [0xff, 1, 1, 3, 2, 0, 0, 0]],
            ),
        ] {
            assert_eq!(commands(setting), expected);
        }
    }

    #[test]
    fn camera_sequence_aborts_on_first_failed_write() {
        let mut writes = Vec::new();
        let result = send_setting(Setting::FieldOfView(Fov::Narrow), |command| {
            writes.push(*command);
            Err(crate::Error::System("disconnected".into()))
        });
        assert!(result.is_err());
        assert_eq!(writes, vec![[0xff, 1, 0, 3, 2, 0, 0, 0]]);
    }
}
