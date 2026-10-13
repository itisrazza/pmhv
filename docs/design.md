# pmhv design

> Status: draft for review. Nothing here is implemented yet. Items marked **[decide]** need a call before work starts; items marked **[verify]** are assumptions to check against current platform docs.

## Summary

pmhv (Poor Man's HyperVisor) is a small daemon and web UI for running virtual machines on a machine that is primarily used for something else — a MacBook, a Linux workstation, a Windows desktop with WSL2. It sits somewhere between UTM/virt-manager (desktop apps) and Proxmox/Hyper-V Server (dedicated hypervisor OSes): a headless service with a browser UI, without taking over the host.

## Goals

- Manage VMs from a browser: create, start, stop, pause, console, edit hardware, notes.
- Manage disk images (VM disks, ISOs, boot images) and virtual networks.
- Run as an unprivileged user service where possible. macOS is the primary target, Linux is tested alongside it, and Windows comes later.
- Pluggable hypervisor backends:
  - **QEMU** on every platform (KVM on Linux, HVF on macOS, WHPX on Windows, TCG for foreign architectures).
  - **Virtualization.framework** on macOS.
- VMs survive a daemon restart or upgrade.
- One self-contained binary with the web UI embedded.

## Non-goals (for now)

- Clustering, live migration, HA — this is one host.
- Containers.
- Replacing the host OS or requiring root/admin for basic use.
- A native desktop app. (A thin menu-bar/tray helper may come later.)

## Reading the mockups

From the four screens (Machines, Disk images, Networks, machine detail):

| Mockup element | Design implication |
| --- | --- |
| Machine cards with live/last-frame thumbnails | Machines with a graphical console produce screenshots: QMP `screendump` on QEMU, a frame from the VNC stream on Virtualization.framework (see [Consoles](#consoles)). Text-console machines show a rendered terminal snapshot. Cache the last frame on stop. |
| Hypervisor icon + "native" chip icon per card | Model carries `backend` and `arch`; "native" is derived (guest arch == host arch, hardware-accelerated). Windows CE Dev has no chip → emulated (TCG). |
| Running / Stopped badges | Live state pushed to the UI (WebSocket/SSE), not polled. |
| Favourites in the sidebar | Per-user UI preference, stored server-side so it follows the user across browsers. |
| Disk images: "Default image group", "ISO images" | Images live in **storage groups** (directories). Show allocated vs virtual size (`13.5 GiB / 128.0 GiB`). Upload, download, delete, and a menu for more (resize, convert, clone). |
| Networks: Built-in NAT, Built-in host-only, user-defined subnets | pmhv owns a network model with per-network subnets. This is the hardest cross-platform piece (see [Networking](#networking)). |
| Detail page: power, pause, menu, settings; console preview with "Open console" | Console is embeddable and also openable full-size. |
| Hardware list: Hypervisor, Architecture, Machine (`virt`), CPU, Memory, Disk / VirtIO 0, Disk / SATA | Hardware model is bus-aware. Some fields are backend-specific (machine type is a QEMU concept). |
| Notes | Free-form Markdown per VM. |
| Disconnect | Log out. There is a single user, identified by a password or passkey (see [Security](#security)). |

Small inconsistency: "Windows CE Dev" says Windows 11 but shows a Windows 7 thumbnail — presumably a placeholder. There's no create-machine screen yet; [Creating a machine](#creating-a-machine) describes a basic one to start from.

## Architecture

```
            browser (React + @swopstar/react-ui)          pmhv CLI
                         │  HTTPS + WebSocket                │ unix socket / named pipe
                         ▼                                   ▼
 ┌──────────────────────────────────────────────────────────────────────┐
 │ pmhvd                                                                │
 │  HTTP API · auth · event bus · image store · network manager         │
 │  VM registry (machine.toml) · supervisor (talks to shims)            │
 └──────────────┬──────────────────────┬────────────────────────────────┘
                │ local socket, one per VM (JSON-RPC)
        ┌───────▼───────┐      ┌───────▼────────┐
        │ pmhv-shim     │      │ pmhv-shim      │      …one per running VM
        │  qemu driver  │      │  vz driver     │
        │   └─ qemu-system-* (child, QMP)       │  Virtualization.framework in-process
        └───────────────┘      └────────────────┘
```

### Components

- **`pmhvd`** — the daemon. Serves the API and embedded UI, owns configuration, images and networks, and supervises shims. Stateless with respect to running VMs: on start it rediscovers shims from their sockets in the runtime directory.
- **`pmhv-shim`** — one process per running VM, similar in spirit to `containerd-shim`. It owns the hypervisor and exposes a small, uniform RPC (start, stop, pause, screenshot, console streams, status, events). This is what lets VMs outlive `pmhvd`, and it solves a Virtualization.framework constraint: a VZ VM lives and dies with the process that created it, so that process must not be the daemon.
- **`pmhv`** — CLI over the same API. Useful for scripting and for testing the backend before the UI exists.
- **`web/`** — React SPA, built with Vite and embedded into `pmhvd` at compile time.

### Backend trait

Roughly:

```rust
trait Driver {
    fn capabilities(&self) -> Capabilities;           // arches, buses, disk formats, display, snapshots…
    fn validate(&self, spec: &MachineSpec) -> Result<(), SpecError>;
    async fn start(&self, spec: &MachineSpec, rt: &RuntimeDir) -> Result<Running, StartError>;
}

trait Running {
    async fn stop(&self, mode: StopMode) -> Result<(), ControlError>;   // ACPI, force
    async fn pause(&self) -> Result<(), ControlError>;
    async fn resume(&self) -> Result<(), ControlError>;
    async fn screenshot(&self) -> Result<Image, ScreenshotError>;
    fn console(&self, kind: ConsoleKind) -> Result<ConsoleStream, ConsoleError>; // serial, framebuffer
    fn events(&self) -> EventStream;
}
```

`Capabilities` drives the UI: the create/edit forms only offer what the chosen backend can do, and `validate` gives specific errors rather than failing at boot.

## Data model and storage

Declarative, file-based, one directory per thing. Human-readable, diffable, easy to back up, no database migration story needed early on.

```
$DATA/                         # ~/Library/Application Support/pmhv, $XDG_DATA_HOME/pmhv, %LOCALAPPDATA%\pmhv
  config.toml                  # daemon settings, storage groups, listen address
  machines/<id>/
    machine.toml               # spec: name, backend, arch, cpu, memory, devices, notes
    nvram.fd / aux storage     # firmware state, backend-specific
    thumbnail.png              # last frame
  images/<group>/…             # disk images and ISOs (groups can point elsewhere, e.g. an external drive)
  networks/<id>.toml
  ui/preferences.toml          # favourites etc.
$RUNTIME/                      # $XDG_RUNTIME_DIR/pmhv, $TMPDIR/pmhv
  machines/<id>/shim.sock, qmp.sock, vnc.sock, serial.sock, shim.pid
```

- IDs are ULIDs; names are display-only and editable.
- Disks reference images by `group/path`, so moving a VM between groups is a config edit.
- Runtime state (running/paused/stopped) is never written to `machine.toml`; it's derived from shims.
- An index (in-memory, maybe a SQLite cache later) is rebuilt from disk at startup.

**Disk formats:** qcow2 for QEMU (thin, snapshots); raw for Virtualization.framework, which can't read qcow2. Sparse raw files keep the "allocated / virtual" display meaningful on APFS and ext4. **[verify]** whether recent macOS versions add a sparse image format VZ can use directly. Conversion between formats via `qemu-img`, exposed as an image action.

## Backends

### QEMU (all platforms)

- Spawned by the shim as a child process with a generated command line; controlled over **QMP** on a unix socket (named pipe on Windows).
- Accelerator chosen automatically: KVM (Linux), HVF (macOS), WHPX (Windows), TCG when guest arch ≠ host arch. This is what gives the mockup's x86 Windows XP on an ARM Mac.
- Display: VNC on a unix socket, proxied by the daemon to noVNC (never exposed directly). Screenshots via QMP `screendump`.
- Serial: a unix socket chardev, proxied to xterm.js.
- Firmware: EDK2/OVMF from the QEMU install; per-VM NVRAM copy.
- TPM for Windows 11 guests needs `swtpm` as an extra child process. **[decide]** whether to support this in v1.
- pmhv locates QEMU on `PATH` or via config; it doesn't bundle it. QEMU is GPL, but running it as a separate process keeps pmhv's own licence unconstrained.

### Virtualization.framework (macOS)

- Bound from Rust with the `objc2` family of crates (there are generated `objc2-virtualization` bindings). **[verify]** binding coverage for the APIs we need.
- The shim binary must be code-signed with `com.apple.security.virtualization`. Ad-hoc signing works for local development; a `just` recipe handles it. Bridged networking needs `com.apple.vm.networking`, which Apple only grants on request — plan around not having it.
- Guests: Linux (ARM64, EFI boot), macOS (Apple silicon only, from an IPSW, limited to two concurrent macOS VMs by the licence/kernel). Windows ARM isn't a realistic VZ guest (no TPM); route it to QEMU.
- Rosetta for Linux guests is a nice extra, exposed as a VZ-only capability.

### Linux

QEMU with KVM. No libvirt dependency: it isn't cross-platform, and driving QEMU directly keeps one code path. A libvirt backend could be added later for people who already run libvirt.

### Windows

The Windows counterpart to Hypervisor.framework is the **Windows Hypervisor Platform (WHP)**, which QEMU already uses as its WHPX accelerator. It coexists with Hyper-V and WSL2. So Windows needs no separate backend: it's the QEMU backend plus platform plumbing (named pipes instead of unix sockets, Windows paths, startup/service install). That work is deferred, not designed out — keep platform-specific code behind small modules so it slots in later.

## Consoles

Every machine has one console type, set in its spec:

- **Graphical** — VNC. The browser uses noVNC.
- **Text** — serial. The browser uses xterm.js.

Which one a machine uses depends on its setup (a headless Linux runner is text, a desktop OS is graphical). The UI and API treat both uniformly: the detail page embeds whichever the machine has, and "Open console" opens it full-size. pmhvd always proxies the stream over an authenticated WebSocket; consoles are never exposed on a network port directly.

| | Graphical (VNC) | Text (serial) | Thumbnail |
| --- | --- | --- | --- |
| QEMU | QEMU's built-in VNC server on a unix socket | chardev on a unix socket | QMP `screendump` |
| VZ | VNC server inside the guest, reached over the VM network | `VZFileHandleSerialPortAttachment` | frame from the VNC stream |

Virtualization.framework has no public API to read a VM's framebuffer (graphics only render into an AppKit `VZVirtualMachineView`), so a VZ graphical console connects to a VNC server running in the guest. macOS guests have one built in (Screen Sharing); Linux guests need one installed. The shim finds the guest's address from the vmnet DHCP leases by MAC address, connects, and hands the stream to pmhvd like any other VNC source. Consequences:

- The graphical console is only available once the guest has booted and started its VNC server, so early boot and installers aren't visible. For VZ, installation is driven by an IPSW (macOS) or a serial console (Linux), so this mostly doesn't matter.
- Credentials for the guest's VNC server are stored in the machine spec.

Not doing: Virtualization.framework's private VNC server class (private API, could vanish in any macOS release) or capturing an offscreen window with ScreenCaptureKit (needs the Screen Recording permission and is fragile).

Text-console thumbnails are a rendered snapshot of the terminal buffer, which the shim keeps anyway for scrollback.

## Networking

Not needed for the first milestones: v1 uses each backend's built-in NAT (QEMU user-mode networking, VZ NAT attachment) so VMs have internet access from day one. The Networks screen shows those as "Built-in NAT" without editing.

The reason for proper networking later is **isolation**: VMs (agents, CI runners) shouldn't reach the internet, the LAN or each other unless explicitly allowed, and it should be visible when they try. The mockup has pmhv-owned networks with their own subnets: Built-in NAT, Built-in host-only, plus user networks like "AI sandbox". Each backend/OS has a different native model:

- QEMU: user-mode (slirp/passt, unprivileged, NAT only), tap/bridge (Linux, privileged), vmnet (macOS, privileged or entitled), socket/stream netdevs.
- VZ: NAT attachment (fixed vmnet subnet, no per-network subnets), bridged (restricted entitlement), file-handle attachment (raw Ethernet frames over a datagram socket). **[verify]** whether newer macOS versions expose configurable vmnet networks to VZ.

**Proposal: a userspace virtual switch owned by pmhv (`pmhv-netd`, or a task inside `pmhvd`).**

- Each network is an L2 switch in userspace. VMs connect through a socket: QEMU `-netdev stream/dgram`, VZ `VZFileHandleNetworkDeviceAttachment`. Same model on every backend and OS, no root needed, and VMs on different backends can share a network.
- The switch provides DHCP and DNS for its subnet, and for NAT networks, outbound NAT through host sockets (the approach used by gvisor-tap-vsock in Podman Machine and by passt). A Rust userspace TCP/IP stack such as `smoltcp` is the likely basis.
- Because every outbound connection goes through pmhv, egress policy comes naturally: per network, deny by default, allow by destination (host, CIDR, port), log every attempt, and surface blocked attempts in the UI. DNS answers can be logged too, which makes "what is this VM trying to reach" readable.
- Port forwarding (host port → guest port) is a first-class network feature, and is how the host reaches guests on unprivileged setups.
- **Host-only with the host on the subnet** needs a real host interface (vmnet host mode on macOS, a tap/bridge on Linux), which needs privileges. Treat that as an optional, privileged network type, enabled via a small helper installed with admin rights. Without it, host-only means isolated from the outside world, with host access via port forwards.

This is the biggest chunk of novel work in the project; it could even be its own crate published separately. Backend-native NAT stays available afterwards as a simple option.

## API

- REST + JSON over HTTP, described with OpenAPI generated from the Rust types (e.g. `utoipa`). TypeScript types for the UI are generated from the spec, so there's one source of truth.
- One WebSocket (or SSE) event stream for state changes, progress (uploads, conversions) and thumbnails.
- Per-console WebSockets: binary VNC (RFB) passthrough and serial byte streams.
- Large uploads (ISOs, disk images) are streamed straight to disk, resumable if practical. Downloads stream and support `Range`.
- Long-running operations (image conversion, IPSW install, downloads from URL) are jobs with IDs and progress events.

Rough resource map:

```
/api/v1/machines            GET POST
/api/v1/machines/{id}       GET PATCH DELETE
/api/v1/machines/{id}/power POST {action: start|stop|reset|pause|resume, force?}
/api/v1/machines/{id}/thumbnail
/api/v1/machines/{id}/console/{vnc|serial}   (WebSocket)
/api/v1/images  /api/v1/images/{group}/{name}   (+ upload, download, convert, resize)
/api/v1/networks  /api/v1/networks/{id}
/api/v1/host                 capabilities, backends found, resources
/api/v1/events               (WebSocket/SSE)
/api/v1/jobs/{id}
```

## Web UI

- Vite + React + TypeScript + `@swopstar/react-ui` (Tailwind v4, shadcn-style components). Built into `web/dist` and embedded in `pmhvd` (`rust-embed` or similar).
- Data fetching with a small query layer (TanStack Query is the obvious candidate) driven by the event stream to invalidate caches.
- noVNC for framebuffer consoles, xterm.js for serial.
- Screens from the mockups: Machines, Machine detail, Disk images, Networks, Settings, plus a create-machine flow (below) until a proper one is designed.
- `@swopstar/react-ui` is installed from npm. Its large CSS bundle is a known tradeoff, being fixed in that library.

### Creating a machine

A placeholder until a designed flow exists: a single dialog from the **+** button on the Machines screen, in the same visual style as the rest.

1. **Name.**
2. **Backend** — QEMU or Virtualization.framework, only those found on the host. Changing it re-filters the remaining fields using the backend's `Capabilities`.
3. **Architecture** — native by default; QEMU also offers emulated ones, labelled as slow.
4. **Guest OS** — Linux, macOS (VZ only), Windows, Other. Only sets sensible defaults (firmware, disk bus, console type, Windows' TPM need); everything stays editable.
5. **CPU and memory** — cores and GiB, defaulting to half the host's cores and a per-OS memory size, capped at what the host has.
6. **Boot disk** — create a new blank disk (size, in the default image group) or pick an existing image.
7. **Install media** — optional ISO/IMG from the image store, or an IPSW for macOS guests.
8. **Console** — graphical or text, defaulted from the guest OS.
9. **Notes** — optional.

**Create** writes `machine.toml` and the blank disk, then lands on the machine's detail page, stopped. Everything else (extra disks, buses, networks) is edited from the detail page's settings afterwards.

## Security

The daemon can boot arbitrary code and read arbitrary files on the host, so every request is authenticated, even on localhost.

- There is a **single user**. They sign in with a password or a passkey (WebAuthn) and get a session cookie; "Disconnect" in the UI ends the session. **[decide]** password only, passkey only, or either. Passkeys need a stable origin (hostname + HTTPS, or `localhost`), which matters if pmhv is reached by IP address over the LAN.
- First run: the daemon prints a one-time setup link, used to set the password or register the first passkey.
- Default listen address is `127.0.0.1` only. Exposing on the LAN requires TLS (user-provided certificate, or a self-signed one generated on first run).
- The CLI uses a local socket protected by filesystem permissions, and doesn't need to sign in.
- Console and API WebSockets are authenticated the same way as HTTP, with origin checks.
- Image paths are confined to configured storage groups; no `..` or absolute paths from the API.
- Run as the logged-in user. Anything privileged (host-only networking, bridging) goes through a narrowly scoped helper, not a root daemon.

## Running as a service

- macOS: a launchd user agent (`~/Library/LaunchAgents`). VZ works from a user agent. **[verify]** behaviour across logout and on a closed-lid MacBook; sleep is the bigger practical issue, and pmhv may want to hold a power assertion while VMs run.
- Linux: a systemd user unit (with lingering for headless hosts) or a system unit.
- Windows (later): a per-user startup task first; a proper service after that.
- `pmhv service install|uninstall` writes the right unit for the platform.

## Repository layout

```
Cargo.toml                 workspace
crates/
  pmhv-model/              specs, IDs, capabilities, API types (serde + OpenAPI)
  pmhv-driver/             Driver/Running traits, shim RPC protocol
  pmhv-qemu/               QEMU driver, QMP client, command-line builder
  pmhv-vz/                 Virtualization.framework driver (cfg(target_os = "macos"))
  pmhv-net/                userspace switch (later)
  pmhvd/                   daemon
  pmhv-shim/               per-VM supervisor binary
  pmhv/                    CLI
web/                       React app
docs/
justfile
```

`just` recipes: `dev` (daemon + Vite with proxy), `build`, `sign` (macOS entitlements), `check`, `fmt`, `lint`, `test`.

## Testing

Deferred until after the initial buildout (roughly milestone 3), once the shape of the code has settled. Then:

- Table-driven unit tests for the pure parts: the QEMU command-line builder, spec validation against capabilities, config parsing, path confinement.
- A fake QMP server to test the QEMU driver's control flow without running QEMU.
- Integration tests that boot a tiny guest (a minimal Linux kernel and initramfs) under QEMU, on macOS (HVF) and Linux (KVM where available, TCG otherwise), driven through the CLI.
- VZ integration tests on macOS only, run locally; CI runners may not allow nested virtualisation.

## Milestones

1. **Skeleton.** Workspace, `pmhvd` serving an empty embedded UI, config loading, `pmhv` CLI talking to it, CI on macOS + Linux.
2. **QEMU on macOS and Linux, headless.** Machine spec, create/start/stop/pause via shim, serial console in the browser, daemon restart without killing VMs.
3. **Graphical console and thumbnails** for QEMU. The Machines grid, detail page and basic create dialog. Tests start here.
4. **Auth.** Single-user sign-in, sessions, log out, first-run setup. Needed before pmhv is left running for long.
5. **Disk images.** Storage groups, upload/download, create blank, convert, resize.
6. **Virtualization.framework backend.** Linux guests first, then macOS guests from IPSW. Serial console; guest-VNC console.
7. **Networking and isolation.** Userspace switch, isolated networks, DHCP, port forwards, egress policy and logging.
8. **Windows.** QEMU + WHPX, named pipes, service install.

Snapshots, cloning, cloud-init/unattended install, and import from UTM/VirtualBox are candidates after that.

## Open questions

- **Auth**: password, passkey, or either (see [Security](#security)).
- **Windows 11 guests**: TPM via `swtpm` early, or with the Windows milestone?
- **Bundling QEMU**: rely on Homebrew/distro packages, or ship a known-good build?
- **Licence** for pmhv itself.
- **Create-machine flow**: to be designed; [Creating a machine](#creating-a-machine) is the placeholder.
