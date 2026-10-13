# Issues

Bugs and rough edges found while building pmhv, in pmhv itself and in projects it depends on, so they can be fixed at the source. Unfinished features are tracked in the [roadmap](roadmap.md#status) instead.

Each entry says what happens, why, what pmhv does about it for now, and what a proper fix looks like. Mark an entry *fixed* (with the commit or release) rather than deleting it, until pmhv's workaround is gone too.

## @swopstar/react-ui

### Compiled utilities lose to the consumer's Tailwind output

*Open.* Found 2026-10-14, react-ui 0.2606.2.

- **What happens:** in a consuming app, the sidebar's menu button does nothing on desktop. The desktop sidebar is permanently `display: none`.
- **Why:** react-ui ships `style.css` with its Tailwind utilities already compiled, and consumers run Tailwind again for their own classes. Both end up in the same `utilities` cascade layer, with the app's copy later. When both emit the same utility, the app's copy wins regardless of Tailwind's intended order. Here the app used `hidden` (on a file input), and its `.hidden` came after react-ui's `md:block` on the sidebar wrapper (`hidden md:block`). Any responsive or state variant in react-ui can be overridden this way by an unrelated class in the app.
- **Workaround in pmhv:** `web/src/index.css` adds `@source "../node_modules/@swopstar/react-ui/dist";`, so the app's build contains every utility either side uses, in Tailwind's order (commit `ae52679`).
- **Upstream fix:** ship the theme (tokens, `@theme inline`, base styles) as a CSS entry point, separately from compiled utilities, and document that consumers `@source` the library. Or keep shipping compiled utilities but in their own layer, ordered before the app's. swoptape and swopcart are exposed to the same bug as soon as they use a class react-ui also uses with a variant.

### The stylesheet is 1.1 MB

*Open, known.* Found 2026-10-13.

- **What happens:** the app's CSS bundle is about 1.1 MB (780 kB gzipped), almost all of it from `@swopstar/react-ui/style.css`.
- **Upstream fix:** planned in react-ui. Probably the same change as above: if consumers `@source` the library, their own build only emits the utilities actually used.

### README and package metadata point at GitHub Packages

*Open.* Found 2026-10-13.

- **What happens:** the README says to add `@swopstar:registry=https://npm.pkg.github.com` to `.npmrc`, and `package.json` has `publishConfig.registry` set to GitHub Packages, but the package is published on the public npm registry (0.2606.2 is there), and that's what pmhv installs from.
- **Upstream fix:** update the README's installation section and `publishConfig` to match where it's actually published. The local checkout (`~/src/swopstar/react-ui`) is at 0.2606.1, behind the published 0.2606.2, which suggests a commit or tag hasn't been pushed.

### Outdated lucide-react means two copies in consumers

*Open.* Found 2026-10-13.

- **What happens:** react-ui depends on `lucide-react ^0.511.0`; current lucide-react is 1.x (1.56.0). An app using current lucide-react bundles both versions.
- **Upstream fix:** move to lucide-react 1.x, or make it a peer dependency so the app's copy is used.

### `toast` isn't exported alongside `Toaster`

*Open.* Found 2026-10-14.

- **What happens:** react-ui exports `Toaster` but not sonner's `toast`, so showing a toast means depending on `sonner` directly and relying on npm deduplicating it to the same copy react-ui uses. Two copies would mean toasts silently never appear.
- **Workaround in pmhv:** depends on `sonner` directly; `npm ls sonner` shows it deduped.
- **Upstream fix:** re-export `toast` from react-ui (or make `sonner` a peer dependency).

## Third-party

### @types/novnc__novnc declares a module path noVNC no longer has

*Open.* Found 2026-10-14, @types/novnc__novnc 1.6.0 with @novnc/novnc 1.7.0.

- **What happens:** the types declare `@novnc/novnc/lib/rfb`, but noVNC 1.7 ships `core/rfb.js` as its package main and has no `lib/` directory.
- **Workaround in pmhv:** `web/src/types/novnc.d.ts` maps `@novnc/novnc` to the declared types.
- **Upstream fix:** update the DefinitelyTyped package for noVNC 1.7 (`declare module "@novnc/novnc"`), or have noVNC ship its own types.

### rust-embed's `allow_missing` hides a wrong folder path

*Open.* Found 2026-10-13, rust-embed 8.13.0.

- **What happens:** `#[folder = "$CARGO_MANIFEST_DIR/../../web/dist"]` isn't expanded without the `interpolate-folder-path` feature, so it's treated as a relative path that doesn't exist. With `#[allow_missing = true]`, that's silently accepted and nothing is embedded.
- **Workaround in pmhv:** use the relative path `../../web/dist`, which rust-embed resolves against the manifest directory anyway.
- **Upstream fix:** when `allow_missing` is set and the path contains `$` without `interpolate-folder-path`, emit a compile-time warning or error, as it already does in the non-`allow_missing` case.

### `reuse lint` scans gitignored `node_modules`

*To investigate.* Found 2026-10-13, reuse 6.2.0.

- **What happens:** `reuse lint` at the repository root reported a licence from `web/node_modules/type-fest/readme.md`, although `/web/node_modules` is in `.gitignore` and REUSE is meant to respect VCS ignores.
- **Workaround in pmhv:** `just licences` runs `reuse lint-file` on `git ls-files` output instead.
- **Next step:** reproduce in a minimal repository (perhaps nested `.gitignore` files, or the `**` annotation in `REUSE.toml`) before reporting.

## pmhv

### A killed shim leaves QEMU running

*Open.* Found 2026-10-13.

- **What happens:** QEMU is a child of `pmhv-shim`. If the shim is killed (rather than exiting normally), QEMU keeps running with nobody supervising it, and pmhvd reports the machine as stopped.
- **Fix:** on Linux, `PR_SET_PDEATHSIG` on QEMU; on macOS, have QEMU watch a pipe from the shim, or have pmhvd find orphaned QEMU processes (by pidfile in the runtime directory) and offer to stop or adopt them.

### Text thumbnails run lines together

*Open.* Found 2026-10-13.

- **What happens:** firmware that moves the cursor with escape sequences instead of printing newlines (EDK2's countdown, for example) ends up as one long line, because the thumbnail strips escapes without interpreting them.
- **Fix:** interpret cursor movement (at least `CSI H`, `CSI G`, `CSI K`) into a small screen buffer, or render the snapshot from the shim's scrollback with a terminal emulator crate.

### Failed sign-ins are only slowed down

*Open.* Found 2026-10-13.

- **What happens:** each failed sign-in waits 750 ms, but nothing limits attempts made in parallel.
- **Fix:** limit attempts per client address (and overall) before exposing pmhvd beyond localhost.

### Jobs are forgotten on restart

*Open.* Found 2026-10-14.

- **What happens:** image conversions are tracked in memory. If pmhvd restarts mid-conversion, the job is forgotten: `qemu-img` either dies with pmhvd (Ctrl+C reaches the whole process group) or finishes into the hidden partial file, which is then never renamed into place. Either way a hidden `.*.convert` file is left in the image group.
- **Fix:** clean up stale `.*.convert` and `.*.upload` files at startup, and run conversions under their own process group (like shims) if they should survive restarts.
