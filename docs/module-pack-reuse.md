# Signed module pack reuse (#445 / #446)

This source candidate extends the already merged optional-pack installer and
publisher (#448/#449). Release builds can restore exact previously published
WASM archives before entering per-module build blocks. Native CUDA/llama.cpp
compilation is unaffected.

## Build modes

`AOS_MODULE_REUSE_MODE` or the manual release input `module_reuse_mode`:

| Mode | Behaviour |
|---|---|
| `off` | No release lookup; rebuild all packs. Local-script default. |
| `prefer` | Restore an exact matching signed artifact; rebuild changed/missing inputs. Workflow default, including the initial publication. |
| `require` | Every module must resolve to a signed compatible published pack; a miss aborts. |

All modes retain final manifest, catalogue and signature checks. A matching
but corrupt, unavailable or incompatible artifact is fatal even in `prefer`.
There is no substitution of a different input fingerprint. `require` is the
mode for auditing a release expected to reuse every module.

The workflow publishes six official module archives and a signed platform
index (`aos-module-catalogue-<platform>.yaml` plus `.sig`). Only the configured
optional modules (Canvas, Create and Illustration Studio in the release
workflow) are omitted from the base package. Notes, Tasks and ext-rt remain
bundled, while also receiving reusable archives.

## Inputs and validation

Fingerprint schema `aos-module-build-inputs/v2` covers guest sources and own
Cargo lockfile, SDK, source templates, build recipes, compiler/Cargo versions,
WASM target, relevant flags and host API. Illustration additionally includes
its host aos-scene dependency and workspace metadata. Independent guest
workspaces do not inherit the host lockfile: changing only the host release
version does not invalidate those packs. Guest builds use `--locked`.

Generated packages/WASM are excluded from the input key. Canvas's manifest
and UI are source templates; its generated hash field is normalized. Text
line endings and the compiler host line are normalized, but archives remain
platform-specific. All three runners can restore their previous release
archives; no claim is made that independently compiled WASM bytes match across
operating systems.

Restoration looks through the 30 most recent published releases. It verifies
the index against the locally pinned Ed25519 key, then platform, API and explicit
empty dependencies, input key, ZIP length/hash, safe bounded extraction,
manifest name/version/WASM hash and required UI. Traversal, symlinks, duplicate
paths, Windows device names and ambiguous paths are refused. ZIPs are bounded
to 64 MiB compressed, 128 MiB extracted, and 4,096 entries.

The previous build package is retained until staging validates. Restored ZIP
bytes are preserved without recompression. Final catalogue metadata is signed
and checked using the existing release path. Publishing refuses a same-name
artifact with different bytes. The Preview development catalogue key retains
its existing development trust scope; this is not a new production PKI.

`gh` needs release read access for index discovery. Downloads use public HTTPS
release URLs; the GitHub token is not forwarded to asset redirects. First-time
or changed-input builds must use `prefer`/`off`, publish their validated packs,
then a later identical-input run can use `require`.

## Evidence and measurement

`dist/module-reuse/report.json` records module, input key, restore/build
decision, source release, archive hash and lookup/restore time.
`restored.txt` controls the build skips. The release-cost JSON and Markdown
include those decisions alongside existing phase timings.

Local contract tests cover actual signing, archive validation/restoration,
corruption, input invalidation and rollback on Windows and Ubuntu WSL. The
three-platform CI matrix runs these contracts after publication of this branch.
The local tests are **not** a measured full release comparison.

For acceptance, run a publication with new inputs, then an identical-input
`require` run on Windows/Linux/macOS. Compare native compilation, toolchain
installation, WASM build, restore/verification and packaging with
[the baseline](release-cost-baseline.md). No percentage or duration saving is
claimed before these runs. Optional WASM extraction also does not remove the
native engines or Illustration asset libraries from the core distribution.

The actual installer/cache tests validate a pack after reopening its cache and
reject invalid upgrades without replacing the previous version. Installed
UI/download/remove/data-retention evidence on all three platforms remains
necessary for #445; #434's real first-install cohort remains separate.
