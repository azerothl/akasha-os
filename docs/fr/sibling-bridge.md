# Pont sibling — Akasha OS ↔ assistant Akasha

**Langue :** [English](../sibling-bridge.md) | Français

> Date : 23/08/2026 · Preview **0.11.0**  
> Statut : adaptateur live (`aos-bridged`) avec parité `mem.*` complète + export JSON Schema  
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
| Secrets | `secrets.get` / `set` / `list` | Vault chiffré ; clé maître TPM/keyring/fichier ; brut réservé aux services |
| ABI WASM | [`module_rt`](../../crates/aos-platform/src/module_rt.rs) + SDK | Dual-surface ; `secrets.get` interdit au guest |

## Mapping HTTP JSON ↔ intents CBOR

La Preview **0.11.0** livre le binaire optionnel séparé `aos-bridged`
(`crates/aos-bridge`) dans `bin/` du zip. Il **n’est pas** démarré par
`aos-session`. Bind toujours `127.0.0.1` (`AOS_BRIDGE_PORT`, défaut
`24710`). Bus : `AOS_BUS_ADDR` (défaut `127.0.0.1:24701`). Smoke :
`.\demo\smoke-bridge.ps1`.

- URL de base : `http://127.0.0.1:<bridge-port>/v1`
- Corps requête : JSON conforme à [`docs/bridge/`](../bridge/)
- Corps réponse : JSON du même type `aos-proto` que le bus encoderait en CBOR
- Identité : en-tête `X-Aos-From` → `Intent.from` (défaut `service:bridge` ;
  ne **pas** faire confiance à un champ JSON `actor` s’il diverge)
- Erreurs : HTTP 403 si `Intent.from` est un agent et l’intent est `secrets.get`

Live : toutes les routes du tableau (parité `mem.*` + secrets).

| HTTP | Intent | Schéma JSON (`$defs`) | Notes |
|------|--------|----------------------|-------|
| `GET /health` | — | — | Vivacité process (pas de bus) |
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
| `POST /mem/extract` | `mem.extract` | `MemExtractRequest` | Peut appeler infer local |
| `POST /mem/relate` | `mem.relate` | `MemRelateRequest` | |
| `POST /mem/unrelate` | `mem.unrelate` | `MemUnrelateRequest` | |
| `POST /mem/neighbors` | `mem.neighbors` | `MemNeighborsRequest` | |
| `POST /mem/list` | `mem.list` | `MemListRequest` | |
| `POST /mem/update` | `mem.update` | `MemUpdateRequest` | |
| `POST /secrets/get` | `secrets.get` | `SecretGetRequest` | Services seulement ; 403 pour les agents |
| `POST /secrets/set` | `secrets.set` | `SecretSetRequest` | `value` vide = suppression |
| `POST /secrets/list` | `secrets.list` | `SecretListRequest` | Noms seulement ; Réponse : `SecretListResponse` |

Note fil : les daemons OS parlent **CBOR**. L’adaptateur transcode JSON ↔ CBOR
1:1 avec les mêmes noms de champs serde. Ne pas inventer un second dialecte.

## Non-objectifs

- Processus partagé ou installateur unique
- Plier HTTP dans `aos-session` / `aos-platformd`
- Canaux type OpenClaw dans le cœur OS
- Copier le code sibling dans ce dépôt
- Façade « assistant as module » (ABI stables)

## Suite

1. Option : façade assistant en module dual-surface une fois les ABI stables.
