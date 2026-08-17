# Pont sibling — Akasha OS ↔ assistant Akasha

**Langue :** [English](../sibling-bridge.md) | Français

> Date : 17/08/2026 · Preview **0.5.0**  
> Statut : gel de contrats (documentation seule)  
> Lié : [paysage-concurrentiel.md](paysage-concurrentiel.md), [plan-evolutions.md](plan-evolutions.md) E8

## Principe

**Ne pas fusionner** [azerothl/akasha](https://github.com/azerothl/akasha)
(daemon assistant) et **akasha-os** en un seul binaire. Aligner les schémas
pour éviter la duplication aveugle ; « assistant as module » reste une option
plus tard.

## Contrats OS (source de vérité dans ce dépôt)

| Zone | Schéma live | Notes |
|------|-------------|-------|
| Intents / IPC | [`crates/aos-proto`](../../crates/aos-proto/src/lib.rs) + bus CBOR | Intents typés ; `Intent.from` = identité appelant |
| Mémoire | `mem.*` + relations E6 + E14 `mem.extract` | JSONL épisodique + `relations.jsonl` ; extract chat→LT opt-in |
| Secrets | `secrets.get` / `set` / `list` | Vault chiffré ; brut réservé aux services |
| ABI WASM | [`module_rt`](../../crates/aos-platform/src/module_rt.rs) + SDK | Dual-surface ; `secrets.get` interdit au guest |

## Non-objectifs

- Processus partagé ou installateur unique
- Canaux type OpenClaw dans le cœur OS
- Copier le code sibling dans ce dépôt

## Suite (post-0.5)

1. Exporter un JSON Schema / CBOR IDL des intents mémoire + secrets (incl. `mem.extract`).
2. Documenter un adaptateur HTTP ↔ bus seulement si un pont est planifié.
3. Option : façade assistant en module dual-surface une fois les ABI stables.
