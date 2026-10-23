# Razer Kiyo Pro support on Windows

## Goal and scope

Support the Razer Kiyo Pro RZ1900364 (USB VID:PID `1532:0E05`) on Windows
10/11 through Secretarybird's existing Rust core, CLI, and tray/editor.
The change should be reviewable as an upstream PR, without replacing the HID
backend, adding a background service, installing a driver, or requiring Synapse.

The first version supports discovery, HDR on/off, HDR dark/bright mode,
wide/medium/narrow field of view, responsive/passive autofocus mode, and saved
profiles. The editor also provides an optional live video preview with supported
resolution and frame-rate selection for that preview session. Autofocus mode is
the vendor's focusing strategy, not a manual-focus slider or an autofocus enable
switch.

Out of scope: firmware checking or flashing, photo capture and video recording,
forcing resolution or frame rate on other applications, generic UVC image controls, other webcam models,
Linux/macOS camera implementations, and a general rewrite of device transport.
Existing non-Windows functionality must continue to build and work unchanged.

## Findings

Both the fork and upstream discover only Razer HID interfaces. Opening a device
requires a successful Protocol 2.5 serial query. The `Transport` trait represents
HID feature reports explicitly. Kiyo Pro uses UVC, including vendor extension-unit
controls, so neither a database-only entry nor a new HID transaction ID addresses
the recognition failure.

Public references:

- https://github.com/soyersoyer/kiyoproctrls
- https://github.com/openrazer/openrazer/issues/1665

Protocol facts will be independently encoded; implementation must respect the
reference project's license rather than copying its source into this project.

## Approach and alternatives

Add a camera-specific module to `secbird-core` alongside existing device support.
Retain the existing HID `Device`, `Transport`, and discovery API. Add explicit
camera discovery and operations, consumed by the current CLI and tray engine.
Use a small metadata view where consumers need common identity/labels; do not
force camera operations through HID commands or fabricate HID replies.

A broad generic-device/transport refactor would touch unrelated capabilities and
make upstream review harder. A standalone camera service would duplicate device
and profile ownership and violate the intended lightweight architecture. Neither
is part of this design.

## Windows backend

Use Windows' built-in camera driver through DirectShow enumeration and kernel
streaming extension-unit properties. Enumerate video capture devices, match the
USB vendor/product IDs from their device identity, and locate the extension unit
by GUID `23e49ed0-1178-4f31-ae52-d2fb8a8d3b48`, not a hard-coded node number.

Discover the node through camera topology, then use its node-specific KS control
interface for property requests. Check required properties and payload sizes
before writes. Bind only the capture device needed for controls; never start a
video stream merely to discover or configure a camera.

Keep COM objects on a dedicated, COM-initialized worker thread owned by the camera
backend. CLI and tray use request/reply messages with plain Rust values; raw COM
interfaces never cross threads or acquire unsafe `Send` implementations. Release
objects and uninitialize COM on their owning thread during shutdown.

Windows bindings are target-specific dependencies. The protocol encoding and
settings types stay platform-neutral and testable without Windows or hardware.
On other platforms the CLI explains that camera control is Windows-only.

## Identity and discovery

Add a webcam device kind and a Kiyo Pro database entry, without lighting, DPI,
polling, or button capabilities. Cameras appear in the existing connected-device
lists with camera-specific capabilities, not as failed Protocol 2.5 devices.

Use a namespaced camera identity derived from the Windows capture-device path.
Never merge two identical cameras merely because their product IDs match. Keep
the display label separate from the persistent identity. A path-based identity
may change when USB ports change; document this rather than claiming serial-based
identity when no usable serial is available.

Discovery is read-only: it must not change HDR, field of view, focus strategy, or
on-device persistent memory. A recognized camera whose controls cannot be opened
should remain visible with an actionable error, rather than silently disappearing.
Exclude any matching camera HID sibling from peripheral protocol probing when its
database classification identifies it as a webcam.

## Protocol and control operations

Use typed values for HDR, HDR mode, field of view, and autofocus strategy. The
camera module independently encodes the documented eight-byte command payloads.
The vendor extension selector for ISP writes is `1`.

Medium and narrow field-of-view changes require an ordered preparation command
followed by the selected command. Execute each setting sequence without
interleaving requests from another caller. Abort the sequence on the first failed
write and return the affected setting and Windows error.

The reference also documents an on-device save command. Do not send it
automatically: Secretarybird already owns saved profiles, and routine edits or
startup should not silently cause persistent camera writes.

Do not assume current settings can be read reliably. Distinguish desired/saved
values and successfully sent values from hardware-confirmed values. Initial
camera settings are unset, so adding a camera cannot overwrite its existing
configuration with invented defaults. Do not send the reference's unexplained
startup command.

## Existing application integration

### CLI and diagnostics

Include cameras in `secbird list` and read-only device reports. Extend existing
device selection to resolve a camera identity or an unambiguous product ID;
multiple matching cameras require explicit selection.

Add a `camera` command with typed subcommands for `hdr`, `hdr-mode`, `fov`, and
`af-mode`. Help text states the accepted values and Windows-only scope. Reject
camera commands for HID devices and peripheral commands for cameras before I/O.
Report success only when the corresponding write sequence succeeds.

### Profiles

Extend the existing profile schema with optional camera settings using serde
defaults so older files still load. Reuse the existing profile file and profile
selection semantics, not a second settings store. Apply only explicitly stored
camera values on profile activation, startup, reconnect, and wake. Retain errors
per setting through the existing apply-report mechanism.

### Tray/editor

Extend the existing engine's enumeration, selection, and rescan paths to include
cameras. Add a Camera section with the four supported controls; hide mouse,
keyboard, lighting, and other inapplicable sections. Reuse the existing local
HTTP server and its authentication, input validation, and profile persistence.

Show values as saved/requested, not live readbacks. Surface camera-busy,
disconnected, inaccessible, and unsupported-extension errors. A disconnect must
invalidate stale camera handles; rescan reopens the currently connected device.
Profile replay must not prevent other devices from being applied if a camera
fails. No firmware features are added for cameras.

### Live video preview and preview formats

Provide an opt-in live preview so users can immediately see the effects of HDR,
field-of-view, and focus changes. This requires a capture/streaming path that
delivers camera frames to the browser editor, with explicit camera-access and
session-lifecycle management. Preview is not photo capture or video recording;
do not store frames or request microphone access.

Start streaming only when the user requests preview. Discovery, device reports,
profile application, and merely opening the editor must not start a stream.
Ensure the preview source matches the camera being configured; if that mapping
is ambiguous, require explicit source selection rather than showing another
camera silently. Stop and release capture resources when preview is stopped,
the selected camera changes, the editor closes, or the camera disconnects. Clean
up abandoned sessions as well; do not rely solely on browser unload events.

Camera access can conflict with OBS, Teams, or other applications. Show clear
busy/access-denied errors and allow configuration without preview. Reuse the
existing authenticated local-server boundary for any backend preview endpoints;
do not introduce an unauthenticated video feed. Keep frame buffering bounded so
a slow or disconnected browser cannot accumulate frames indefinitely.

Offer resolution/FPS combinations supported by the selected camera and capture
path, rather than independent choices that may form an unsupported combination.
Show the actual negotiated format, report unsupported combinations, and account
for format restrictions when HDR changes. Release and reconfigure the preview
session when needed without silently claiming a requested format was accepted.

Resolution and FPS are normally selected by the recording application for its
own capture session. Secretarybird's choices apply only to its preview; they do
not force OBS, Teams, or other applications to use the same format. Label these
as preview settings, separate from camera-control profiles, and do not replay
them as device-wide settings on startup or reconnect. Reconnect must not restart
preview automatically.

## Verification and acceptance

Write failing tests before production changes for protocol payloads, ordered
field-of-view sequences, early failure handling, input validation, identity and
selection, backwards-compatible profiles, and unset-settings/no-write behavior.
Use a fake camera control boundary rather than COM or physical hardware in unit
tests. Cover editor/API rejection of settings that do not apply to the device.
Add preview tests for opt-in startup, source selection, supported format
validation, negotiated-format reporting, bounded buffering, session cleanup,
and camera-access failures. Ensure profile application never starts preview.

Run existing regression tests, formatting, and appropriate lint checks. Compile
the Windows core, CLI, and tray paths; keep Linux core/CLI checks green. Verify
the existing HID simulator behavior is unaffected. Where the current macOS
environment cannot execute Windows checks, document the limitation and use
Windows CI rather than treating a non-Windows build as Windows verification.

Before claiming hardware-verified support, test on Windows with a physical Kiyo
Pro and Synapse closed:

1. Correct recognition, no duplicate entries, and read-only discovery/reporting.
2. Visible HDR and field-of-view changes, plus autofocus-mode command behavior.
3. Profile switching and restart/reconnect/wake reapplication.
4. Errors and recovery with the camera unplugged or in use by a video app.
5. No regressions for an existing HID device used alongside the camera.
6. Preview shows the selected camera and live HDR/field-of-view/focus changes.
7. Supported preview resolution/FPS combinations negotiate correctly, including
   HDR-related restrictions; the UI reports the actual format.
8. Stopping preview, switching devices, closing the editor, and unplugging the
   camera release capture resources. Verify access conflicts and recovery with
   a video app, and confirm reconnect does not restart preview automatically.

Record tested Windows and camera firmware versions in the PR. Until physical
checks pass, describe the feature as implemented but not hardware-verified. Do
not infer success from a saved UI value or from successful compilation.
