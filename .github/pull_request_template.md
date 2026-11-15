<!--
Adding or fixing a device in devices.json? Please open a device report instead:
https://github.com/infinityInfinite/secbird/issues/new?template=device-report.yml
with the output of `secbird report` (or tray → Make a device report…). Every entry is checked
against a report before it ships.

For code changes, please read CONTRIBUTING.md first: the five rules (visible and safe, easy,
light, new devices are easy to add, checkable), the security checklist and the PR checklist.
-->

## What it does

## What I tested, on which device

## What I couldn't test

## Checklist
- [ ] `cargo test` and `cargo clippy --all-targets -- -D warnings` pass
- [ ] `node --check` on the editor script (if `editor.html` changed)
- [ ] Simulator test for new device behaviour; a source comment for each protocol fact
- [ ] No new network calls, dependencies or system changes (or explained above)
- [ ] CHANGELOG line under [Unreleased]; screenshots in dark and light theme for UI changes
