# Kiyo Pro setup and hardware verification

Support for Razer Kiyo Pro RZ1900364 (`1532:0E05`) is implemented for Windows
10/11 but has not yet been verified on physical hardware. Automated tests and
cross-compilation do not prove that a camera accepts a control or changes its
image. Keep this distinction in the PR and device-support list.

## Build and use

On Windows, install stable Rust with the MSVC build tools, then run:

```powershell
cargo build --release
.\target\release\secbird.exe list
.\target\release\secbird.exe -d 0E05 camera
.\target\release\secbird-tray.exe
```

Close Synapse to prevent competing writes. Use the tray's **Open editor**, select
the Kiyo Pro, then choose **Camera**. No administrator rights or replacement UVC
driver are intended. Allow camera access for desktop apps in Windows privacy
settings if discovery/control access fails.

The controls are HDR on/off, HDR dark/bright mode, wide/medium/narrow field of
view, and responsive/passive autofocus strategy. The autofocus strategy is not
an autofocus enable switch or a manual-focus slider. The exact image/focus
behavior of these vendor modes still needs confirmation on hardware.

The **Image** and **Camera** sections expose standard Windows controls if the
driver reports support. Query with `secbird -d 0E05 camera controls` for each
control's native range, step, current value, and Auto/manual capabilities.
Numeric values use driver units; exposure is commonly logarithmic rather than
milliseconds. A grayed-out control is not writable on that driver. Reset sends
the driver's reported numeric default in manual mode when supported; it is not
a full factory reset. Anti-flicker 50/60 Hz describes the mains frequency of
lighting, not camera FPS. Low-light compensation requires Auto exposure and may
lower FPS in dim light. Standard autofocus Auto is separate from the vendor's
responsive/passive autofocus strategy.

Standard controls have driver readback after writes, unlike the vendor extension
controls. A running preview should reflect accepted changes without restarting,
but other apps can override controls and Auto mode can alter values over time.

Unset profile controls leave the camera unchanged. Vendor extension values shown
in the editor and CLI are saved/requested values, not current-camera readbacks. A
successful vendor write is not evidence that the visible result matches the request.
Profiles reapply explicitly stored controls on startup, reconnect, wake, and
profile changes. Firmware operations and on-device persistent-save commands are
not part of this implementation.

## Preview

Preview is optional and starts only after a user action. Click **Enable preview /
list cameras** to grant video permission, select the source explicitly, then
click **Start preview**. The browser may briefly open its default camera while
obtaining permission to reveal device labels; that temporary stream is released
before source selection. No audio permission is requested.

Browser camera IDs cannot be reliably matched to Windows device paths. Confirm
the physical camera yourself, especially if multiple devices have the same name.
Changing device/page, stopping preview, hiding/closing the editor, or losing the
video track releases the preview. Reconnecting does not restart it automatically.

Resolution/FPS choices apply only to Secretarybird's preview. They do not set
capture formats in OBS, Teams, or other applications. The browser reports ranges
rather than every valid combination, so preset tuples are labeled **try
combination** until accepted. Unsupported requests retain the last working format
or stop preview if it cannot be restored. Check **Actual preview** for the
negotiated format, including after changing HDR.

Preview can contend with other camera apps. If it is busy, stop preview in the
other app or stop Secretarybird's preview. Configuration controls remain
available without video preview, subject to the driver's access rules.

## Multiple cameras and diagnostics

Use `secbird list` to find full camera identities and quote them when passing
`-d`. A bare `0E05` is rejected when more than one camera matches. Profiles are
separate per device path; changing USB ports can change that identity.

`secbird report` and `secbird report --json` include read-only camera diagnostics
with anonymous IDs, not full device paths. Avoid pasting the full `list` output
publicly without checking it: camera identities can contain serial information.

## Automated verification

```text
cargo test --release
cargo clippy --release --all-targets -- -D warnings
node --test crates/secbird-tray/src/*.test.cjs
```

Windows CI must execute the native-path tests, including the KS metadata and
authenticated server-route tests. Linux CI continues checking core/CLI support.
The JavaScript tests exercise video lifecycle and negotiation without a webcam.
A browser smoke check using a synthetic stream exercises rendering but is not
hardware verification.

## Physical test record

Fill this out on Windows before changing the support status to verified. Leave
unperformed checks unchecked and attach failures instead of inferring success.

| Environment | Result |
| --- | --- |
| Commit/build tested | Not tested |
| Windows version/build | Not tested |
| Camera firmware | Not tested; use an independently known value if available |
| Browser/version | Not tested |
| USB connection, hub, port | Not tested; keep serial-bearing paths private |
| Synapse closed | Not tested |

- [ ] Exactly one entry per physical Kiyo Pro; no duplicate HID failure entry.
- [ ] Report/discovery leave the image and persistent settings unchanged.
- [ ] HDR off/on produces the expected visible result.
- [ ] HDR dark/bright modes behave as expected; document the observed difference.
- [ ] Wide/medium/narrow FOV changes framing correctly in both directions.
- [ ] Responsive/passive focus strategies are accepted; document observed behavior.
- [ ] Query standard driver ranges; change brightness/contrast/white balance,
  focus Auto/manual, exposure Auto/manual, and observe the running preview.
- [ ] Confirm unsupported controls stay disabled; check anti-flicker and low-light
  behavior separately from capture FPS.
- [ ] Verify per-control reset where a default exists and explicit profile replay
  after reconnect; compare readback with the saved Auto/manual choice.
- [ ] Standard controls: read driver ranges, apply brightness/contrast/white balance,
  focus Auto/manual, exposure Auto/manual, and see the running preview respond.
- [ ] Confirm disabled/unsupported controls remain read-only; anti-flicker values
  are not confused with video FPS; low-light requires Auto exposure.
- [ ] Test per-control reset when a driver default is available and profile replay
  on reconnect; compare current readback versus the saved Auto/manual choice.
- [ ] Saved profiles switch/reapply correctly, including foreground-app profiles.
- [ ] Restart, reconnect and wake reapply only explicitly stored controls.
- [ ] An existing mouse/keyboard still works alongside the camera.
- [ ] Preview shows the selected physical camera; verify twins if available.
- [ ] Actual preview formats match accepted requests; record tuples below.
- [ ] Unsupported tuples show an error and restore the previous format safely.
- [ ] HDR-related frame-rate restrictions are reported accurately.
- [ ] Stop preview releases camera access for another app.
- [ ] Closing/hiding the editor and switching devices/pages release capture.
- [ ] Stop/switch while a permission request is pending releases any late stream.
- [ ] Disconnect stops preview; reconnect requires explicit preview restart.
- [ ] Access conflict with OBS/Teams gives a clear error and recovers after rescan.
- [ ] Permission denial is actionable and does not prevent HID devices working.

| Requested preview tuple | Actual tuple | HDR state | Result |
| --- | --- | --- | --- |
| Browser default | Not tested | Not tested | Not tested |

If a control is accepted but does not visibly work, report that separately from
discovery or permission failures. Do not mark the camera verified based only on
saved UI values, successful tests, or a successful build.
