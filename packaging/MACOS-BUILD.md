# macOS Apple Silicon Preview — build & release notes

**Language:** English | [Français](../docs/fr/packaging-MACOS-BUILD.md)

Apple Silicon only (`arm64`). Intel Mac is **not** a Preview target.

## One-shot local build (Loïc / maintainer)

On an **Apple Silicon** Mac with Xcode CLI tools and Rust stable:

```bash
git clone https://github.com/azerothl/akasha-os.git
cd akasha-os
rustup target add wasm32-unknown-unknown
chmod +x packaging/build-preview-macos.sh packaging/install-macos.sh
SKIP_MODELS=1 ./packaging/build-preview-macos.sh
cd dist
zip -r "AgentOS-Preview-$(tr -d '[:space:]' < ../VERSION)-macos-arm64.zip" \
  "AgentOS-Preview-$(tr -d '[:space:]' < ../VERSION)-macos-arm64"
```

Tester install (one gesture):

```bash
unzip AgentOS-Preview-*-macos-arm64.zip
cd AgentOS-Preview-*-macos-arm64
./install.sh
agentos-preview
```

Data prefix: `~/.local/share/agentos-preview` (same as Linux).

## CI

Tag `v*` triggers [`.github/workflows/preview-release.yml`](../.github/workflows/preview-release.yml)
which builds Win + Linux + macOS and publishes release assets.

**Backfill macOS on an existing Release** (e.g. `v0.12.0` already tagged):

1. Actions → **preview-release** → **Run workflow**
2. Branch: `main` (or the branch with packaging scripts)
3. Inputs: `macos_only` = true, `release_version` = `0.12.0`, `upload_release` = true
4. Run — builds on `macos-14`, uploads `AgentOS-Preview-0.12.0-macos-arm64.zip`, merges macOS into `latest.json` on that Release (Win/Linux entries preserved)

Tag push builds:

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `AgentOS-Preview-<ver>-macos-arm64.zip` (Apple Silicon + Metal; CI unsigned)

Unified artefact ships **Metal** `aos-modeld` + **CPU** `aos-modeld-cpu` (same session logic as Win/Linux).

**Expected size:** the macOS zip is much smaller than Win/Linux (~30 MB compressed vs ~800 MB) because GGUF models download on first run (`SKIP_MODELS=1` in CI) and Metal binaries do not bundle CUDA runtime DLLs or multi-arch CUDA SASS. A ~30 MB zip with all `aos-*` daemons, modules, and `__ggml_metallib` in `aos-modeld` is a real Preview — not a stub.

## Codesign / notarization blockers (unsigned CI builds)

GitHub Actions produces **unsigned** binaries. Testers may need Gatekeeper clearance after `install.sh` (`xattr -cr` on `bin/`).

For smoother distribution outside the cohort, Loïc must run **once** on a Mac with a Developer ID:

```bash
# After build-preview-macos.sh, before zipping:
APP_ROOT="dist/AgentOS-Preview-$(tr -d '[:space:]' < VERSION)-macos-arm64"
IDENTITY="Developer ID Application: …"   # your Apple cert

for bin in "$APP_ROOT/bin"/aos-*; do
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$bin"
done

# Optional but recommended for testers: notarize the zip
ditto -c -k --keepParent "$APP_ROOT" AgentOS-Preview-macos-arm64.zip
xcrun notarytool submit AgentOS-Preview-macos-arm64.zip \
  --apple-id "…" --team-id "…" --password "…" --wait
xcrun stapler staple AgentOS-Preview-macos-arm64.zip
```

Without notarization, cohort testers can still run via `./install.sh` + `agentos-preview`.

## Product constraints

- Do **not** add Mac copy to the mill website, chat chrome, or Settings until a real downloadable zip is on GitHub Releases.
- NVIDIA/CPU remains the mill story until that zip ships publicly.
