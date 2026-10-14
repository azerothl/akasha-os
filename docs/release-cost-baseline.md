# Preview release cost baseline

This page records observed release costs and the repeatable measurements added
to `preview-release`. It does not predict savings from future packaging changes.

## Historical observation

Run [38205079376](https://github.com/azerothl/akasha-os/actions/runs/38205079376)
on commit `d5a22ee41ed72119665df5d80cd4e02a66726c01` (2026-10-12, full
Windows/Linux/macOS Preview release):

| Platform | Job duration | `Build Preview package` step | GitHub Actions uploaded artifact |
|---|---:|---:|---:|
| Windows x64 + CUDA | 105.42 min | 94.73 min | 654,565,310 bytes (624.1 MiB) |
| Linux x64 + CUDA | 43.82 min | 41.05 min | 942,578,208 bytes (899.0 MiB) |
| macOS arm64 + Metal | 31.05 min | 29.73 min | 53,832,415 bytes (51.3 MiB) |

The three platform jobs ran in parallel; they are not additive. The whole run
took about 107 minutes, with Windows determining the wall time. Windows spent
about 5.9 minutes in CUDA Toolkit setup. The artifact byte counts above are
GitHub Actions transport artifacts, not measurements of the individual files
inside each release archive.

Observed `sccache` counters from that run:

| Platform | Hits | Misses | Hit rate | Extra observation |
|---|---:|---:|---:|---|
| Windows | 4 | 727 | 0.55% | 10 compile failures |
| Linux | 341 | 2,792 | 10.88% | CUDA compilation had no cache hits |
| macOS | 241 | 1,515 | 13.72% | — |

This was one run with caches enabled, not a controlled cold/warm pair. No
component size breakdown was collected by that older run, so it is not used to
claim which module or resource is the largest.

## Measurements on new full releases

Each platform job now writes a `release-cost-<platform>.json` report and a
GitHub Actions job summary. The report records:

- the source commit, run ID, requested cache profile, final archive byte count,
  SHA-256, and exact uncompressed bytes by shipped component, sorted largest
  first;
- module package sizes individually, Illustration Studio resources, binaries,
  skills, model catalogue data, and remaining files;
- build phase durations from the packaging scripts, archive compression, and
  package verification;
- `sccache` hit/miss counters and whether the request used the `cold` or `reuse`
  cache profile;
- whether any `.gguf` file is present. GGUFs downloaded on first launch are
  outside the Preview archive when absent from the report.

Manual full non-publishing benchmark runs also measure each component as a
separate deflated ZIP to compare possible future pack boundaries. This column is
a separate compression run; its values do not add up to the platform archive.
That comparison is disabled on normal release builds so it cannot add extra
compression work to every release. ZIP releases expose exact per-component
compressed bytes from their archive directory. The Linux `.tar.gz` format has no
per-member compressed byte count, so its report marks that field unavailable and
gives the optional separate ZIP comparison instead.

The detailed phase durations sit inside the overall `package-build` duration and
must not be added to it. `native-cpu-companion` is inside `package-assembly`.
The macOS packaging script creates its ZIP internally, so its `package-build`
duration includes the separately reported compression phase as well.

## Full cold run (2026-10-14)

Run [38602553174](https://github.com/azerothl/akasha-os/actions/runs/38602553174)
on commit `a18d910664b531a2f8d8ec9b964d121099090363`, dispatched with
`macos_only=false`, `include_macos_in_full_build=true`,
`create_release=false`, `upload_release=false`, and `cache_profile=cold`.
Windows, Linux, and macOS ran in parallel; the full run wall time was 93m 12s.
Cargo cache and sccache setup were skipped on every platform. No GGUF files
were included; model weights remain first-run downloads.

| Platform | Job duration | `Build Preview package` | Release archive | Archive SHA-256 |
|---|---:|---:|---:|---|
| Windows x64 + CUDA | 93m 08s | 85m 28s | 654,562,992 bytes (624.2 MiB) | `31bcfed803b2c0455d8178a09ba750ab9321c342a39ba33f34f63a4ce77d9ff7` |
| Linux x64 + CUDA | 54m 21s | 52m 02s | 942,589,774 bytes (898.9 MiB) | `8b0e0bcce2b6a74806c2d94841bdffe6f0f9d3ecefee1251e232cb74a35f747d` |
| macOS arm64 + Metal | 31m 47s | 30m 58s | 53,839,654 bytes (51.3 MiB) | `fed0751f57f6c6fa02547d148f4a8713641c33bd117ed68e4790ebde33c21ec8` |

The phase timings below are in seconds. The `package-build` row is an outer
wrapper that contains the listed phases; do not add it to them.

| Phase | Windows | Linux | macOS |
|---|---:|---:|---:|
| `native-build` | 4,395.3 | 2,611.0 | 1,575.0 |
| `wasm-modules` | 563.8 | 170.0 | 238.0 |
| `native-cpu-companion` | 46.5 | 129.0 | 32.0 |
| `package-assembly` | 52.7 | 138.0 | 32.0 |
| `package-build` | 5,013.4 | not recorded* | 1,851.0 |
| `archive-compression` | 31.5 | not recorded* | 6.0 |
| `package-verification` | 3.7 | not recorded* | 1.0 |

*The cold Linux run recorded zero for its three outer wrapper timers because the
workflow step ran with `sh`, where Bash's `$SECONDS` variable is unavailable.
The packaging script's internal Linux phase timings and GitHub Actions step
duration remain valid. Commit `8f4e58d` changes those outer timers to Python's
monotonic clock; the next full run supplies corrected values.

The six WASM packages were small relative to the native binaries, although
their build phase was measurable on all three runners:

| Module | Windows archive ZIP | Linux standalone ZIP comparison | macOS archive ZIP |
|---|---:|---:|---:|
| Illustration Studio | 338.1 KiB | 337.8 KiB | 336.8 KiB |
| Notes | 148.6 KiB | 147.5 KiB | 147.0 KiB |
| Create | 117.8 KiB | 116.9 KiB | 116.4 KiB |
| Canvas | 109.8 KiB | 109.1 KiB | 108.6 KiB |
| Tasks | 90.1 KiB | 89.7 KiB | 89.2 KiB |
| ext-rt | 80.1 KiB | 80.4 KiB | 79.3 KiB |
| **Total** | **880.6 KiB** | **881.4 KiB** | **877.3 KiB** |

The Windows and macOS columns are exact compressed member sizes in their ZIP
release archives. Linux uses separate ZIP comparisons because tar.gz has no
per-file compressed size. The module packages represent about 0.14% of the
Windows archive, 0.10% by the Linux comparison, and 1.67% of the macOS archive.
The Illustration assets and renderer pack are additional small components. This
supports optional installation as a user choice, but does not indicate a large
base-download reduction. Reusing unchanged module builds could save the measured
WASM phase—9m 24s on Windows, 2m 50s on Linux, and 3m 58s on macOS—without
changing the much larger native build.

Standalone per-component ZIP measurement itself took 75.9s on Windows, 88.9s on
Linux, and 5.5s on macOS. The workflow now disables that extra compression on
normal release builds and keeps it on full non-publishing benchmark runs.

For a full non-publishing baseline, dispatch `preview-release` with
`macos_only=false`, `include_macos_in_full_build=true`, and
`create_release=false`. Set `cache_profile=cold` for the run without sccache or
Cargo caches; normal tag builds and the default manual profile use `reuse`. The
report includes observed hit counts so a requested profile is not mistaken for
proof that a cache hit occurred.

## Reading the baseline

Compare reports only when platform, source scope, cache profile, runner/toolchain
and release archive format match. Keep build time, CUDA/toolchain setup, module
compilation, archive compression, and artifact upload as separate observations.
The GitHub Actions step table supplies setup and upload durations; the JSON
report supplies the phases inside the packaging script. A reduction is a measured
result only after comparable reports exist before and after a pipeline change.
