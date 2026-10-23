# Proposed PR: Add Kiyo Pro camera controls to Secretarybird

The Razer Kiyo Pro (`1532:0E05`, RZ19-0364) is a UVC webcam, not a Protocol 2.5
HID peripheral. Secretarybird's HID-only discovery could not recognize or
configure it. This branch adds camera discovery and control inside the existing
Rust core and tray, without changing the HID transport or adding a service or
driver.

## What changes

- Discover the Kiyo Pro through Windows video capture enumeration, identify each
  physical camera separately, and access its UVC extension unit on a dedicated
  COM worker. HDR, dark/bright HDR mode, wide/medium/narrow field of view, and
  responsive/passive autofocus strategy use the documented Kiyo Pro payloads.
- Show the camera in the existing CLI, tray and browser editor. Add named camera
  profiles and replay only explicitly saved settings at startup, reconnect, wake,
  and profile changes. Vendor extension values are labeled *saved/requested*, not
  live hardware readback.
- Expose standard Windows Image/Camera controls shown by the webcam's property
  dialog. The backend queries per-control support, range, step, default where
  known, and Auto/manual capability. Supported numeric settings and Auto modes
  are editable in the CLI and editor, saved with profiles and read back after
  writes. Unsupported controls are disabled. Standard autofocus Auto is separate
  from responsive/passive focus strategy; low-light compensation requires Auto
  exposure.
- The editor reads control values/ranges from the driver, separates Auto from
  manual values, and applies changes while browser preview remains running.
  Windows driver/permission errors are shown per control. Profiles store explicit
  requests and reapply them; Auto remains Auto instead of freezing a sampled
  measurement. Low-light compensation is kept in dependency with Auto exposure.
- Offer opt-in, video-only preview through the browser. It releases its stream
  on Stop/navigation. Resolution/FPS choices affect **preview only**, never
  other applications or a camera-wide default.
- Preserve existing HID device behavior. Firmware flashing, recording, audio
  capture, and device-wide resolution/FPS controls are not included.

## How to try it on Windows

```powershell
cargo build --release
.\target\release\secbird.exe list
.\target\release\secbird.exe -d 0E05 camera controls
.\target\release\secbird.exe -d 0E05 camera white-balance auto
.\target\release\secbird.exe -d 0E05 camera hdr off
.\target\release\secbird-tray.exe
```

Close Synapse before testing control writes so it cannot overwrite them. Open
the editor from the tray to compare the image before and after a supported
control changes. The browser's selected preview source must match the physical
camera being edited, particularly with two identical Kiyo Pros. A full camera
identity shown by `secbird list` can contain serial information; don't paste it
publicly without checking it.

## Verification and limitations

- A user built the earlier Kiyo Pro branch on Windows and captured native format
  diagnostics: the driver advertised 1920×1080 at discrete 60 FPS. That output
  establishes an advertised format, **not** this branch's standard-control
  behavior, image effects, or camera-wide format persistence.
- On this development Mac after merging upstream v0.4.10,
  `cargo test --release --workspace --quiet` passed:
  **106 unit tests, 1 CLI integration test, 1 doctest**; three live-network
  tests remained ignored. `node --test` passed **26** editor tests.
  `cargo clippy --release --workspace --all-targets -- -D warnings` passed
  locally and for the Windows GNU cross-target. These checks verify compilation
  and portable behavior; they do **not** prove the Windows driver accepts each
  control. CI and a physical Kiyo Pro are still required for that claim.
- A synthetic-video browser smoke test confirmed a standard brightness edit
  updated the API-supplied current value and saved profile while preview remained
  active. This used a fake camera endpoint, not a real device or driver.
- **Physical standard-control behavior and preview response are not yet verified.**
  Please follow [`docs/kiyo-pro-verification.md`](kiyo-pro-verification.md)
  and attach a sanitized `secbird report` when submitting the upstream device
  report. Do not label Kiyo Pro verified until those tests pass.
- Standard controls can be unavailable even when vendor controls work (or vice
  versa); Windows camera permissions and other camera apps can affect access.
  Some low-level defaults cannot be safely inferred, so Reset stays disabled
  for those controls. User-supplied screenshots are of the Synapse property
  dialog, not evidence that this implementation works on the device yet.

The project's PR template asks for a device report for `devices.json` changes:
https://github.com/infinityInfinite/secbird/issues/new?template=device-report.yml
