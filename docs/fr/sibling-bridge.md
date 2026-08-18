# Pont sibling — Akasha OS ↔ assistant Akasha

**Langue :** [English](../sibling-bridge.md) | Français

> Date : 18/08/2026 · Preview **0.8.0**  
> Statut : gel de contrats + export JSON Schema (pas de daemon live)  
> Lié : [paysage-concurrentiel.md](paysage-concurrentiel.md), [plan-evolutions.md](plan-evolutions.md) E8, [docs/bridge/](../bridge/)

## Principe

**Ne pas fusionner** [azerothl/akasha](https://github.com/azerothl/akasha)
(daemon assistant) et **akasha-os** en un seul binaire. Aligner les schémas
pour éviter la duplication aveugle ; « assistant as module » reste une option
plus tard.

## Contrats OS (source de vérité dans ce dépôt)

| Zone | Schéma live | Notes |
|------|-------------|-------|
| Intents / IPC | [`crates/aos-proto`](../../crates/aos-proto/src/lib.rs) + bus CBOR | Intents typés ; `Intent.from` = identité appelant |
| Export JSON Schema | [`docs/bridge/`](../bridge/) | Vue JSON draft-07 des payloads `mem.*`, `secrets.*` et E15 `declarative_ui` |
| Mémoire | `mem.*` + relations E6 + E14 `mem.extract` | JSONL épisodique + `relations.jsonl` ; extract chat→LT opt-in |
| Secrets | `secrets.get` / `set` / `list` | Vault chiffré ; clé maître dans le keyring OS (fallback fichier 0600) ; brut réservé aux services |
| ABI WASM | [`module_rt`](../../crates/aos-platform/src/module_rt.rs) + SDK | Dual-surface ; `secrets.get` interdit au guest |

## Mapping HTTP JSON ↔ intents CBOR (contrat seulement)

La Preview **0.8.0 n'exécute pas** d'adaptateur HTTP dans le process OS. Un
futur daemon de pont (binaire séparé, optionnel) PEUT exposer du JSON HTTP
et relayer vers le bus CBOR. Jusqu'à ce daemon, ce tableau est le gel.

Surface d'adaptateur supposée (non livrée) :

- URL de base : `http://127.0.0.1:<bridge-port>/v1`
- Corps requête : JSON conforme à [`docs/bridge/`](../bridge/)
- Corps réponse : JSON du même type `aos-proto` que le bus encoderait en CBOR
- Identité : en-tête `X-Aos-From` → `Intent.from` (ne **pas** faire confiance
  à un champ JSON `actor` s'il diverge)
- Erreurs : HTTP 403 si `Intent.from` est un agent et l'intent est `secrets.get`

| HTTP | Intent | Schéma JSON (`$defs`) | Notes |
|------|--------|----------------------|-------|
| `POST /mem/working_set` | `mem.working_set` | `MemWorkingRequest` | |
| `POST /mem/working_get` | `mem.working_get` | `MemWorkingRequest` | `messages` ignoré |
| `POST /mem/episodic_write` | `mem.episodic_write` | `MemEpisodicWriteRequest` | |
| `POST /mem/episodic_query` | `mem.episodic_query` | `MemEpisodicQueryRequest` | |
| `POST /mem/episodic_delete` | `mem.episodic_delete` | `MemEpisodicDeleteRequest` | |
| `POST /mem/stats` | `mem.stats` | (objet vide) | Réponse : `MemStats` |
| `POST /mem/shared_read` | `mem.shared_read` | `MemSharedReadRequest` | |
| `POST /mem/shared_write` | `mem.shared_write` | `MemSharedWriteRequest` | |
| `POST /mem/user/remember` | `mem.user.remember` | `MemUserRememberRequest` | Réponse : `MemRememberResponse` |
| `POST /mem/user/recall` | `mem.user.recall` | `MemUserRecallRequest` | |
| `POST /mem/context` | `mem.context` | `MemContextRequest` | Réponse : `MemContextResponse` |
| `POST /mem/extract` | `mem.extract` | `MemExtractRequest` | Réponse : `MemExtractResponse` |
| `POST /mem/relate` | `mem.relate` | `MemRelateRequest` | |
| `POST /mem/unrelate` | `mem.unrelate` | `MemUnrelateRequest` | |
| `POST /mem/neighbors` | `mem.neighbors` | `MemNeighborsRequest` | |
| `POST /mem/list` | `mem.list` | `MemListRequest` | |
| `POST /mem/update` | `mem.update` | `MemUpdateRequest` | |
| `POST /secrets/get` | `secrets.get` | `SecretGetRequest` | Services seulement ; 403 pour les agents |
| `POST /secrets/set` | `secrets.set` | `SecretSetRequest` | `value` vide = suppression |
| `POST /secrets/list` | `secrets.list` | `SecretListRequest` | Noms seulement ; Réponse : `SecretListResponse` |

Note fil : les daemons OS parlent **CBOR**. L'adaptateur transcode JSON ↔ CBOR
1:1 avec les mêmes noms de champs serde. Ne pas inventer un second dialecte.

## Non-objectifs

- Processus partagé ou installateur unique
- Daemon HTTP live dans `aos-session` / `aos-platformd` en Preview 0.6
- Canaux type OpenClaw dans le cœur OS
- Copier le code sibling dans ce dépôt
- Façade « assistant as module » (post-0.6, ABI stables)

## Suite (post-0.6)

1. Implémenter un daemon de pont **séparé** seulement s'il est planifié — ne
   pas plier HTTP dans les services noyau OS.
2. Option : façade assistant en module dual-surface une fois les ABI stables.
