# Illustration Studio release lockfile (#444)

The non-publishing full Preview validation at source `2256386` failed on macOS
in run `41527122297`, job `125557622430`, after native compilation, when
`modules/build-illustration-studio.sh` built the WASM module with `--locked`.
Cargo returned 101: the module's checked-in lockfile recorded the local
`aos-scene` dependency at **0.18.0**, while the workspace manifest provides
**0.19.0**.

The same locked command failed locally before the change. Updating only that
path package offline changes one version line; no external dependency version
changes. The exact release WASM command then completed successfully on Windows:

```text
cargo build --manifest-path modules/illustration-studio/Cargo.toml --target wasm32-unknown-unknown --release --locked
```

The existing Illustration Studio CI matrix now uses `--locked` on Linux,
Windows and macOS. It previously omitted that flag, which let its builds update
the stale lock silently and pass while release packaging failed later.

Hashes of the failure/success logs, lockfile and generated WASM are in
`evidence/2026-11-02-illustration-lockfile.json`. This local WASM result does not
complete a full macOS package, Metal inference or installed UI validation.
No generated package/catalogue files are committed. The full release baseline
and #439, #444, #247, #460, #434 and #435 remain open; no release was published.
