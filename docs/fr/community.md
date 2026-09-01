# Communauté — Akasha OS Preview 0.15.1

**Langue :** [English](../community.md) | Français

> Date : 01/09/2026 · Preview **0.15.1**

Le lieu de rencontre est **GitHub Discussions** sur
[azerothl/akasha-os](https://github.com/azerothl/akasha-os/discussions).
Pas de Discord, Matrix ou Slack pour ce projet pour l’instant — ce
seraient des salons vides. Les canaux de messagerie sont aussi **hors du
noyau OS** (voir [plan-evolutions.md](plan-evolutions.md)) ; cette page
est pour les humains.

Site public :
[Community](https://azerothl.github.io/akasha-os/community.html?lang=fr).

## Gate cohorte

**3 Windows + 1 Linux + 1 macOS Apple Silicon** suivent le
[chemin de 15 minutes](TESTER.md#chemin-court-15-minutes) sans toolchain
Rust. Chacun laisse un `var/feedback/fb-*.json` exploitable (et de
préférence une issue GitHub). Le protocole TESTER long reste la checklist
équipe ; il n’est pas exigé de chaque testeur.

Les builds macOS ne sont pas signés (Gatekeeper). Pas d’Intel Mac.

## Catégories de Discussion suggérées

À créer dans l’UI GitHub si elles manquent (Settings → General →
Features → Discussions) :

| Catégorie | Rôle |
|-----------|------|
| **Cohort** | Check-ins et l’appel public ci-dessous |
| **Q&A** | Install, premier run, « est-ce que ça compte » |
| **Show and tell** | Skills, modules, captures — pas une PR |
| **Ideas** | Idées produit qui ne sont pas des bugs |
| **Research** | Caps, seL4, placement — pas l’onboarding testeur |

Les catégories natives Q&A / Ideas / Show and tell suffisent tant que
Cohort et Research n’existent pas.

## Checklist mainteneur (une fois)

1. Confirmer que Discussions est activé (c’est le cas sur ce dépôt).
2. Ajouter les catégories **Cohort** et **Research** si elles manquent.
3. Poster l’appel ci-dessous dans **Cohort** (EN, puis une réponse FR ou
   un second post). L’épingler.
4. Confirmer les labels : `bug`, `cohort`, `area:modules`,
   `area:skills`, `good first issue`.

Le texte d’appel à coller est dans la version
[anglaise](../community.md#public-call-paste-into-a-cohort-discussion)
(corps EN et FR).

## Ce que les testeurs font

1. [Installer](INSTALL.md) sans `cargo`.
2. [Chemin court](TESTER.md#chemin-court-15-minutes).
3. Retour depuis l’UI.
4. Check-in Discussion optionnel.

Code de conduite : [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
Sécurité : [SECURITY.md](SECURITY.md).
Comment contribuer : [CONTRIBUTING.md](CONTRIBUTING.md).
