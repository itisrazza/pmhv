# Roadmap

Milestones in order. Each one should leave pmhv usable for what it covers. Details of the how are in [design.md](design.md).

## MVP

Milestones marked *(done)* or *(in progress)*; see [Status](#status) for what's left.

1. *(done)* **Skeleton.** Cargo workspace, `pmhvd` serving an empty embedded UI, config loading, `pmhv` CLI talking to it, CI on macOS and Linux.
2. *(done)* **QEMU on macOS and Linux, headless.** Machine spec, create/start/stop/pause via shim, serial console in the browser, daemon restart without killing VMs.
3. *(done)* **Graphical console and thumbnails** for QEMU. Console tab with keyboard/mouse toolbar, virtual keyboard and mouse. TPM via `swtpm`, so Windows 11 guests work. The Machines grid, detail page and basic create dialog. Tests start here (see [development.md](development.md#testing)).
4. *(in progress)* **Auth.** Single-user sign-in with password and/or passkey, temporary first-launch password, sessions, log out, CLI reset. Needed before pmhv is left running for long.
5. *(done)* **Disk images.** Storage groups, upload/download, create blank, convert, resize.
6. *(in progress)* **Virtualization.framework backend.** macOS guests from IPSW, guest-VNC and serial consoles. Linux guests on VZ as an opt-in.

## After the MVP

Roughly in order:

7. **VZ private API spike** for a host-side graphical console (see [Consoles](design.md#consoles)).
8. **Networking and isolation.** Userspace switch, isolated networks, DHCP, port forwards, egress policy and logging (see [Networking](design.md#networking)). Port forwards on QEMU's built-in NAT are done already.
9. **Packaging.** Signed and notarised macOS builds, known-good QEMU and `swtpm` builds shipped with pmhv, service install. On macOS, a simple installer and uninstaller, with a short first-run flow that registers the service and sets the password, then opens the UI in a browser. On Linux, an install script that can be piped from `curl` into `sh`, installing the binaries and the systemd user service; first launch there keeps the temporary password in the log (see [Security](design.md#security)). Auto-updates on every platform, restarting pmhvd without stopping running machines. `pmhv service install` (launchd agent or systemd user service) is done already.
10. **Status indicator.** A lightweight helper in the macOS menu bar (and the Windows notification area once Windows is supported) showing running machines and resource usage, with a link to open the UI on localhost.
11. **Windows.** QEMU + WHPX, named pipes, service install. The same installer, uninstaller, first-run flow and auto-updates as on macOS.

## Candidates

Not scheduled yet:

- Snapshots and cloning
- cloud-init / unattended install
- Import from UTM and VirtualBox
- A designed create-machine flow (replacing the [placeholder](design.md#creating-a-machine))

## Status

What's left in the milestones marked in progress, and what's done but untested:

- **Auth:** passkeys (see [Security](design.md#security)). Password sign-in, the temporary first-launch password, setup, changing the password from Settings and `pmhv auth reset` are done.
- **Virtualization.framework:** macOS guests (IPSW install and boot) and the guest-VNC graphical console. Linux and other EFI guests with a text console are done, verified only as far as Apple's EFI firmware with a blank disk.
- **TPM:** the swtpm integration is written but hasn't been run, because swtpm wasn't installed on the development machine (`brew install swtpm`).
- **Guests:** booted so far: EDK2's UEFI shell, on text and graphical consoles. No real OS install has been tried yet.
