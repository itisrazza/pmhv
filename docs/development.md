# Development

> Nothing is buildable yet. This describes the intended setup so the first milestone has something to aim for.

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

The Virtualization.framework backend needs Xcode's command line tools for `codesign`.

## Commands

All orchestration goes through [`just`](https://github.com/casey/just):

| Recipe | Does |
| --- | --- |
| `just dev` | Runs `pmhvd` and the Vite dev server, with Vite proxying API calls to the daemon |
| `just build` | Builds the web UI, then the Rust workspace with the UI embedded |
| `just sign` | Ad-hoc signs the shim with the `com.apple.security.virtualization` entitlement (macOS) |
| `just fmt` | `cargo fmt` and the web formatter |
| `just lint` | `cargo clippy` and the web linter |
| `just check` | Type checks: `cargo check` and `tsc` |
| `just test` | Unit tests, then integration tests where a hypervisor is available |

Before calling a change done: `just fmt lint check build`.

## Testing

Deferred until after the initial buildout (milestone 3 in the [roadmap](roadmap.md)), once the shape of the code has settled. Then:

- Table-driven unit tests for the pure parts: the QEMU command-line builder, spec validation against capabilities, config parsing, path confinement.
- A fake QMP server to test the QEMU driver's control flow without running QEMU.
- Integration tests that boot a tiny guest (a minimal Linux kernel and initramfs) under QEMU, on macOS (HVF) and Linux (KVM where available, TCG otherwise), driven through the CLI.
- VZ integration tests on macOS only, run locally; CI runners may not allow nested virtualisation.

## Conventions

- Commits follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), with the crate or `web` as the scope where it helps (`feat(qemu): …`).
- Prose and documentation use New Zealand English; code identifiers use US English.
- Licensing metadata follows [REUSE](https://reuse.software/): `REUSE.toml` covers the whole repository, and `reuse lint` should pass.
