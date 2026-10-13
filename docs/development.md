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

QEMU and `swtpm` aren't used yet; they're needed from milestone 2. The Virtualization.framework backend will also need Xcode's command line tools for `codesign`.

Then install the web UI's dependencies once:

```bash
just web-deps
```

## Running

```bash
just dev
```

This runs `pmhvd` on <http://127.0.0.1:8640> and the Vite dev server (the URL it prints), which proxies `/api` to `pmhvd`. In another terminal, the CLI talks to the same daemon:

```bash
cargo run -p pmhv -- host
```

`pmhvd` reads `config.toml` from its data directory (`~/Library/Application Support/pmhv` on macOS, `~/.local/share/pmhv` on Linux) and uses the defaults when there isn't one:

```toml
[http]
listen = "127.0.0.1:8640"
```

To keep development state separate, point both binaries elsewhere with `PMHV_DATA_DIR` and `PMHV_RUNTIME_DIR` (or `--data-dir` and `--runtime-dir`). The runtime directory holds the control socket, and unix socket paths are limited to about 100 bytes, so keep it short.

The HTTP API has no authentication until milestone 4, so don't change `listen` away from localhost before then.

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
| `just test` | Tests (none yet; see below) |
| `just licences` | REUSE check on tracked files |

Before calling a change done: `just fmt lint check build`. CI runs the same on macOS and Linux, and also fails if `just generate` would change anything, so run it after touching API types.

Debug builds of `pmhvd` read `web/dist` from disk at runtime, so `just build-web` is enough to pick up UI changes without rebuilding Rust. Release builds compile the UI in. Without a built UI, `pmhvd` still runs and answers UI requests with a 503 explaining what to build.

A `just sign` recipe, for the Virtualization.framework entitlement, arrives with that backend.

## Testing

Deferred until after the initial buildout (milestone 3 in the [roadmap](roadmap.md)), once the shape of the code has settled. Then:

- Table-driven unit tests for the pure parts: the QEMU command-line builder, spec validation against capabilities, config parsing, path confinement.
- A fake QMP server to test the QEMU driver's control flow without running QEMU.
- Integration tests that boot a tiny guest (a minimal Linux kernel and initramfs) under QEMU, on macOS (HVF) and Linux (KVM where available, TCG otherwise), driven through the CLI.
- VZ integration tests on macOS only, run locally; CI runners may not allow nested virtualisation.

## Conventions

- Commits follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), with the crate or `web` as the scope where it helps (`feat(qemu): …`).
- Prose and documentation use New Zealand English; code identifiers use US English.
- Licensing metadata follows [REUSE](https://reuse.software/): `REUSE.toml` covers the whole repository, and `just licences` should pass.
