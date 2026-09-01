# Contribuer à Akasha OS

**Langue :** [English](../../CONTRIBUTING.md) | Français

Merci. Tu n’as besoin ni de `cargo` ni d’un clone pour aider.

La cohorte Preview est ouverte. Gate : **3 Windows + 1 Linux + 1 macOS
Apple Silicon** suivent le
[chemin de 15 minutes](TESTER.md#chemin-court-15-minutes) sans toolchain
Rust, chacun avec un rapport `var/feedback/` exploitable.

## Cohorte Preview (prioritaire)

1. Installer depuis les
   [GitHub Releases](https://github.com/azerothl/akasha-os/releases) — voir
   [INSTALL.md](INSTALL.md). Les builds macOS ne sont pas signés ; lance
   `install.sh` et attends Gatekeeper. Pas d’Intel Mac.
2. Suivre le [chemin court](TESTER.md#chemin-court-15-minutes).
3. Envoyer un retour depuis l’UI (onglet Feedback). Laisser **Create a
   GitHub issue** coché, sauf security.
4. Optionnel : un check-in sur les
   [Discussions](https://github.com/azerothl/akasha-os/discussions).

Le protocole long dans [TESTER.md](TESTER.md) est la checklist équipe pour
PC.6–PC.9 et PC.11–PC.13. Il n’est pas exigé de chaque testeur.

Lieu de rencontre public : GitHub Discussions. Pas de Discord (ni d’autre
canal de messagerie) pour ce projet pour l’instant. Page communauté :
[community.md](community.md).

## Aider sans PR noyau

| Chemin | Où | Licence aujourd’hui |
|--------|----|---------------------|
| Rapport testeur | Feedback in-app → issue GitHub | n/a |
| Question / check-in cohorte | [Discussions](https://github.com/azerothl/akasha-os/discussions) | n/a |
| Idée de skill ou de module | Discussions (Show and tell) | Publier dans une Discussion **n’accorde pas** la licence commerciale |
| Typo / traduction de docs | Pull request | Licence de contribution ci-dessous |
| Proposition d’ADR | Pull request sous `adr/` | Licence de contribution ci-dessous |
| Changement noyau / crate | Pull request | Licence de contribution ci-dessous |

Jusqu’à un split de licence ultérieur, **tout fichier que tu PR dans ce
dépôt** est couvert par la licence de contribution ci-dessous. Préfère une
Discussion si tu veux partager un skill ou un module sans cet octroi.

## Proposer un skill

Les skills sont du Markdown sous `share/skills/` / `var/skills/` (exemple
livré : [`skills/planner/SKILL.md`](../../skills/planner/SKILL.md)). Ouvre
une Discussion Show and tell avec le corps du `SKILL.md`. N’ouvre une PR
que si tu acceptes la licence de contribution.

## Scaffolder un module

Dans la Preview : Settings → Modules, ou demande à un agent
`module.scaffold` (TESTER étapes 14–15). Partage le `.aospkg` en Discussion.
L’install fait toujours la revue de caps. SDK guest :
[`modules/sdk`](../../modules/sdk).

## Proposer une ADR

Reprendre le style de
[`adr/0001-microkernel.md`](../../adr/0001-microkernel.md). Ouvrir une PR
titrée `adr: …`. La licence de contribution s’applique.

## Labels

`bug`, `cohort`, `area:modules`, `area:skills`, `good first issue`. Les
formulaires d’issue en posent certains ; les Discussions restent sans label
sauf action d’un mainteneur.

## Code de conduite

Voir [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Sécurité

Voir [SECURITY.md](SECURITY.md). N’ouvre pas d’issue publique.

## Licence des contributions

Le projet est en **double licence** (AGPL-3.0-only + licence commerciale).
Pour que les deux voies restent possibles, chaque contribution **fusionnée
dans ce dépôt** doit pouvoir être redistribuée sous les deux régimes.

En ouvrant une pull request ou en poussant du code dans ce dépôt, tu
certifies que :

1. tu es titulaire des droits sur ta contribution, ou tu as le droit de
   la soumettre ;
2. tu la places sous **GNU AGPL-3.0-only** (`LICENSE`) ;
3. tu accordes au concédant (Loïc Peaudecerf) une licence irrévocable,
   mondiale, gratuite, de redistribuer ta contribution aussi sous la
   **licence commerciale** décrite dans `LICENSE-COMMERCIAL.md`.

Si tu ne peux pas accorder le point 3, ne soumets pas la contribution
ici ; discute d’abord d’une exception écrite. Un post de Discussion n’est
pas une contribution au sens de cette section.

## Marque

N’utilise pas « Akasha OS » comme nom d’un fork. Conserve
`NOTICE` et un lien vers
<https://github.com/azerothl/akasha-os>.
