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
