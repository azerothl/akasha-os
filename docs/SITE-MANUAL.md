# Site manual vs repo docs (single source of truth)

**Language:** English | [Français](fr/SITE-MANUAL.md)

> Preview public docs stay **static HTML** in `website/` (PRODUCT stack).
> This page defines which surface is canonical when HTML and Markdown overlap.
> Machine-readable twin: [`packaging/site-docs-map.json`](../packaging/site-docs-map.json).
> Guard: `./packaging/check-site-docs.ps1` (also in workspace-ci).

## Rules

1. **Do not introduce Docusaurus / MD→HTML for the public site** unless PRODUCT
   revisits the static HTML/CSS/JS decision.
2. **Edit the SoT first**, then update the twin (offline MD) or digest (short HTML).
3. **French on the site** uses inline `data-lang` in the same HTML file.
   **French in the repo** lives under `docs/fr/` (some filenames differ —
   see [I18N.md](I18N.md)).
4. When a site chapter links to repo depth, bilingual pages must offer the
   `docs/fr/…` mirror when it exists.

## HTML is source of truth (public how-to)

| Site chapter | Offline twin (release zip / GitHub) |
|--------------|-------------------------------------|
| [`website/install.html`](../website/install.html) | [`INSTALL.md`](INSTALL.md) · [`fr/INSTALL.md`](fr/INSTALL.md) |
| [`website/docs/first-run.html`](../website/docs/first-run.html) | [`FIRST-RUN.md`](FIRST-RUN.md) · [`fr/FIRST-RUN.md`](fr/FIRST-RUN.md) |
| [`website/docs/skill.html`](../website/docs/skill.html) | [`write-a-skill.md`](write-a-skill.md) · [`fr/write-a-skill.md`](fr/write-a-skill.md) |
| [`website/docs/module.html`](../website/docs/module.html) | [`write-a-module.md`](write-a-module.md) · [`fr/write-a-module.md`](fr/write-a-module.md) |
| [`website/docs/feedback.html`](../website/docs/feedback.html) | [`TESTER.md`](TESTER.md) (short path + long checklist) |
| [`website/community.html`](../website/community.html) | [`community.md`](community.md) |
| [`website/docs/use.html`](../website/docs/use.html) | — (depth: [`FEATURES.md`](FEATURES.md)) |
| [`website/docs/network.html`](../website/docs/network.html) | — (site-only) |
| [`website/docs/troubleshoot.html`](../website/docs/troubleshoot.html) | — (site-only) |
| [`website/docs/limits.html`](../website/docs/limits.html) | — (site-only) |
| [`website/docs/whats-new.html`](../website/docs/whats-new.html) | — (depth: FEATURES) |

Offline twins must say **Canonical:** and link the live site path. Prefer
updating HTML, then mirroring steps into MD for the zip.

## Markdown is source of truth (developer digests)

Short bilingual HTML points at the contract; edit the MD first.

| Digest HTML | Canonical MD |
|-------------|--------------|
| [`module-sdk.html`](../website/docs/module-sdk.html) | [`module-sdk.md`](module-sdk.md) · [`fr/module-sdk.md`](fr/module-sdk.md) |
| [`lan-cluster.html`](../website/docs/lan-cluster.html) | [`lan-cluster.md`](lan-cluster.md) · [`fr/cluster-lan.md`](fr/cluster-lan.md) |
| [`rich-apps.html`](../website/docs/rich-apps.html) | [`rich-app-contract.md`](rich-app-contract.md) (+ create-contract / ADR 0009) |
| [`devices.html`](../website/docs/devices.html) | [`device-capture.md`](device-capture.md), [`device-usb.md`](device-usb.md) (EN only today) |
| [`build.html`](../website/docs/build.html) | [`INSTALL.md`](INSTALL.md)#build-from-source |

Specs, ADRs, phases, and tickets stay **repo-only** under `docs/` — never
duplicated as full site chapters.
