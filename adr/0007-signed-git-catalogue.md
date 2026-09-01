# ADR 0007: Opt-in signed Git catalogue source

**Language:** English | [Français](../docs/fr/adr/0007-signed-git-catalogue.md)

> Date: 01/09/2026 · Status: **accepted**

## Context

E10 already ships a **local** signed catalogue (`share/modules/catalogue.yaml`,
ed25519, hash check, cap review on `module.install`). Horizon 1 needs a way
for people to publish a skill or `.aospkg` **outside** the Preview zip
without a paid store or a ClawHub clone.

[ADR 0006](0006-license-split.md) already split licenses: host AGPL + CLA;
`community/` default MIT, no commercial grant; a future catalogue index is
Apache-2.0 and each package keeps its license.

## Decision

Preview may load a **second, opt-in** catalogue from a Git-hosted signed
index (same YAML format). Default URL is the raw `community/catalogue.yaml`
in this repository.

- **Off by default.** Settings → Local module catalogue → enable.
- **Authenticity is the ed25519 signature** verified with the pinned Preview
  catalogue key — not “trust GitHub HTTPS”.
- **Offline-first.** A verified copy is cached under
  `var/catalogue/community/`. Boot never requires the network.
- **Install** uses the same cap review as the bundled catalogue. Tampered
  index or hash mismatch **refuses**.
- Index license: Apache-2.0. Each package keeps its own license field.

This is not a public marketplace, not paid, and not Discord/Telegram in
the OS core.

## Consequences

- Authors PR under `community/` (sign the index; list caps; no `crates/`).
- A later dedicated catalogue repo is allowed; it must keep the same
  signed-index + cap-review contract.
