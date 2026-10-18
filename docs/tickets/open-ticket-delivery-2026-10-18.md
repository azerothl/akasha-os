# Traitement des neuf tickets ouverts — 18 octobre 2026

## Périmètre et décisions

Demande : implémenter et vérifier les tickets d'Akasha OS. Départ de main
`ba773c0bc211c4704cbf7e987cd915c267527174`, branche locale `codex/open-tickets`.
Le checkout de travail initial et ses modifications ont été préservés.

Décisions explicites de l'utilisateur :

- Réutiliser **Code Studio via un adaptateur Akasha** pour la profondeur IDE du #247.
- Faute de machines supplémentaires, **conserver #434 et #435 ouverts**.

Aucun ticket n'est fermé par ce document. Les modifications restent locales,
sans push, PR, fusion ou publication. « Implémenté » ci-dessous décrit ce
candidat source ; cela ne signifie pas « distribué et validé sur tous les OS ».

## État par ticket

| Ticket | Travail effectué | Validation restante avant clôture |
|---|---|---|
| [#437](https://github.com/azerothl/akasha-os/issues/437) | Binding llama.cpp 0.1.159, adaptation mtmd ; compilation/tests CPU et CUDA ; génération réelle Spark et Qwen | Metal, régression vision réelle et parcours installé |
| [#438](https://github.com/azerothl/akasha-os/issues/438) | Catalogue 0.3.6 : Spark officiel optionnel, révision/hash/taille/licence vérifiés ; chat template et génération réelle CPU/CUDA | Téléchargement→chargement→chat dans Models installé, prompts représentatifs |
| [#439](https://github.com/azerothl/akasha-os/issues/439) | MCP image, vidéo courte et upscale ; découverte Notes/Tasks installés en lecture ; allowlist/caps/audit et refus ; docs FR/EN | Génération image/vidéo avec vrais packs depuis un client MCP ; le test IPC utilise des doubles de service |
| [#444](https://github.com/azerothl/akasha-os/issues/444) | Baseline déjà livrée par #447 ; deux runs réussis revérifiés, mesures Windows/Linux/macOS disponibles | Aucun gain nouveau à annoncer : comparaison après modification du pipeline encore à produire sous #446 |
| [#445](https://github.com/azerothl/akasha-os/issues/445) | Suite de #448/#449 : tests cache/archives/rollback, index signés publiés avec les packs, garde d'immuabilité ; modules de base archivables mais encore embarqués | Installation/mise à jour/retrait réels et données conservées sur les trois OS ; ressources natives encore dans le core |
| [#446](https://github.com/azerothl/akasha-os/issues/446) | Restauration signée par empreinte exacte ; compilation des modules restaurés sautée ; invalidation sources/SDK/lock/toolchain/API ; ZIP conservé octet pour octet ; décisions dans le rapport de coût | Deux builds de release successifs sur Windows/Linux/macOS et comparaison mesurée à #444 |
| [#247](https://github.com/azerothl/akasha-os/issues/247) | Tâches autorisées et commits préparés sous confirmation ; Problems ; widgets natifs API 2/UI 2 ; adaptateur Code Studio projet/index/LSP borné | Vrai daemon et parcours installé ; DAP léger, refactor multi-agent et validation gros dépôt restent ouverts |
| [#434](https://github.com/azerothl/akasha-os/issues/434) | Périmètre de cohorte conservé | Ouvert sur décision utilisateur : 3 Windows + Linux + Mac Apple Silicon, installation fraîche en moins de 15 minutes |
| [#435](https://github.com/azerothl/akasha-os/issues/435) | Runner E9 reproductible ; comptage de GPU physiques ; contrôle local : 1 GPU CUDA, refus attendu avec code 77 | Ouvert sur décision utilisateur : vrai placement et génération sur deux GPU + comparaison mono-GPU |

## Vérifications observées

| Vérification locale Windows | Résultat |
|---|---:|
| `cargo test -p aos-proto --locked` | 194 réussis |
| `cargo test -p aos-mcp --locked` | 8 unitaires + 1 intégration IPC réussis |
| `cargo test -p aos-session --locked` | 51 réussis |
| `cargo test -p aos-ui-egui --locked` | 541 réussis, 1 ignoré |
| `cargo test -p aos-platform --no-default-features --lib --locked` | 279 réussis |
| `cargo test -p aos-model --no-default-features --lib --locked` | 43 réussis |
| `cargo test -p aos-llama --no-default-features --locked` | 20 réussis |
| `cargo test -p aos-llama --locked` (CUDA + mtmd) | 20 réussis |
| Contrats packs Python, Windows | 9 réussis |
| Contrats packs Python, Ubuntu WSL | 9 réussis |
| Clippy strict, plateforme sans embeddings, toutes cibles | Réussi |
| Clippy strict, protocole/MCP/UI/session, toutes cibles | Réussi |
| Analyse PowerShell, syntaxe Bash et chargement YAML des workflows | Réussis |

Après le test complet de plateforme, le drainage des sorties a reçu une borne
supplémentaire contre les pipes restés ouverts ; les 8 tests de commandes ont
été relancés avec succès. Les nouveaux contrats et les bindings CPU/mtmd/Metal
sont ajoutés à la matrice CI, mais **cette branche n'a pas encore tourné en CI**.

Les correctifs de validation incluent les chemins temporaires Windows,
`echo` via l'interpréteur Windows, le hash canonique LF du catalogue et deux
expressions booléennes équivalentes signalées par Clippy. Ils ne changent pas
les contrats fonctionnels de ces anciennes fonctionnalités.

## Inférence réelle et preuve matérielle

Les fichiers Spark-X2.5-4B Q4_K_M et Qwen3.5-4B Q4_K_M ont été téléchargés et
leurs SHA-256 contrôlés. Chacun a produit `Paris` à une demande de capitale,
avec le moteur compilé sans CUDA puis avec CUDA sur RTX 4080 SUPER 16 Gio.
Ce sont des smokes chargement/template/génération, pas des benchmarks de
qualité ou de performances. Aucun chiffre d'accélération n'est annoncé.

La preuve compacte est [ce JSON](evidence/2026-10-18-local-validation.json).
Les journaux détaillés locaux sont dans le dossier `evidence` voisin de ce
worktree ; ils ne sont pas des artefacts de release ni une preuve de cohorte.

## Baseline déjà disponible

- [Run cold 38602553174](https://github.com/azerothl/akasha-os/actions/runs/38602553174), succès, commit `a18d910`.
- [Run reuse 38616302662](https://github.com/azerothl/akasha-os/actions/runs/38616302662), succès, commit `072dc3`.
- [Mesures et limites de comparaison](../release-cost-baseline.md).

Les durées observées ne démontrent pas une accélération homogène de tous les
jobs. Les packs WASM ne représentent qu'une petite part des binaires et
ressources du core ; leur extraction ne démontre pas une réduction importante
du poids total ni du temps de compilation natif.

## Contrats pratiques

- [Commandes, confirmation et adaptateur Code Studio](../dev-assistant-studio.md).
- [MCP : configuration et périmètre exact](../mcp-server.md).
- [Packs signés : modes, entrées et mesure attendue](../module-pack-reuse.md).
- [Spark et protocole de preuve multi-GPU](../spark-x25-validation.md).


## Commits locaux de l'implémentation

- `da97933` — binding llama.cpp, Spark et runner E9.
- `50d9e74` — outils MCP et contrats de refus/audit.
- `da6c9a0` — restauration des packs, publication et CI.
- `1995d0c` — portabilité des fixtures et Clippy.
- `b49f0c9` — commandes, surfaces natives et adaptateur Code Studio.

La preuve locale décrit ce code. La suite logique exige publication de branche
et CI pour les validations de build sur les autres plateformes ; cela ne
remplace pas les tests matériels explicitement laissés ouverts.
