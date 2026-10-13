# Roadmap

Milestones in order. Each one should leave pmhv usable for what it covers. Details of the how are in [design.md](design.md).

## MVP

1. **Skeleton.** Cargo workspace, `pmhvd` serving an empty embedded UI, config loading, `pmhv` CLI talking to it, CI on macOS and Linux.
2. **QEMU on macOS and Linux, headless.** Machine spec, create/start/stop/pause via shim, serial console in the browser, daemon restart without killing VMs.
3. **Graphical console and thumbnails** for QEMU. Console tab with keyboard/mouse toolbar, virtual keyboard and mouse. TPM via `swtpm`, so Windows 11 guests work. The Machines grid, detail page and basic create dialog. Tests start here (see [development.md](development.md#testing)).
4. **Auth.** Single-user sign-in with password and/or passkey, temporary first-launch password, sessions, log out, CLI reset. Needed before pmhv is left running for long.
5. **Disk images.** Storage groups, upload/download, create blank, convert, resize.
6. **Virtualization.framework backend.** macOS guests from IPSW, guest-VNC and serial consoles. Linux guests on VZ as an opt-in.

## After the MVP

Roughly in order:

7. **VZ private API spike** for a host-side graphical console (see [Consoles](design.md#consoles)).
8. **Networking and isolation.** Userspace switch, isolated networks, DHCP, port forwards, egress policy and logging (see [Networking](design.md#networking)).
9. **Packaging.** Signed and notarised macOS builds, known-good QEMU and `swtpm` builds shipped with pmhv, service install.
10. **Windows.** QEMU + WHPX, named pipes, service install.

## Candidates

Not scheduled yet:

- Snapshots and cloning
- cloud-init / unattended install
- Import from UTM and VirtualBox
- A designed create-machine flow (replacing the [placeholder](design.md#creating-a-machine))
