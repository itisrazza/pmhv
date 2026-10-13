# Development

## Prerequisites

macOS is the primary development platform; Linux is tested alongside it.

On macOS, via Homebrew:

```bash
brew install rustup just node qemu swtpm reuse
```

```bash
rustup default stable
```

On Linux, the same tools from the distro or upstream installers, plus access to `/dev/kvm` (usually membership of the `kvm` group).

QEMU provides `qemu-system-*`, `qemu-img` and the EDK2 UEFI firmware; `swtpm` is only needed for machines with a TPM. On Linux, install the distro's QEMU and OVMF/AAVMF firmware packages, which provide the firmware descriptors pmhv looks for in `/usr/share/qemu/firmware`. The Virtualization.framework backend will also need Xcode's command line tools for `codesign`.

Then install the web UI's dependencies once:

```bash
just web-deps
```

## Running

```bash
just dev
```

This builds the workspace, then runs `pmhvd` on <http://127.0.0.1:8640> and the Vite dev server (the URL it prints), which proxies `/api` and the console WebSockets to `pmhvd`.

On first launch `pmhvd` logs a temporary password and saves it as `initial-password.txt` in its data directory. Sign in with it in the browser, then choose a real password. Forgot it? `pmhv auth reset` issues a new temporary one.

The CLI talks to the same daemon over its local socket, without signing in:

```bash
cargo run -p pmhv -- ls
```

```bash
cargo run -p pmhv -- create spec.toml --disk-gib 32
```

```bash
cargo run -p pmhv -- start "Debian"
```

A spec file is the same TOML as a machine's `machine.toml`:

```toml
name = "Debian"
guest_os = "linux"
backend = "qemu"
arch = "aarch64"
firmware = "uefi"
cpu_cores = 4
memory_mib = 4096
network = "nat"
console = "text"

[[disks]]
bus = "virtio"
media = "cdrom"
image = { group = "default", name = "debian-13.7.0-arm64-netinst.iso" }
```

`pmhvd` reads `config.toml` from its data directory (`~/Library/Application Support/pmhv` on macOS, `~/.local/share/pmhv` on Linux) and uses the defaults when there isn't one:

```toml
[http]
listen = "127.0.0.1:8640"

[qemu]
# bin_dir = "/opt/homebrew/bin"           # where qemu-system-* and qemu-img live; default: PATH
# firmware_dirs = ["/usr/share/qemu/firmware"]
# swtpm = "/opt/homebrew/bin/swtpm"

[images.groups]
# external = "/Volumes/VMs"               # extra storage groups; "default" is <data dir>/images/default

[thumbnails]
interval_secs = 300
```

To keep development state separate, point both binaries elsewhere with `PMHV_DATA_DIR` and `PMHV_RUNTIME_DIR` (or `--data-dir` and `--runtime-dir`). The runtime directory holds the control socket, and unix socket paths are limited to about 100 bytes, so keep it short.

There's no TLS yet, so keep `listen` on localhost: on a LAN the password and session cookie would cross the network in the clear.

## Commands

All orchestration goes through [`just`](https://github.com/casey/just):

| Recipe | Does |
| --- | --- |
| `just web-deps` | Installs the web UI's npm dependencies |
| `just dev` | Runs `pmhvd` and the Vite dev server, with Vite proxying API calls to the daemon |
| `just build-web` | Builds the web UI into `web/dist` |
| `just build` | Builds the web UI, then the Rust workspace in release mode with the UI embedded |
| `just generate` | Regenerates `web/openapi.json` from the Rust types, then the web API client in `web/src/api` |
| `just fmt` | `cargo fmt` and Prettier |
| `just fmt-check` | The same, without changing anything |
| `just lint` | Clippy (warnings are errors) and oxlint |
| `just check` | Type checks: `cargo check` and `tsc` |
| `just test` | Unit tests |
| `just test-integration` | Boots UEFI firmware under QEMU through `pmhvd`, the shim and the CLI, restarting `pmhvd` mid-run |
| `just licences` | REUSE check on tracked files |

Before calling a change done: `just fmt lint check build`. CI runs the same on macOS and Linux, and also fails if `just generate` would change anything, so run it after touching API types.

Debug builds of `pmhvd` read `web/dist` from disk at runtime, so `just build-web` is enough to pick up UI changes without rebuilding Rust. Release builds compile the UI in. Without a built UI, `pmhvd` still runs and answers UI requests with a 503 explaining what to build.

A `just sign` recipe, for the Virtualization.framework entitlement, arrives with that backend.

## Testing

- Table-driven unit tests cover the pure parts: the QEMU command-line builder, spec validation, firmware descriptor matching, image names and path confinement, config parsing, the spec store, terminal text cleanup, sign-in and cookies.
- The QMP client is tested against a fake QMP server.
- `just test-integration` boots EDK2's UEFI shell (no guest OS download needed) with HVF on macOS or KVM on Linux, checks the serial console, kills and restarts `pmhvd`, and checks the machine kept running. CI runs it on Linux.
- VZ integration tests will run on macOS only, locally; CI runners may not allow nested virtualisation.

## Conventions

- Commits follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), with the crate or `web` as the scope where it helps (`feat(qemu): …`).
- Prose and documentation use New Zealand English; code identifiers use US English.
- Licensing metadata follows [REUSE](https://reuse.software/): `REUSE.toml` covers the whole repository, and `just licences` should pass.
