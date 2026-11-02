//! Key remaps done on the PC for a keypad whose own keymap is off.
//!
//! The Tartarus V2 Pro keeps remaps on board (see [`crate::buttons`]), but only uses them in
//! normal mode. With the stick as keys it has to be in driver mode, where it types its factory
//! keys and Synapse remaps in software. Windows doesn't say which keyboard a keystroke came
//! from, but in driver mode the keypad also reports every key it has down, with its key id,
//! in report 0x0B (`[id, value hi, value lo]` entries, value 0-65535 for how far the key is
//! pushed; the stick is ids FB and FC). So a keystroke whose key the keypad has down right now
//! is the keypad's, and is swapped for the remap; the same key from another keyboard, with
//! the keypad's key up, is left alone.
//!
//! The thumb buttons and the stick press type nothing at all in driver mode: they come as
//! report 04 `[Razer key]` (01, 67, 68, 69; `04 00` when let go), so a remapped one is simply
//! pressed here.
//!
//! This module only decides; the tray's keyboard hook does the swallowing and sending.

use crate::buttons::{parse_setting_key, Action, Input, Layer, KEYPAD_THUMBS, KEYS};
use crate::stick::usage_scan_code;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

/// A key the keypad stopped reporting this recently still counts as down: its keystroke and
/// its report don't arrive in a fixed order.
pub const RECENT: Duration = Duration::from_millis(60);
/// How long the hook may wait for the keypad's report when a keystroke comes first.
pub const WAIT: Duration = Duration::from_millis(25);

/// Key ids the keypad has down in one report 0x0B, or `None` for any other report.
pub fn report_keys(report: &[u8]) -> Option<Vec<u8>> {
    let (&id, entries) = report.split_first()?;
    if id != 0x0B {
        return None;
    }
    Some(
        entries
            .chunks_exact(3)
            .filter(|e| e[0] != 0 && e[0] != 0xFB && e[0] != 0xFC && (e[1] | e[2]) != 0)
            .map(|e| e[0])
            .collect(),
    )
}

/// Razer keys the keypad has down in one report 04 (its thumb buttons in driver mode), or
/// `None` for any other report.
pub fn report_razer_keys(report: &[u8]) -> Option<Vec<u8>> {
    let (&id, keys) = report.split_first()?;
    (id == 0x04).then(|| keys.iter().copied().filter(|&k| k != 0).collect())
}

/// The remaps to do: scan code the keypad's key types out of the box -> (key id, scan code
/// to send instead), and Razer key of a thumb input -> scan code to send. Scan codes carry
/// 0xE0 in the high byte for extended keys.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Remaps(HashMap<u16, (u8, u16)>, HashMap<u8, u16>);

impl Remaps {
    /// From a profile's button remaps (`"key-5": "key:g"`): single keys on the normal layer,
    /// the ones the keypad can hold on board too. Anything else is left out.
    pub fn from_buttons(buttons: &BTreeMap<String, String>) -> Self {
        let (mut map, mut thumbs) = (HashMap::new(), HashMap::new());
        for (setting, action) in buttons {
            let Ok((Input::Key(id), Layer::Normal)) = parse_setting_key(setting) else { continue };
            let Ok(Action::Key { modifiers: 0, usage }) = action.parse::<Action>() else { continue };
            if let Some(thumb) = KEYPAD_THUMBS.iter().find(|k| k.0 == id) {
                if let Some(to) = usage_scan_code(usage) {
                    thumbs.insert(thumb.3, to);
                }
                continue;
            }
            let Some(factory) = KEYS.iter().find(|k| k.0 == id).map(|k| k.3) else { continue };
            if let (Some(from), Some(to)) = (usage_scan_code(factory), usage_scan_code(usage)) {
                if from != to {
                    map.insert(from, (id, to));
                }
            }
        }
        Remaps(map, thumbs)
    }

    /// Nothing for the keyboard hook to do (thumb remaps don't need it).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, scan: u16) -> Option<(u8, u16)> {
        self.0.get(&scan).copied()
    }
}

/// What to do with one keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Not the keypad's, or not remapped: let it through.
    Pass,
    /// Swallow it and send this scan code (down or up) instead.
    Replace(u16, bool),
}

/// Matches keystrokes with the keypad's reports.
#[derive(Debug, Default)]
pub struct Matcher {
    remaps: Remaps,
    /// Key id -> when the keypad last reported it down.
    seen: HashMap<u8, Instant>,
    /// Scan codes swallowed on the way down -> what was sent instead, so the key's repeats
    /// and its release are swapped too, even after the keypad stopped reporting it.
    held: HashMap<u16, u16>,
    /// Thumb inputs down (Razer key -> scan code sent down for it).
    thumbs_down: HashMap<u8, u16>,
}

impl Matcher {
    /// New remaps (profile switch, an edit). Keys held under the old ones are let go by
    /// the caller with [`Matcher::release_all`] first.
    pub fn set_remaps(&mut self, remaps: Remaps) {
        self.remaps = remaps;
    }

    pub fn remaps(&self) -> &Remaps {
        &self.remaps
    }

    /// The keypad reported these keys down.
    pub fn report(&mut self, keys: &[u8], now: Instant) {
        for &id in keys {
            self.seen.insert(id, now);
        }
    }

    fn keypad_has(&self, id: u8, now: Instant) -> bool {
        self.seen.get(&id).is_some_and(|&t| now.saturating_duration_since(t) <= RECENT)
    }

    /// A key-down that is remapped but not (yet) seen on the keypad: worth waiting a moment
    /// for the keypad's report before deciding.
    pub fn worth_waiting(&self, scan: u16, down: bool, now: Instant) -> bool {
        down && !self.held.contains_key(&scan) && self.remaps.get(scan).is_some_and(|(id, _)| !self.keypad_has(id, now))
    }

    /// Decide about one keystroke from a real keyboard (not one we sent).
    pub fn key(&mut self, scan: u16, down: bool, now: Instant) -> Decision {
        if !down {
            return match self.held.remove(&scan) {
                Some(to) => Decision::Replace(to, false),
                None => Decision::Pass,
            };
        }
        if let Some(&to) = self.held.get(&scan) {
            return Decision::Replace(to, true); // the key repeating while held
        }
        match self.remaps.get(scan) {
            Some((id, to)) if self.keypad_has(id, now) => {
                self.held.insert(scan, to);
                Decision::Replace(to, true)
            }
            _ => Decision::Pass,
        }
    }

    /// The keypad's report 04: its thumb inputs down now. Returns the keys to press and let
    /// go, `(scan code, down)`.
    pub fn razer_keys(&mut self, down: &[u8]) -> Vec<(u16, bool)> {
        let mut out = Vec::new();
        let released: Vec<u8> = self.thumbs_down.keys().copied().filter(|k| !down.contains(k)).collect();
        for k in released {
            if let Some(code) = self.thumbs_down.remove(&k) {
                out.push((code, false));
            }
        }
        for &k in down {
            if self.thumbs_down.contains_key(&k) {
                continue;
            }
            if let Some(&code) = self.remaps.1.get(&k) {
                self.thumbs_down.insert(k, code);
                out.push((code, true));
            }
        }
        out
    }

    /// Scan codes to release: everything sent down and not yet released.
    pub fn release_all(&mut self) -> Vec<u16> {
        let thumbs: Vec<u16> = self.thumbs_down.drain().map(|(_, code)| code).collect();
        self.held.drain().map(|(_, to)| to).chain(thumbs).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIVE: u16 = 0x06; // scan code of 5
    const G: u16 = 0x22;

    fn remaps() -> Remaps {
        let mut b = BTreeMap::new();
        b.insert("key-5".to_string(), "key:g".to_string());
        b.insert("key-s/hypershift".to_string(), "key:x".to_string()); // not the normal layer
        b.insert("key-a".to_string(), "key:ctrl+c".to_string()); // a shortcut
        b.insert("key-d".to_string(), "key:d".to_string()); // no change
        Remaps::from_buttons(&b)
    }

    #[test]
    fn reports_from_the_keypad() {
        // Recorded on the keypad (listen keys): key 6 pushed, then nothing down.
        assert_eq!(report_keys(&[0x0B, 0x06, 0x06, 0xC6]), Some(vec![6]));
        assert_eq!(report_keys(&[0x0B]), Some(vec![]));
        assert_eq!(report_keys(&[0x0B, 0xFB, 0x10, 0x00, 0x20, 0x1D, 0x5F, 0, 0, 0]), Some(vec![32]), "stick and padding left out");
        assert_eq!(report_keys(&[0x04, 0x01]), None);
        // Recorded with Synapse running: top thumb button, then let go; the stick press.
        assert_eq!(report_razer_keys(&[0x04, 0x01, 0, 0]), Some(vec![1]));
        assert_eq!(report_razer_keys(&[0x04, 0, 0]), Some(vec![]));
        assert_eq!(report_razer_keys(&[0x04, 0x69]), Some(vec![0x69]));
        assert_eq!(report_razer_keys(&[0x0B, 0x06, 0x06, 0xC6]), None);
    }

    #[test]
    fn thumb_inputs_press_their_remap() {
        let mut b = BTreeMap::new();
        b.insert("key-stick-press".to_string(), "key:2".to_string());
        b.insert("key-thumb-middle".to_string(), "key:c".to_string());
        b.insert("key-thumb-top".to_string(), "hypershift".to_string()); // its default: nothing to do
        let r = Remaps::from_buttons(&b);
        assert!(r.is_empty(), "the keyboard hook isn't needed for thumbs");
        let mut m = Matcher::default();
        m.set_remaps(r);
        assert_eq!(m.razer_keys(&[0x69]), vec![(0x03, true)], "stick press types 2");
        assert_eq!(m.razer_keys(&[0x69]), vec![], "still down: nothing new");
        assert_eq!(m.razer_keys(&[0x69, 0x67]), vec![(0x2E, true)]);
        assert_eq!(m.razer_keys(&[0x67]), vec![(0x03, false)]);
        assert_eq!(m.razer_keys(&[0x01]), vec![(0x2E, false)], "top button isn't remapped to a key");
        assert_eq!(m.razer_keys(&[0x69]), vec![(0x03, true)]);
        assert_eq!(m.release_all(), vec![0x03]);
        assert_eq!(m.razer_keys(&[]), vec![]);
    }

    #[test]
    fn only_plain_keys_on_the_normal_layer() {
        let r = remaps();
        assert_eq!(r.get(FIVE), Some((6, G)));
        assert_eq!(r.0.len(), 1, "{r:?}");
    }

    #[test]
    fn the_keypads_key_is_swapped_and_others_pass() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut m = Matcher::default();
        m.set_remaps(remaps());

        // 5 from the main keyboard, keypad idle: untouched.
        assert!(m.worth_waiting(FIVE, true, ms(0)));
        assert_eq!(m.key(FIVE, true, ms(25)), Decision::Pass);
        assert_eq!(m.key(FIVE, false, ms(80)), Decision::Pass);

        // Key 6 on the keypad: G down, repeats as G, and G up even once the report stopped.
        m.report(&[6], ms(1000));
        assert!(!m.worth_waiting(FIVE, true, ms(1001)));
        assert_eq!(m.key(FIVE, true, ms(1001)), Decision::Replace(G, true));
        assert_eq!(m.key(FIVE, true, ms(1500)), Decision::Replace(G, true));
        assert_eq!(m.key(FIVE, false, ms(3000)), Decision::Replace(G, false));

        // A while after the keypad let go, the main keyboard's 5 is its own again.
        assert_eq!(m.key(FIVE, true, ms(3200)), Decision::Pass);
        assert_eq!(m.key(FIVE, false, ms(3300)), Decision::Pass);

        // Keys not remapped always pass, keypad or not.
        m.report(&[32], ms(4000));
        assert_eq!(m.key(0x1F, true, ms(4000)), Decision::Pass);

        // Remaps change while G is held: it is let go.
        m.report(&[6], ms(5000));
        assert_eq!(m.key(FIVE, true, ms(5000)), Decision::Replace(G, true));
        assert_eq!(m.release_all(), vec![G]);
        m.set_remaps(Remaps::default());
        assert_eq!(m.key(FIVE, false, ms(5100)), Decision::Pass);
    }
}
