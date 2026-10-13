<div align="center"><img src="logo.svg" alt="" width="64" height="64"></div>
<h1 align="center">pmhv</h1>
<div align="center">
  <img alt="Licence: MIT OR Apache-2.0" src="https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue">
</div>

**Poor Man's HyperVisor** — a small daemon and web UI for running virtual machines on a computer that's mainly used for something else.

Proxmox and Hyper-V Server want the whole machine. Desktop apps like UTM want you sitting in front of it. pmhv sits in between: a background service on your MacBook (or Linux box, or eventually Windows desktop) with a browser UI for creating, running and reaching your VMs, without replacing the host OS.

> **Status: planning.** There's no code yet. The design and roadmap are below; nothing here works until the first milestones land.

## Planned features

- Manage machines, disk images and ISOs, and networks from the browser
- Consoles that open in their own tab — graphical (VNC) or text (serial) — with helpers for the key combos the browser normally swallows, plus a virtual keyboard and mouse
- Live thumbnails of running machines
- VMs keep running when the daemon restarts or upgrades
- Single-user sign-in with a password and/or passkey
- Isolated networks with egress control, so VMs can't reach out without you knowing (after the MVP)

## Platforms and backends

| Host | Backends | Priority |
| --- | --- | --- |
| macOS (Apple silicon) | QEMU (HVF), Apple Virtualization.framework | Primary |
| Linux | QEMU (KVM) | Tested alongside macOS |
| Windows | QEMU (WHPX, coexists with Hyper-V and WSL2) | Later |

QEMU also emulates foreign architectures (slowly), so x86 guests can run on an ARM Mac.

## Built with

- A Rust daemon (`pmhvd`), a small supervisor process per VM (`pmhv-shim`) and a CLI (`pmhv`)
- A React web UI using [`@swopstar/react-ui`](https://github.com/swopstar/react-ui), embedded in the daemon binary

## Documentation

- [Design](docs/design.md) — architecture, backends, consoles, networking, API, security
- [Roadmap](docs/roadmap.md) — milestones to the MVP and beyond
- [Development](docs/development.md) — prerequisites, `just` recipes, testing and conventions

## Licence

Licensed under either of [MIT](LICENSES/MIT.txt) or [Apache-2.0](LICENSES/Apache-2.0.txt), at your option.
