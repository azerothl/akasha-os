# Suite du ticket #247 — 18 octobre 2026

## Livraison et périmètre

[PR #450](https://github.com/azerothl/akasha-os/pull/450) fusionnée à 17:38:24 UTC,
commit `9b91f972365cfcc171676dc9a1fb0c2b50a34534`, après succès de ses contrôles CI.
Ce nouveau lot part de ce commit sur `codex/dev-assistant-studio-completion`.
Le code est enregistré dans `e21b63b`. Il reste un candidat source local,
sans nouvelle release. Le checkout initial
`E:\akasha-os` a été préservé.

Les tickets [#434](https://github.com/azerothl/akasha-os/issues/434) et
[#435](https://github.com/azerothl/akasha-os/issues/435) restent **ouverts**,
conformément à la décision explicite de l'utilisateur. #247 reste également
ouvert : les validations et dépendances ci-dessous sont encore nécessaires.

## Ajouts implémentés

| Partie | Implémentation | Preuve observée |
|---|---|---|
| DAP léger | Configurations fixes, capacités, confirmation, acteur/workspace, pas-à-pas, pile, variables, points d'arrêt, arrêt/expiration | Vrai debugpy : deux pas, pile à `main.py:3`, variable `value=42`, autre acteur refusé, arrêt propre |
| Propositions multi-agents par sous-dossier | Identités distinctes, dossiers disjoints, propositions sans écriture, hash de revue, confirmation, journal durable et annulation | Tests sur vrais fichiers : deux acteurs, refus puis approbation, annulation après réouverture du journal, interruption récupérable, modifications externes préservées |
| Mémoire de projet | Champs persistants du projet Studio, espace `studio-project:<id>`, mise à jour d'un seul champ sous confirmation | Vrai daemon : contexte relu après arrêt/redémarrage ; mauvais identifiant de projet refusé |
| Index et navigation | Réindexation incrémentale et LSP réutilisés depuis Studio ; omission du texte corrigée pour lire le disque | Vrai serveur TypeScript : définition trouvée ; 2 005 fichiers indexés sur une fixture de 2 000 sources générées |
| MCP en mode hors ligne strict | Politique autoritative avant découverte/appel, contrôle pendant l'appel et au repos, arrêt du groupe de processus | Vrai bus IPC + vrai serveur stdio : transition online → offline_strict, serveur arrêté ; politique absente/refusée bloquante |
| Module de référence | API hôte 3, UI 2, ext-rt 1.1.0 ; arguments JSON typés et texte littéral ; formulaires mémoire/DAP/revue | Package réellement installé/invoqué en WASM ; passage jusqu'à la vraie confirmation hôte, refus sans écriture puis application ; rendu egui sans fenêtre et saisie JSON typée |

Les propositions de refactorisation des tests sont déterministes : elles ne
prouvent pas qu'un modèle a conçu un bon refactor. Le coordinateur ne démarre
pas de modèles et n'accorde aucune permission. Les agents doivent déjà disposer
de leurs outils et des droits limités nécessaires.

Le module générique ne demande aucun accès hôte. Après liaison du workspace,
les capacités choisies doivent être déclarées dans le manifeste source local,
puis le package mis à jour et approuvé. Le test réel WASM suit ce chemin ; il
ne contourne pas le refus des capacités non déclarées.

## Essais réels et limites

Le daemon testé est identifié par le SHA-256
`964889430da97609aa8e0ce22eab3b1d610eb6b5fa60701a195249878bed2928`.
Il a été copié dans un dossier de test isolé, avec ses propres données, projets,
politique et port loopback. Aucun projet utilisateur n'a été modifié et aucune
tâche LLM n'a été appelée. Le SDK TypeScript et le serveur de langage déjà
installés ont été réutilisés. La copie du SDK est exclue de l'index de la fixture.

L'essai de 2 000 sources a produit un index prêt de **2 005 fichiers / 2 005
chunks**. Le parcours de la sonde (métadonnées, mémoire, réindexation, navigation,
refus de mauvais projet) a duré **10 500 ms** sur ce poste Windows. Ce chiffre
n'est ni une mesure isolée de réindexation, ni un benchmark comparatif, ni une
validation sur un vrai gros monorepo. La mémoire a survécu au redémarrage du daemon.

L'endpoint LSP testé dépend encore de
[la PR Akasha #169](https://github.com/azerothl/Akasha/pull/169), ouverte au moment
de la vérification. Aucune PR des dépôts Akasha ou Code Studio n'a été fusionnée
ici. Le simple mapping d'un dossier ne remplace pas les règles propres au daemon.

Les processus hôtes restent exécutés avec les droits du compte. La politique MCP
arrête les groupes possédés à la détection du changement de mode ; le contrôle
périodique n'est pas un pare-feu et ne contient pas un processus volontairement
échappé. Le DAP refuse les actions non prévues. Les propositions excluent règles
ignore, chemins usuels de secrets et clés privées évidentes, sans scanner complet
de secrets. Le journal local conserve des extraits de source pour l'annulation.

## Vérifications locales

| Contrôle Windows | Résultat |
|---|---:|
| Plateforme, suite complète séquentielle sans embeddings | 295 réussis |
| Agents | 376 réussis |
| Protocole | 194 réussis |
| Interface | 542 réussis, 1 ignoré |
| Runtime ext-rt | 2 réussis ; WASM reconstruit et catalogue re-signé |
| Contrats de packs Python | 9 réussis |
| Clippy strict, quatre crates, toutes cibles, sans embeddings | Réussi |
| Package référence et parcours WASM/hôte après reconstruction | Réussis |
| Cohérence des versions, des docs du site et du catalogue | Réussie |

Les commandes et résultats détaillés sont dans
[le relevé JSON](evidence/2026-10-18-dev-assistant-validation.json).
Un ancien test de quota de capture a dépassé sa petite échéance sous charge lors
d'un premier passage parallèle ; il a réussi lors du passage complet séquentiel.
Le premier essai WASM a également révélé une configuration de manifeste sans
les capacités déclarées ; la fixture et la documentation ont été corrigées, sans
assouplir le contrôle d'installation.

## Reproduire les essais ciblés

```powershell
cargo run -p aos-platform --no-default-features --example workspace_debug_probe --locked -- C:/Python/python.exe
cargo build -p aos-platform --no-default-features --example studio_adapter_probe --locked
python scripts/verify-studio-adapter.py --daemon-bin C:/chemin/akasha-daemon.exe --probe-bin C:/chemin/studio_adapter_probe.exe --lsp-bin C:/chemin/node_modules/.bin --typescript-package C:/chemin/node_modules/typescript --fixture-files 2000
```

Le Python choisi doit disposer de debugpy. Les exécutables et dépendances sont
fournis explicitement ; la sonde ne les télécharge pas. La sonde Studio refuse
les projets sans marqueur de fixture ; la sonde DAP crée son propre dossier. Les journaux détaillés sont dans le dossier
`evidence` voisin du worktree ; le script Studio conserve son dossier temporaire
isolé et affiche son chemin pour inspection. Les processus de fixture sont arrêtés.

## Critères encore ouverts

- Parcours humain sur un **module fraîchement installé**, clics natifs et
  confirmations, sur les OS visés. Le test WASM/hôte et le rendu sans fenêtre
  ne constituent pas une validation manuelle de l'application installée.
- Livraison de l'endpoint LSP du daemon et de son interface Studio par leurs PR
  respectives ; aucune fusion implicite d'un autre dépôt.
- Refactor multi-agent conçu/exécuté avec de vrais modèles, qualité du résultat,
  recherche structurelle et politique complète de secrets sur des projets
  représentatifs. Le coordinateur et les tests déterministes ne prouvent pas cela.
- CI de ce nouveau lot sur Linux/macOS ; aucune preuve matérielle supplémentaire
  pour #434/#435. Pas de comparaison multi-GPU inventée.

[Contrats et configuration](../dev-assistant-studio.md) ·
[Bilan du lot initial](open-ticket-delivery-2026-10-18.md).
