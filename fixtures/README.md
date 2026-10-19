# Project-policy quality corpus

`workspace-policy-quality.json` contains 74 synthetic cases, created for #247.
No case contains a real credential. Shapes and long literals intentionally
exercise the production filter; a detection is not evidence of a live secret.

Each case has a stable ID, category, input, `sensitive` ground-truth label and
`expected_detected` regression expectation. The labels describe the intended
scenario: a literal standing in for a secret versus benign source/example text.
Known unsupported sensitive cases deliberately have `expected_detected: false`.
Known false positives deliberately have `expected_detected: true`. Neither is
removed from precision/recall denominators. `--check` checks the recorded
behaviour, not perfect secret coverage.

This is a small, authored regression suite used while developing the fix. It
has no held-out or independently annotated sample and is not a representative
estimate of real-world recall. Variants of the nine assignment names are
correlated examples, not independent repository observations.

Run the actual Rust policy, without Python or external services:

```text
cargo run -p aos-platform --no-default-features --example workspace_policy_quality --locked -- --check
```

Output contains IDs, labels, counts, rules and line numbers, never input text or
matched values. Corpus bytes are fingerprinted. The platform unit suite also
exercises these cases through actual filesystem reads and project discovery.
The Linux Rust CI job runs the evaluator; this does not establish Windows/macOS
CI execution of the corpus or any human native-application validation.
