# Security policy

**Language:** English | [Français](docs/fr/SECURITY.md)

Do **not** open a public GitHub issue for a security report.

## How to report

Email **loic.peaudecerf@proton.me** with subject `Akasha OS — security`.

Include: Preview version (`VERSION` in the install prefix), host OS, what
you did, what happened, and whether a capability, secret, or network
egress is involved. Do not attach live vault keys.

## In-app Feedback

The Feedback tab accepts category **security**. Those reports are **not**
published as GitHub issues. A local copy is written under
`var/feedback/`. Prefer email if the finding is sensitive.

## Scope

In scope: capability bypass, secret leakage, unsigned module install,
egress when network is off, audit-log tampering, privilege of
`aos-bridged` beyond loopback.

Out of scope: seL4 / bare-metal (not in the public Preview zip), third-party
models and MCP servers you added, issues that require a local attacker who
already owns the user account unless they escalate a cap.

## Attribution

A fix will credit you in the release notes if you want that. There is no
bug bounty yet.
