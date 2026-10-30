//! When to restart Spectrum on the keyboard and mouse together, so they cycle in step
//! again after one of them wakes. Driven by events only: Windows Raw Input (event headers,
//! no HID traffic), the tray sending a device its profile on its own, and the outcome of
//! each attempt. Nothing here talks to a device, so every decision is tested without one.
//!
//! A wireless device sleeps after a stretch without input, so its first input after a gap
//! that long is a wake. One whose last input is more recent than its sleep timer must still
//! be awake; one quiet for longer may be asleep and isn't written to: it restarts its own
//! lights when it wakes, and that wake syncs everything again. Whichever device wakes first,
//! the write happens once both are awake, however long apart they woke.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Assumed while a device's own sleep timer is unknown: Razer timers start at one minute.
const DEFAULT_IDLE: Duration = Duration::from_secs(55);
/// Taken off a device's own sleep timer, which isn't kept to the second.
const SLEEP_MARGIN: Duration = Duration::from_secs(5);
/// Wait after a wake before writing, for the device to take commands again.
pub const SETTLE: Duration = Duration::from_millis(1_500);
/// Further attempts while a device that woke doesn't take the write; then wait for its next wake.
const RETRY_AFTER: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(4)];

/// What the tray knows about a device it may get input from, refreshed after each rescan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer {
    /// Wired devices never sleep: their input is never a wake and they always count as awake.
    pub wireless: bool,
    /// The sleep timer set in its profile, if any.
    pub sleep: Option<Duration>,
}

impl Peer {
    /// Quiet for longer than this, the device may be asleep.
    fn idle_limit(&self) -> Duration {
        match self.sleep {
            Some(sleep) if sleep > SLEEP_MARGIN * 2 => sleep - SLEEP_MARGIN,
            _ => DEFAULT_IDLE,
        }
    }
}

/// A device's first input after a gap long enough to have slept.
#[derive(Debug, PartialEq, Eq)]
pub struct Wake {
    pub pid: u16,
    /// The gap; `None` for the first input seen from the device.
    pub idle: Option<Duration>,
}

/// One attempt: the devices that woke (or got their profile again), and its number from 1.
#[derive(Debug, PartialEq, Eq)]
pub struct Job {
    pub woke: Vec<u16>,
    pub attempt: usize,
}

/// What to do for a [`Job`], given the devices the tray has right now.
#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    /// Write the Spectrum zones of these devices in this order. The first `strict` ones just
    /// woke: if one of them doesn't take the write, stop there ([`Outcome::NotReady`]), so the
    /// others aren't restarted out of step with it.
    Write { order: Vec<u16>, strict: usize },
    /// Devices that woke but aren't in the tray's list (dropped while asleep): rescan first.
    Missing(Vec<u16>),
    /// Nothing to write now, and why.
    Wait(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Written, or nothing to write: wait for the next wake.
    Done,
    /// A device that woke isn't taking commands yet, or isn't back in the list yet.
    NotReady,
}

#[derive(Default)]
pub struct WakeSync {
    peers: HashMap<u16, Peer>,
    last_input: HashMap<u16, Instant>,
    /// Devices that woke or got their profile again since the last attempt finished, in order.
    woke: Vec<u16>,
    /// Attempts made for `woke`.
    attempts: usize,
}

impl WakeSync {
    pub fn set_peers(&mut self, peers: HashMap<u16, Peer>) {
        self.peers = peers;
    }

    /// Input from device `pid`. `Some` when it is a wake: (re)arm the timer for [`SETTLE`] and
    /// call [`WakeSync::start`] when it fires. Other input only moves a timestamp, so steady
    /// input can't cause a stream of writes.
    pub fn input(&mut self, pid: u16, now: Instant) -> Option<Wake> {
        let peer = *self.peers.get(&pid)?;
        let idle = self.last_input.insert(pid, now).map(|last| now.saturating_duration_since(last));
        if !peer.wireless || idle.is_some_and(|idle| idle < peer.idle_limit()) {
            return None;
        }
        if !self.woke.contains(&pid) {
            self.woke.push(pid);
        }
        // A fresh wake gets every attempt again.
        self.attempts = 0;
        Some(Wake { pid, idle })
    }

    /// The tray sent these devices their profile by itself (they turned up again in a rescan,
    /// or answered after the lights came back), which restarted their Spectrum alone. True
    /// when that makes an attempt due: arm the timer for [`SETTLE`].
    pub fn applied(&mut self, pids: &[u16]) -> bool {
        for pid in pids {
            if !self.woke.contains(pid) {
                self.woke.push(*pid);
            }
        }
        !pids.is_empty()
    }

    /// Whether `pid` must be awake now: wired, or input more recent than its sleep timer.
    pub fn awake(&self, pid: u16, now: Instant) -> bool {
        self.peers.get(&pid).is_some_and(|peer| {
            !peer.wireless
                || self.last_input.get(&pid).is_some_and(|last| now.saturating_duration_since(*last) < peer.idle_limit())
        })
    }

    /// The timer fired: the attempt to make, if one is due.
    pub fn start(&mut self) -> Option<Job> {
        if self.woke.is_empty() {
            return None;
        }
        self.attempts += 1;
        Some(Job { woke: self.woke.clone(), attempt: self.attempts })
    }

    /// What to write for `job`: the devices that woke, then the others on Spectrum that must
    /// still be awake. `present` are the devices in the tray's list, `spectrum` those of them
    /// whose profile in use shows Spectrum.
    pub fn plan(&self, job: &Job, present: &[u16], spectrum: &[u16], now: Instant) -> Plan {
        let missing: Vec<u16> = job.woke.iter().copied().filter(|pid| !present.contains(pid)).collect();
        if !missing.is_empty() {
            return Plan::Missing(missing);
        }
        let mut order: Vec<u16> = job.woke.iter().copied().filter(|pid| spectrum.contains(pid)).collect();
        let strict = order.len();
        if strict == 0 {
            return Plan::Wait(format!("{} doesn't show Spectrum", list(&job.woke)));
        }
        let mut asleep = Vec::new();
        for &pid in spectrum {
            if order.contains(&pid) {
                continue;
            }
            if self.awake(pid, now) {
                order.push(pid);
            } else {
                asleep.push(pid);
            }
        }
        if order.len() >= 2 {
            return Plan::Write { order, strict };
        }
        if !asleep.is_empty() {
            return Plan::Wait(format!("waiting for {} to wake", list(&asleep)));
        }
        // A device dropped from the list while asleep can't be seen on Spectrum yet.
        let mut absent: Vec<u16> = self.peers.keys().copied().filter(|pid| !present.contains(pid)).collect();
        absent.sort_unstable();
        if absent.is_empty() {
            Plan::Wait("no other device shows Spectrum".into())
        } else {
            Plan::Wait(format!("no other device shows Spectrum; {} not in the device list (asleep?)", list(&absent)))
        }
    }

    /// The attempt is over. `Some(delay)`: try again after it. `None`: done until the next wake.
    pub fn finish(&mut self, outcome: Outcome) -> Option<Duration> {
        if outcome == Outcome::NotReady {
            if let Some(delay) = self.attempts.checked_sub(1).and_then(|i| RETRY_AFTER.get(i)) {
                return Some(*delay);
            }
        }
        self.woke.clear();
        self.attempts = 0;
        None
    }

    /// For the log: how long each device has been quiet, and which may be asleep.
    pub fn describe(&self, now: Instant) -> String {
        let mut pids: Vec<u16> = self.peers.keys().copied().collect();
        pids.sort_unstable();
        let parts: Vec<String> = pids
            .iter()
            .map(|&pid| {
                let state = match (self.peers[&pid].wireless, self.last_input.get(&pid)) {
                    (false, _) => "wired".to_string(),
                    (true, None) => "no input yet".to_string(),
                    (true, Some(last)) => {
                        let quiet = now.saturating_duration_since(*last).as_secs();
                        if self.awake(pid, now) {
                            format!("quiet {quiet} s")
                        } else {
                            format!("quiet {quiet} s, may be asleep")
                        }
                    }
                };
                format!("{pid:04X} {state}")
            })
            .collect();
        parts.join(", ")
    }
}

/// Product ids for the log: `02C9, 00EA`.
pub fn list(pids: &[u16]) -> String {
    pids.iter().map(|pid| format!("{pid:04X}")).collect::<Vec<_>>().join(", ")
}

/// The Razer product id in a Raw Input or HID device path, such as
/// `\\?\HID#VID_1532&PID_02C9&MI_02&Col01#...`. The receiver's id for a wireless device.
pub fn razer_pid(path: &str) -> Option<u16> {
    const PREFIX: &str = "VID_1532&PID_";
    let upper = path.to_ascii_uppercase();
    let at = upper.find(PREFIX)? + PREFIX.len();
    let hex = upper.get(at..at + 4)?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u16::from_str_radix(hex, 16).ok()
}

/// The interface and collection in a device path (`MI_02 COL01`): tells a receiver's
/// collections apart in the log without logging the whole path.
pub fn collection(path: &str) -> String {
    let upper = path.to_ascii_uppercase();
    let parts: Vec<&str> = ["MI_", "COL"]
        .into_iter()
        .filter_map(|tag| {
            let at = upper.find(tag)?;
            upper.get(at..at + tag.len() + 2)
        })
        .collect();
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pair this was built for: BlackWidow V4 Low-Profile HyperSpeed and Basilisk V4
    /// HyperSpeed, each on its own receiver, both set to sleep after one minute.
    const BLACKWIDOW: u16 = 0x02C9;
    const BASILISK: u16 = 0x00EA;
    const BOTH: [u16; 2] = [BLACKWIDOW, BASILISK];
    const ONE_MINUTE: Peer = Peer { wireless: true, sleep: Some(Duration::from_secs(60)) };

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn write(order: [u16; 2]) -> Plan {
        Plan::Write { order: order.to_vec(), strict: 1 }
    }

    /// Both used at `t0`, synced, in step from then on.
    fn in_step(t0: Instant) -> WakeSync {
        let mut t = WakeSync::default();
        t.set_peers(HashMap::from([(BLACKWIDOW, ONE_MINUTE), (BASILISK, ONE_MINUTE)]));
        assert!(t.input(BLACKWIDOW, t0).is_some(), "first input seen counts as a wake");
        assert!(t.input(BASILISK, t0).is_some());
        assert_eq!(fire(&mut t, t0 + SETTLE, &BOTH), Plan::Write { order: BOTH.to_vec(), strict: 2 });
        t
    }

    /// `pid` used every 10 s for `span` seconds after `from`, when it was last used.
    fn keep_using(t: &mut WakeSync, pid: u16, from: Instant, span: u64) {
        for s in (10..=span).step_by(10) {
            assert_eq!(t.input(pid, from + secs(s)), None, "steady input is not a wake");
        }
    }

    /// The timer firing with `present` in the list, all on Spectrum, and every write taking.
    fn fire(t: &mut WakeSync, now: Instant, present: &[u16]) -> Plan {
        let job = t.start().expect("an attempt is due");
        let plan = t.plan(&job, present, present, now);
        assert_eq!(t.finish(Outcome::Done), None);
        plan
    }

    /// Both fell asleep. `first` wakes and is used for `used` seconds; `second` wakes `gap`
    /// seconds after `first` did. Returns the plan made for the second wake.
    fn both_slept(first: u16, second: u16, gap: u64, used: u64) -> (WakeSync, Instant, Plan) {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        let woke = t0 + secs(300);
        assert_eq!(t.input(first, woke), Some(Wake { pid: first, idle: Some(secs(300)) }));
        // The other one still sleeps: nothing is written, not even to the one that woke.
        let waiting = Plan::Wait(format!("waiting for {second:04X} to wake"));
        assert_eq!(fire(&mut t, woke + SETTLE, &BOTH), waiting);
        keep_using(&mut t, first, woke, used);
        let second_at = woke + secs(gap);
        assert_eq!(t.input(second, second_at), Some(Wake { pid: second, idle: Some(secs(300 + gap)) }));
        let plan = fire(&mut t, second_at + SETTLE, &BOTH);
        (t, second_at, plan)
    }

    #[test]
    fn both_slept_mouse_then_keyboard_soon_after() {
        assert_eq!(both_slept(BASILISK, BLACKWIDOW, 10, 5).2, write([BLACKWIDOW, BASILISK]));
    }

    #[test]
    fn both_slept_keyboard_then_mouse_soon_after() {
        assert_eq!(both_slept(BLACKWIDOW, BASILISK, 10, 5).2, write([BASILISK, BLACKWIDOW]));
    }

    #[test]
    fn both_slept_mouse_then_keyboard_90_s_later_while_the_mouse_is_in_use() {
        assert_eq!(both_slept(BASILISK, BLACKWIDOW, 90, 85).2, write([BLACKWIDOW, BASILISK]));
    }

    #[test]
    fn both_slept_keyboard_then_mouse_90_s_later_while_typing() {
        assert_eq!(both_slept(BLACKWIDOW, BASILISK, 90, 85).2, write([BASILISK, BLACKWIDOW]));
    }

    /// The first device was left alone and went back to sleep before the second woke: the
    /// sync waits for the first one's next wake instead of writing to a sleeping device.
    fn first_slept_again(first: u16, second: u16) {
        let (mut t, second_at, plan) = both_slept(first, second, 90, 20);
        assert_eq!(plan, Plan::Wait(format!("waiting for {first:04X} to wake")));
        let back = second_at + secs(30);
        assert!(t.input(first, back).is_some(), "100 s quiet: a wake");
        assert_eq!(fire(&mut t, back + SETTLE, &BOTH), write([first, second]));
    }

    #[test]
    fn both_slept_mouse_then_keyboard_after_the_mouse_slept_again() {
        first_slept_again(BASILISK, BLACKWIDOW);
    }

    #[test]
    fn both_slept_keyboard_then_mouse_after_the_keyboard_slept_again() {
        first_slept_again(BLACKWIDOW, BASILISK);
    }

    /// The case that already worked on hardware: one sleeps while the other is in use.
    fn one_slept(sleeper: u16, in_use: u16) {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        keep_using(&mut t, in_use, t0, 300);
        let woke = t0 + secs(305);
        assert!(t.input(sleeper, woke).is_some());
        assert_eq!(fire(&mut t, woke + SETTLE, &BOTH), write([sleeper, in_use]));
    }

    #[test]
    fn mouse_slept_alone() {
        one_slept(BASILISK, BLACKWIDOW);
    }

    #[test]
    fn keyboard_slept_alone() {
        one_slept(BLACKWIDOW, BASILISK);
    }

    /// Seen in the log of 2026-10-29: a rescan while the keyboard slept dropped it from the
    /// list, so the old sync found one device, wrote nothing and forgot the wake.
    #[test]
    fn keyboard_dropped_from_the_list_while_asleep() {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        let mouse_only = [BASILISK];
        assert!(t.input(BASILISK, t0 + secs(300)).is_some());
        let waiting = Plan::Wait("no other device shows Spectrum; 02C9 not in the device list (asleep?)".into());
        assert_eq!(fire(&mut t, t0 + secs(302), &mouse_only), waiting);
        let woke = t0 + secs(312);
        assert!(t.input(BLACKWIDOW, woke).is_some());
        let job = t.start().unwrap();
        let now = woke + SETTLE;
        assert_eq!(t.plan(&job, &mouse_only, &mouse_only, now), Plan::Missing(vec![BLACKWIDOW]));
        // After the rescan the tray finds it again.
        assert_eq!(t.plan(&job, &BOTH, &BOTH, now), write([BLACKWIDOW, BASILISK]));
        assert_eq!(t.finish(Outcome::Done), None);
    }

    /// Seen on 2026-10-29: the timers went from 1 to 7 minutes in the editor; the attempt
    /// reads them again, so a keyboard quiet for 99 s counts as awake.
    #[test]
    fn sleep_timers_changed_in_the_editor_count_at_the_next_attempt() {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        assert!(t.input(BASILISK, t0 + secs(99)).is_some());
        let job = t.start().unwrap();
        let now = t0 + secs(100);
        assert_eq!(t.plan(&job, &BOTH, &BOTH, now), Plan::Wait("waiting for 02C9 to wake".into()));
        let seven = Peer { wireless: true, sleep: Some(secs(7 * 60)) };
        t.set_peers(HashMap::from([(BLACKWIDOW, seven), (BASILISK, ONE_MINUTE)]));
        assert_eq!(t.plan(&job, &BOTH, &BOTH, now), write([BASILISK, BLACKWIDOW]));
    }

    #[test]
    fn a_device_that_doesnt_take_the_write_gets_two_more_tries_then_waits() {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        keep_using(&mut t, BLACKWIDOW, t0, 300);
        let woke = t0 + secs(305);
        assert!(t.input(BASILISK, woke).is_some());
        for (attempt, retry) in [(1, Some(secs(2))), (2, Some(secs(4))), (3, None)] {
            let job = t.start().unwrap();
            assert_eq!(job, Job { woke: vec![BASILISK], attempt });
            assert_eq!(t.finish(Outcome::NotReady), retry);
        }
        assert_eq!(t.start(), None, "given up until the next wake");
        assert_eq!(t.input(BASILISK, woke + secs(20)), None, "still the same wake");
        assert!(t.input(BASILISK, woke + secs(90)).is_some());
        assert_eq!(t.start().unwrap().attempt, 1);
    }

    #[test]
    fn a_new_wake_during_retries_starts_the_count_again() {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        assert!(t.input(BASILISK, t0 + secs(300)).is_some());
        t.start();
        assert_eq!(t.finish(Outcome::NotReady), Some(secs(2)));
        assert!(t.input(BLACKWIDOW, t0 + secs(301)).is_some());
        assert_eq!(t.start(), Some(Job { woke: vec![BASILISK, BLACKWIDOW], attempt: 1 }));
    }

    #[test]
    fn steady_input_never_writes_again() {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        for tenth in 1..=6_000 {
            let now = t0 + Duration::from_millis(100 * tenth);
            assert_eq!(t.input(BLACKWIDOW, now), None);
            assert_eq!(t.input(BASILISK, now), None);
        }
        assert_eq!(t.start(), None);
    }

    #[test]
    fn a_profile_sent_again_syncs_with_the_awake_device() {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        keep_using(&mut t, BLACKWIDOW, t0, 250);
        assert!(t.applied(&[BASILISK]));
        assert_eq!(fire(&mut t, t0 + secs(256), &BOTH), write([BASILISK, BLACKWIDOW]));
        assert!(!t.applied(&[]));
        assert_eq!(t.start(), None);
    }

    #[test]
    fn wired_devices_never_wake_and_always_count_as_awake() {
        const WIRED: u16 = 0x02CC;
        let mut t = WakeSync::default();
        t.set_peers(HashMap::from([(BASILISK, ONE_MINUTE), (WIRED, Peer { wireless: false, sleep: None })]));
        let t0 = Instant::now();
        assert_eq!(t.input(WIRED, t0), None);
        assert_eq!(t.input(WIRED, t0 + secs(600)), None);
        assert!(t.input(BASILISK, t0 + secs(900)).is_some());
        assert_eq!(fire(&mut t, t0 + secs(902), &[BASILISK, WIRED]), write([BASILISK, WIRED]));
    }

    #[test]
    fn the_sleep_timer_from_the_profile_sets_what_counts_as_a_wake() {
        let mut t = WakeSync::default();
        t.set_peers(HashMap::from([(BASILISK, Peer { wireless: true, sleep: Some(secs(300)) })]));
        let t0 = Instant::now();
        t.input(BASILISK, t0);
        assert_eq!(t.input(BASILISK, t0 + secs(70)), None, "a pause, not a sleep");
        assert!(t.awake(BASILISK, t0 + secs(300)));
        assert!(!t.awake(BASILISK, t0 + secs(370)));
        assert!(t.input(BASILISK, t0 + secs(370)).is_some());
        let mut unknown = WakeSync::default();
        unknown.set_peers(HashMap::from([(BASILISK, Peer { wireless: true, sleep: None })]));
        unknown.input(BASILISK, t0);
        assert!(unknown.input(BASILISK, t0 + secs(55)).is_some(), "Razer timers start at one minute");
    }

    #[test]
    fn devices_the_tray_doesnt_know_are_ignored() {
        let mut t = WakeSync::default();
        let now = Instant::now();
        assert_eq!(t.input(BLACKWIDOW, now), None, "no device list yet");
        t.set_peers(HashMap::from([(BLACKWIDOW, ONE_MINUTE)]));
        assert_eq!(t.input(0x1234, now), None);
        assert_eq!(t.start(), None);
    }

    #[test]
    fn a_device_without_spectrum_writes_nothing() {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        assert!(t.input(BASILISK, t0 + secs(300)).is_some());
        let job = t.start().unwrap();
        let plan = t.plan(&job, &BOTH, &[BLACKWIDOW], t0 + secs(302));
        assert_eq!(plan, Plan::Wait("00EA doesn't show Spectrum".into()));
    }

    #[test]
    fn the_log_line_says_who_may_be_asleep() {
        let t0 = Instant::now();
        let mut t = in_step(t0);
        assert_eq!(t.describe(t0 + secs(5)), "00EA quiet 5 s, 02C9 quiet 5 s");
        t.input(BASILISK, t0 + secs(100));
        assert_eq!(t.describe(t0 + secs(101)), "00EA quiet 1 s, 02C9 quiet 101 s, may be asleep");
    }

    #[test]
    fn raw_input_paths_name_the_receiver() {
        let keyboard = r"\\?\HID#VID_1532&PID_02C9&MI_02&Col01#8&2a1b3c4d&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91e6bf6}";
        assert_eq!(razer_pid(keyboard), Some(BLACKWIDOW));
        assert_eq!(collection(keyboard), "MI_02 COL01");
        let mouse = r"\\?\hid#vid_1532&pid_00ea&mi_00#7&1f2e3d4c&0&0000#{378de44c-56ef-11d1-bc8c-00a0c91e6bf6}";
        assert_eq!(razer_pid(mouse), Some(BASILISK));
        assert_eq!(collection(mouse), "MI_00");
        assert_eq!(razer_pid(r"\\?\HID#VID_046D&PID_C52B&MI_00#x"), None);
        assert_eq!(razer_pid("VID_1532&PID_02"), None);
        assert_eq!(razer_pid("VID_1532&PID_+2C9"), None);
        assert_eq!(collection("VID_1532&PID_02C9"), "");
        assert_eq!(list(&BOTH), "02C9, 00EA");
    }
}
