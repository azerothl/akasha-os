# Reprise du backlog — 2 novembre 2026

## Branches locales

La branche du dossier principal `codex/chat-canvas-painted-render` avait trois
commits de retard sur sa branche distante. Elle a été avancée sans modifier les
empreintes des cinq fichiers déjà modifiés ni supprimer les fichiers non suivis.
La référence locale `main`, qui avait soixante et un commits de retard, correspond
désormais à `origin/main` (`2470aa8`). Le travail utilise le worktree déjà attaché.

Le suivi mémoire `9c48704`, encore local, a été conservé et a intégré les trois
mises à jour de dépendances arrivées sur `main`.

## CI et PR en attente

- Le run `workspace-ci` [41467470383](https://github.com/azerothl/akasha-os/actions/runs/41467470383)
  sur `2470aa8` échoue dans Clippy et dans les vérifications de l'interface des
  trois plateformes. Les journaux montrent le même changement d'API de `zip` 9 :
  `ZipFile::name()` est devenu faillible. Le correctif `a1c9753` propage cette
  erreur pour les packs et utilise `is_dir()` pour l'extraction des mises à jour.
  Trois tests de packs, quatre tests de mise à jour et Clippy strict réussissent
  localement. Les validations de chemins, empreintes et activation restent actives.
- La [PR #457](https://github.com/azerothl/akasha-os/pull/457) a été fermée comme
  remplacée : son correctif du test WASM est déjà intégré, avec conservation de
  la fixture, par la PR #461 (`c4b5ef4`). Le ticket #456 a été fermé comme résolu.
  Le test ciblé a aussi été revalidé avec Wasmtime 48 : succès en 3,57 secondes.
  La PR #457 n'a pas été fusionnée.
- La [PR #442](https://github.com/azerothl/akasha-os/pull/442) est fusionnable,
  en brouillon, avec des contrôles réussis sur son dernier commit et aucun fil de
  revue non résolu. Son audit WCAG mentionné en suivi reste à réaliser ; son état
  de brouillon est conservé. Aucun nouveau contrôle de release n'est déduit de
  ces anciens résultats CI.
- Deux PR ont été publiées pendant cette reprise : la [#468](https://github.com/azerothl/akasha-os/pull/468)
  corrige déjà le ticket #466 / ZIP 9, et la [#467](https://github.com/azerothl/akasha-os/pull/467)
  ajoute la reprise de connexion au bus et des filtres de dépannage pour #464.
  Leurs commits ont été intégrés aux branches locales correspondantes pour
  préparer des compléments à ces PR, sans PR concurrente. Le complément #468
  vérifie l'extraction imbriquée et le refus d'une traversée (cinq tests de mise
  à jour réussis). Le complément #467 conserve la reprise du bus, ajoute le
  correctif du cache hybride et date les incidents. Un canari sain ne suffit
  pas à ignorer un agent qui vient réellement d'échouer.
  Les deux têtes distantes ont une CI réussie, sans fil de revue non résolu.
  Les nouveaux compléments locaux passent 556 tests d'interface, sept tests IPC,
  22 tests serveur et Clippy strict ; leur CI attend leur publication.

## Tickets

| Ticket | Résultat de la reprise | Acceptation restante |
|---|---|---|
| #466 | Panne ZIP 9 reproduite ; PR #468 existante intégrée localement et complétée par des tests | Publication des compléments, intégration et CI du nouveau commit |
| #464 | Diagnostic des faux incidents historiques et reproduction/correction d'un défaut du cache hybride ; voir le [bilan](preview-troubleshooting-464.md) | Intégration et recette dans Preview installé |
| #460 | Suivi `9c48704` conservé : accusé de traitement exact, reprise d'un lot incomplet, historique modèle ignoré ; preuve Qwen longitudinale synthétique existante ; 17 tests ciblés et Clippy strict revalidés avec les dépendances actuelles | Publication du suivi, validation native explicitement ouverte, qualité sur historiques représentatifs |
| #456 | Correctif déjà intégré par #461, test maintenu ; ticket fermé | — |
| #444 | Baseline et comparaison froide/réutilisée documentées, résultats mixtes ; les anciens artefacts de mesure ne sont plus listés par GitHub | Conservation/reprise de mesures complètes comparables pour le suivi des optimisations |
| #445 | Installation optionnelle signée et transactionnelle déjà intégrée ; la compatibilité ZIP 9 est corrigée localement | Recette install/update/remove et conservation des données sur les trois plateformes, tailles livrées |
| #446 | Restauration de packs par empreinte et validation déjà intégrées | Deux releases successives sur les trois plateformes, preuve de réutilisation et comparaison mesurée |
| #437 | Binding `llama-cpp-sys-2` 0.1.159 intégré ; smokes CPU/CUDA existants ; le suivi du cache protège Qwen hybride | Recette installée Models et régression Metal/vision représentative |
| #438 | Offering Spark optionnel épinglé et smokes existants ; rejeu CPU et CUDA vérifié pendant cette reprise | Téléchargement/chargement/chat dans Models installé et prompts représentatifs |
| #439 | Façade MCP et refus par capacités déjà intégrés | Appels image et vidéo par un vrai client MCP avec packs installés |
| #247 | Adaptateur Code Studio et suivi de projet déjà intégrés dans les PR précédentes | Validation native et cas représentatifs encore ouverts |
| #434 | Périmètre de cohorte conservé | Trois Windows, un Linux et un Mac Apple Silicon ; autres machines indisponibles |
| #435 | Périmètre E9 conservé | Placement et génération sur deux GPU physiques ; hôte de test disponible à un GPU |

Les tickets #247, #434 et #435 restent ouverts conformément à la consigne
utilisateur. Le passage de tests locaux ou la fusion d'une PR n'est pas utilisé
pour attester ces recettes. Aucun nouveau release Preview n'a été publié.

La lecture GitHub finale recense douze tickets ouverts et trois PR ouvertes
(#467, #468 et le brouillon #442). Les trois têtes publiées sont fusionnables,
sans contrôle en échec/en cours ni fil de revue non résolu ; les contrôles de
#442 datent du 11 octobre. Aucun complément local n'a encore été poussé et
aucune PR n'a été fusionnée pendant cette reprise.
