# Technical failure — relative output path

The old compiled runner supplied a relative PNG path to sd-cli, which runs from
its own engine directory. sd-cli logged successful generation, but the expected
workspace PNG was absent. The runner correctly reported an error. No visual
acceptance or finished five-pass result.

Run 02 uses an absolute output path and preserves this failed run. The current
source resolves the pass output directory to an absolute path before invoking
the backend; a unit test covers absolute output and reference publication.
