# Phase P10 — Preview 0.10.0

**Langue :** [English](../../phases/phase-preview-10.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.10.0** : reste d’**Horizon B** sur l’hôte —
enveloppe vault **E7 TPM**, daemon bridge HTTP sibling **E8 live**,
chemin multi-GPU **E9 / P5.2** — plus **polish Media/UX** (documenter et
durcir la profondeur studio post-0.9). En parallèle, une **voie interne
d’intégration seL4** (`sel4-pv-0.10.0` + gate CI QEMU) qui **n’entre pas**
dans le zip Preview ni `latest.json`.

Dépend de P09 (E18 + E19). Pas de nouveau numéro de gate P6. Pas fer nu.
Pas fermeture cohorte PC. Pas scellage PCR / attestation.

Priorités : [plan-evolutions.md](../plan-evolutions.md) reste Horizon B.
Séquençage : P10.12 tôt ∥ P10.5–P10.8 ∥ P10.1–P10.4 ∥ P10.9–P10.11 ∥
P10.S* → P10.13–P10.16.

## Versioning (deux canaux)

| Canal | Tag | Produit |
|-------|-----|---------|
| Preview public | `v0.10.0` + workspace `0.10.0` | Zips Win/Linux + `latest.json` |
| seL4 interne | `sel4-pv-0.10.0` (jamais `v*`) | Artefacts CI seulement (loader + log) |

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P10.1 | E7 TPM | `MasterBackend::Tpm` via scellage Platform Crypto Win `NCrypt` (blob `TPM2`) ; Linux keyring/fichier tant que tpm2 n’est pas branché ; legacy `TPM1` migré | fait |
| P10.2 | E7 TPM | Préférer TPM → keyring → file ; overrides env ; `master.backend=tpm` | fait |
| P10.3 | E7 TPM | Migration one-shot keyring/file → TPM si hardware ; `vault.enc` inchangé | fait |
| P10.4 | E7 TPM | Tests skip-if-no-TPM ; TESTER/Settings ; agents toujours refusés sur `secrets.get` | fait |
| P10.5 | E8 live | Crate `aos-bridge` → `aos-bridged` : HTTP loopback `/v1`, JSON↔CBOR via bus | fait |
| P10.6 | E8 live | Health + cœur `mem.*` + `secrets.list` ; `secrets.get`/`set` style service (403 agents) | fait |
| P10.7 | E8 live | `X-Aos-From` par requête → `Intent.from` | fait |
| P10.8 | E8 live | Lancement opt-in ; smoke ; maj sibling-bridge.md | fait |
| P10.9 | E9 | `LoadOptions` + llama `tensor_split` / liste devices (pipeline couches) | fait (chemin 0.10) |
| P10.10 | E9 | `HardwareProfile` multi-GPU + partition Placement Manager | fait (chemin 0.10) |
| P10.11 | E9 | `aos-gate-p5` : vrai device count ; pass/fail si ≥2 GPU, skip si 1 | fait (skip sur 1 GPU) |
| P10.12 | Media | Hygiene UI : strokes `_f32` ; brancher `metrics_ram` / `metrics_disk` / `models_media_packs` | fait |
| P10.13 | Media | FEATURES / TESTER / site : upscale, composition, expert ; Wan/LTX expérimental | fait |
| P10.14 | Media | Pas d’img2img/inpaint first-class ; pas de surface vidéo produit | fait |
| P10.S1 | seL4 | Documenter public vs `sel4-pv-*` | fait |
| P10.S2 | seL4 | Workflow CI gate QEMU → `AOS_GATE_VM_PASS` | fait |
| P10.S3 | seL4 | Tag `sel4-pv-0.10.0` (indépendant de `v0.10.0`) | fait |
| P10.15 | Docs | phase-preview-10 + STATUS/FEATURES/TESTER/roadmap | fait |
| P10.16 | Ship | Version `0.10.0` ; tag `v0.10.0` quand gates hôte verts | fait |

## Gates de sortie

| Gate | Critère |
|------|---------|
| E7 | `master.backend=tpm` sur hôte TPM ; fallback sinon ; restart survit ; agent `secrets.get` refusé |
| E8 | `aos-bridged` opt-in ; health + `mem.context` OK ; secrets agent → 403 ; bind hors loopback refusé |
| E9 | Code path + skip sur 1 GPU ; sur ≥2 GPU, load layer-split + stream tokens |
| Media | Warnings float/dead_code ciblés absents ; TESTER upscale + composition |
| seL4 | CI ou run local `AOS_GATE_VM_PASS` ; `sel4-pv-0.10.0` ne touche pas `latest.json` |
| Régression | `cargo test --workspace` ; gates p4/p5 |
| Packaging | Deux zips + `latest.json` pour `v0.10.0` uniquement |

## Hors scope

Fermeture cohorte PC, macOS, winget/apt, PCR / attestation, merge sibling,
assistant-as-module, img2img/inpaint first-class, vidéo produit, STT / mic
24/7, E12 / E13, PV.4+, P5.3 AccelDevice, marketplace public, inventer P6.

## Suite

Après 0.10 : fermeture cohorte PC quand les testeurs sont prêts ; Horizon C /
PV.4+ quand planifié ; E9 « hard green » seulement après un run 2 GPU documenté.
