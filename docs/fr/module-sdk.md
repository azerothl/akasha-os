# SDK Module / DeclUI — guide développeur

**Langue :** [English](../module-sdk.md) | Français

> Date : 11/09/2026 · Preview **0.17.0**

Guide pour contributeurs et testeurs avancés qui packagent des modules WASM avec
UI déclarative. Pour un premier module sans cargo, commencez par
[write-a-module.md](write-a-module.md). Ce document ne invente pas d’API — il
cite chemins, constantes et scripts de build livrés.

Contrats associés :

- [rich-app-contract.md](../rich-app-contract.md) — gates DeclUI v1/v2 et kinds
- [create-contract.md](../create-contract.md) — app riche de référence (`create`)
- [specs-techniques.md](specs-techniques.md) §0, §7 — carte architecture
- [bridge/aos-proto-decl-ui.json](../bridge/aos-proto-decl-ui.json) — schéma JSON exporté

## Ce qu’est un module

Un module Preview est une archive `.aospkg` signée :

```text
module.aospkg/
├── manifest.yaml      # nom, version, hash, caps, tools, ui, min_os_api
├── module.wasm        # guest (script réutilise ext-rt ; rust compile par module)
├── ui/                # document declarative_ui (JSON ou index.html legacy)
├── schemas/           # schémas JSON optionnels des outils
└── signatures/        # signatures catalogue (builds mainteneurs)
```

Double surface : **outils** pour les agents (`tool.invoke:<name>`) et
**declarative_ui** pour l’humain (onglet hôte `module.ui`). Pas de webview sur
l’hôte Preview.

## Gates de versioning

Vérifiées dans `crates/aos-proto/src/rich_app_contract.rs` :

| Gate | Champ / constante | Hôte actuel |
|------|-------------------|-------------|
| API module hôte | `min_os_api` ≤ `OS_API_VERSION` (`1`) | `1` |
| Vocabulaire DeclUI | `ui.contract` | `1` (Notes/Tasks/script) ou `2` (apps riches) |
| Façade jobs async | `services.jobs` | `1` si déclaré |
| Façade génération image | `services.media_image` | `1` (Create) |

Listes de kinds :

- v1 : `decl_ui::WIDGET_KINDS` dans `crates/aos-proto/src/decl_ui.rs`
- ajouts v2 : `UI_V2_ADDITIONAL_WIDGET_KINDS` dans `rich_app_contract.rs`

L’install est transactionnelle : manifeste, hash WASM, document UI, références
outils et gates sont validés **avant** activation. Échec → staging supprimé,
paquet précédent conservé (`module_rt::install`).

## Chemin A — module script (sans cargo)

Parcours testeur ([write-a-module.md](write-a-module.md)) :

1. `module.scaffold` avec `kind: script` → `var/modules/src/<name>/handlers.yaml`
2. Champ `ui` JSON optionnel au scaffold → copié dans le paquet
3. `module.package` → `var/modules/packages/<name>.aospkg` (WASM `ext-rt` précompilé)
4. `module.install` → revue de caps (fail-closed) ; onglet **Modules → &lt;name&gt;**

Règles de nom : 2–32 car., `[a-z][a-z0-9-]*`, pas un nom bundlé (`notes`, `tasks`,
`canvas`, `ext-rt`, `create`).

## Chemin B — module Rust (SDK WASM)

Quand les handlers script ne suffisent pas :

1. `module.scaffold` avec `kind: rust` sous `var/modules/src/<name>/`
2. Dépendre du SDK guest :

```toml
[dependencies]
aos-module-sdk = { path = "../../modules/sdk" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

3. Exporter avec `aos_module_sdk::export_module!(handle)` — le guest parle à
   l’hôte **uniquement** via `host_call` (`fs.read`, `fs.write`, `fs.list`,
   `mem.episodic_write`, `mem.episodic_query`). Pas de WASI ambiant.
4. `module.compile` — cap critique ; refus statique de `unsafe` et de
   `std::fs` / `net` / `process` ambiant ; `CARGO_NET_OFFLINE=true`.
5. `module.package` + `module.install` comme ci-dessus.

Sources SDK : [`modules/sdk`](../../modules/sdk) (Apache-2.0, workspace séparé).

## Scripts de build mainteneurs

Paquets officiels reconstruits depuis `modules/` (pas requis pour les modules script) :

| Paquet | Script | Sortie |
|--------|--------|--------|
| `notes` | `modules/build-notes.ps1` | `share/modules/notes.aospkg` |
| `tasks` | `modules/build-tasks.sh` / `.ps1` | `share/modules/tasks.aospkg` |
| `canvas` | `modules/build-canvas.sh` / `.ps1` | `share/modules/canvas.aospkg` |
| `create` | `modules/build-create.sh` | `share/modules/create.aospkg` |
| `ext-rt` | `modules/build-ext-rt.ps1` | runtime précompilé pour modules script |
| `gallery-demo` | `modules/build-gallery-demo.sh` | démo widgets DeclUI v2 |

Exemple (Create) :

```bash
./modules/build-create.sh
```

Compile `modules/create/` → wasm32, valide le JSON UTF-8 `ui/index.json`, écrit
le manifeste avec `ui.contract: 2`, copie vers `share/modules/create.aospkg`, et
peut rafraîchir le hash dans `share/modules/catalogue.yaml`.

## Document DeclUI

- Entrée : `ui/index.json` (apps riches) ou `ui/index.html` legacy (paquets v1)
- Mode : `declarative_ui` dans le manifeste
- Intent hôte : `module.ui` — platformd valide ; l’hôte lie les résultats d’outils
  et route soumissions bouton/form via la même revue de caps que `module.invoke`
- Schéma JSON : [`bridge/aos-proto-decl-ui.json`](../bridge/aos-proto-decl-ui.json)

Les apps riches ajoutent slots `state.local` / `state.document`, `bindings` et
`actions` (voir [rich-app-contract.md](../rich-app-contract.md)). Les widgets
de composition (`layer_canvas`, `layer_list`, `undo_redo`) sont génériques — pas
de branche hôte spécifique Create.

## Capabilities et catalogue

- Catalogue local signé : `share/modules/catalogue.yaml` + `catalogue.yaml.sig`
  (ed25519) ; Paramètres → Modules → Install exécute toujours la revue de caps
- Index communautaire opt-in : Git signé `community/catalogue.yaml` (cache hors ligne)
- Désinstall : Paramètres → Modules installés ; les apps bundlées peuvent être
  retirées — le choix persiste au boot (voir [FEATURES.md](FEATURES.md))

Les caps déclarées dans `manifest.yaml` sous `permissions.required_caps` doivent
correspondre à l’usage réel. L’hôte n’accorde jamais une cap parce qu’un widget
nomme un outil.

## ABI WASM (services visibles guest)

Autorisés via `host_call` (représentatif) : `fs.read`, `fs.write`, `fs.list`,
`mem.episodic_write`, `mem.episodic_query`, `web.search`, `web.browse`,
`net.fetch`, `files.generate`, `mem.context`, `mem.user.*`, `mem.shared_*`,
`ext.load_handlers`.

**Interdits dans les guests :** `module.install`, `module.compile`, `secrets.get`,
`agent.*`, `trust.set`.

## Paquets de référence

| Nom | `ui.contract` | Notes |
|-----|---------------|-------|
| `notes` | 1 | onglet hardcodé + outils WASM |
| `tasks` | 1 | slot Daily overflow si installé |
| `canvas` | 1 | outils vector session + onglet déclaratif |
| `create` | 2 | espace riche ; [create-contract.md](../create-contract.md) |
| `ext-rt` | — | runtime script, pas une app utilisateur |
| `gallery-demo` | 2 | galerie widgets DeclUI v2 (mainteneur) |

## Étapes suivantes

- Parcours testeur : [TESTER.md](TESTER.md) §7 (scénarios module)
- Partager un paquet : [community.md](community.md) (discussions / `community/modules/`)
- Architecture cible : [specs-techniques.md](specs-techniques.md) §7
- ADR : [adr/0009-rich-module-app-contract.md](adr/0009-rich-module-app-contract.md)
