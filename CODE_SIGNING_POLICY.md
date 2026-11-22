# Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

## What is signed

Windows release files built by the [Release workflow](.github/workflows/release.yml) from this repository:

- `secbird-tray.exe` and `secbird.exe`, in the installer and in the portable zip
- `Secretarybird-Setup-<version>.exe`, the installer

Each release is built on GitHub-hosted runners from a tagged commit. Nothing is built or signed on a personal machine. The Linux build is not signed.

Secretarybird contains no proprietary code. Its dependencies are open-source Rust crates listed in `Cargo.lock`.

## Team roles

- Committers and reviewers: [infinityInfinite](https://github.com/infinityInfinite)
- Approvers: [infinityInfinite](https://github.com/infinityInfinite)

Pull requests from outside contributors are reviewed by a committer before they are merged. Every signing request is approved by hand in SignPath.

Team members use multi-factor authentication for GitHub and SignPath.

## Privacy

Secretarybird sends no telemetry, usage data or device data. It makes two kinds of network request:

- **Update check:** once a day, the tray asks `api.github.com` for the latest Secretarybird release. It's on by default. To turn it off, use "New version notices" in the editor's Settings, or "Tell me when a new version is out" in the tray menu. When you choose to update, the installer is downloaded from GitHub Releases.
- **Firmware check:** only when you click "Check for firmware update" or turn on "Check for firmware updates weekly" (off by default). Secretarybird asks `apps.razer.com`, the same server Synapse uses, whether newer firmware exists for your connected devices. The request contains the device's product ID, never a serial number or anything about you.

Nothing else is sent anywhere. The editor runs on `127.0.0.1` and can't be reached from other computers.

## Reporting

To report a signed file that does something this page doesn't describe, open an issue in this repository.
