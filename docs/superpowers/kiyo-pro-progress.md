# Kiyo Pro support

Goal: support the Razer Kiyo Pro RZ1900364 on Windows 10/11 in the existing
Secretarybird Rust core, CLI, and tray/editor, suitable for an upstream PR.

- [x] Inspect fork and upstream discovery, transport, and tray integration.
- [x] Confirm Windows target and upstream architecture requirement.
- [x] Agree on approach and feature scope in chat.
- [x] Write and self-review architectural design specification.
- [x] Obtain user approval of the written specification.
- [x] Write implementation plan and agree on execution method (native).
- [x] Implement with regression tests.
- [x] Verify local builds/tests and document required Windows hardware checks.

Findings: discovery currently enumerates HID interfaces and requires a successful
Protocol 2.5 serial query. Kiyo Pro uses UVC camera controls (USB 1532:0E05),
including vendor extension-unit commands for HDR, field of view, and autofocus
mode. A database-only addition cannot provide camera support. The HID Transport
trait explicitly models feature reports; camera commands must not be disguised
as HID reports.

Product implementation is on branch `feat/kiyo-pro` in an isolated worktree.

Design: `specs/2026-10-18-kiyo-pro-design.md`. Firmware work is explicitly
excluded at the user's request. The amended specification was approved in chat.

Scope amendment: include opt-in editor video preview and resolution/FPS choices
for that preview session only. Photo capture/video recording remain excluded.
The specification includes camera-access conflicts, resource cleanup, and
preview-specific acceptance checks.

User approved the amended specification. Implementation plan written and
self-reviewed at `plans/2026-10-18-kiyo-pro.md`; native execution selected.
Preview uses browser-native capture inside the existing local editor. Portable
tests and Windows cross-compilation are available on this Mac; Windows runtime
and physical-camera validation remain outstanding. See
`../kiyo-pro-verification.md` for the hardware checklist.

## Verification so far

- macOS release workspace build: passed.
- macOS release workspace tests: 95 unit tests, 1 CLI integration test, and 1
  doctest passed; 2 existing
  live-network tests ignored.
- Browser lifecycle/profile-race tests: 20 passed using Node 22.
- Release clippy with warnings denied: passed on macOS and cross-checked for
  Windows GNU (workspace/all targets) and Linux GNU (core/CLI/all targets).
- Browser smoke test with a synthetic stream: preview played, a control edit
  preserved playback, and navigating away released tracks. Not a hardware test.
- New Rust module formatting and git whitespace checks: passed. Repository-wide
  rustfmt checks flag baseline upstream formatting; no unrelated formatting sweep.
- Windows/Linux runtime execution, Windows release executable linking, and the
  physical-camera checklist have not been run here. CI and hardware checks remain
  required before claiming native runtime or hardware verification.

## Implementation decisions

- Browser-native preview avoids a video server, frame queue, and codec dependency.
- Preview format presets are labeled as candidates until exact browser negotiation
  succeeds; they are not an exhaustive list of the camera's native formats.
- Camera JSON receives the store and foreground context explicitly, so edits and
  displayed values target the effective profile rather than its manual fallback.
- Portable engine/API tests run on the host; native route/KS tests compile in the
  Windows target and still require Windows execution.

## Final review

A fresh-context reviewer identified two important issues. Both were fixed with
regression tests: identical cameras now have consistent distinguishing labels
and inspectable local identities; profile operations capture their target before
awaiting, so late responses cannot overwrite a newly selected device. Multi-step
create/delete requests retain the original target, and superseded responses are
ignored. Browser smoke checks confirmed twin labels, About identity visibility,
and a camera profile response arriving after selection moved to a mouse.

Additional tests cover failure of the second FOV write without saving settings,
and imported profiles containing both camera and HID settings. Release suites
and cross-target clippy were rerun after these changes.

Deferred minor review findings:

- Native extension-node discovery reports a generic failure instead of preserving
  every candidate node's HRESULT and malformed-metadata details.
- After Stop/Start, the preview format dropdown may retain the previous selection
  while capture starts with browser defaults. The **Actual preview** label reports
  the newly negotiated format; use it rather than the dropdown as readback.
- Direct setting-write errors retain native HRESULTs but omit the requested
  setting/FOV-stage context (profile replay already adds setting context).

Physical control effects, firmware compatibility, concurrent browser/native
access, and Windows runtime behavior remain unverified rather than waived.

## Execution rulings and remaining risks

1. The harness worktree tool failed, so an explicitly approved Git worktree was
   used and the session moved to it. Risk: the harness may not list that worktree;
   Git still tracks branch `feat/kiyo-pro` and main remains unchanged.
2. The user authorized global Rust installation. Official rustup was installed
   without modifying shell profiles. Risk: the installation remains on the Mac
   until explicitly uninstalled.
3. On macOS, native Windows tests were replaced locally by cross-compilation and
   clippy, not treated as runtime evidence. Risk: Windows/driver failures may still
   surface; native CI and hardware testing remain required.
4. Camera JSON/dispatch receives Store and foreground context explicitly. Risk:
   this is an internal interface adjustment from the written plan; it avoids
   global-read coupling and targets the effective profile.
5. Portable engine/API tests run on the host; native server/KS tests remain
   Windows-only. Risk: those tests are compiled here but still need CI execution.
6. Preview presets are honestly labeled candidates because the browser does not
   enumerate all valid tuples. Risk: formats outside the preset list may only be
   available via browser defaults or another application. The final review
   accepted this trade-off without treating it as exhaustive format support.
7. Existing upstream formatting was not swept. Risk: whole-repository rustfmt
   remains red; new modules pass. The final review kept this baseline issue
   separate from feature defects.
8. The final review could not validate physical image effects, firmware
   compatibility, concurrent control/capture access, or Windows runtime behavior.
   These remain unverified, not waived; native testing may require adjustments.

## Native resolution/FPS experiment removed

Removed the experimental device-format read/set commands and investigation-only
format enumeration at the user's request. The associated COM requests, parser,
media-type allocations, format-specific Windows binding features, and active
documentation were removed as well. The experiment did not retain the selected
mode across the user's separate-process test; it was not a global camera setting.

HDR, HDR mode, field of view, autofocus strategy, profiles, and browser live
preview remain unchanged. Preview-only format choices remain local to the browser
session; they do not configure a device-wide capture mode.

Regression tests reject the removed commands without device/profile writes and
check that CLI help exposes the supported controls rather than the experiment.
Both regression tests failed before removal and pass afterward. The release
build, host/Windows/Linux cross-target clippy checks, and all 20 JavaScript tests
pass. The removal review found no actionable issues; native driver runtime
behavior remains outside the local macOS verification.

## Standard Windows controls follow-up

The user approved adding the standard Image/Camera controls shown in their
property dialog. The camera backend now queries native support and ranges on the
existing COM worker; CLI, authenticated editor API, browser UI and camera
profiles expose the supported values. Unsupported controls remain visible but
disabled. Standard Auto focus is separate from responsive/passive focus strategy.
Low-light compensation needs Auto exposure; anti-flicker Hz does not set video
FPS. No device-wide resolution/FPS control was reintroduced.

Portable validation, profile compatibility and browser rules are tested. A local
browser smoke test with synthetic video kept preview playing while a brightness
edit updated saved and readback values. That test used a fake camera API, not a
physical Kiyo Pro or Windows driver. The new PR summary is in
`docs/kiyo-pro-pr.md` and explicitly asks for the project's device report and
physical verification before claiming hardware support.
