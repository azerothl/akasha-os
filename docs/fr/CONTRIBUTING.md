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

Politique de licence : [ADR 0006](../../adr/0006-license-split.md) (noyau vs
extensions).

| Chemin | Où | Licence / CLA |
|--------|----|----------------|
| Rapport testeur | Feedback in-app → issue GitHub | n/a |
| Question / check-in cohorte | [Discussions](https://github.com/azerothl/akasha-os/discussions) | n/a |
| Idée de skill ou de module | Discussions (Show and tell) ou PR [`community/`](../../community/README.md) | **Pas** d’octroi commercial. Une Discussion est toujours sûre. `community/` défaut MIT |
| SDK guest / module WASM first-party | PR sous `modules/` | Apache-2.0 ([`LICENSE-APACHE`](../../LICENSE-APACHE)) ; **pas** d’octroi commercial |
| Typo / traduction de docs | Pull request sous `docs/` | CLA hôte (AGPL + octroi commercial) |
| Proposition d’ADR | Pull request sous `adr/` | CLA hôte |
| Noyau / crate / UI hôte | Pull request sous `crates/`, `packaging/`, `website/` | CLA hôte |

Ne **mélange pas** fichiers AGPL hôte et fichiers d’extension permissifs
dans une même PR.

## Proposer un skill

Les skills sont du Markdown sous `share/skills/` / `var/skills/`. Exemple
communautaire (hors zip Preview) :
[`community/skills/morning-brief/`](../../community/skills/morning-brief/).
Recette livrée : [`skills/planner/SKILL.md`](../../skills/planner/SKILL.md).
Ouvre
une Discussion Show and tell avec le corps du `SKILL.md`, ou une PR sous
[`community/`](../../community/README.md) (défaut MIT, pas d’octroi
commercial). Une PR qui change les `skills/` livrés dans le zip Preview est
MIT — toujours sans octroi commercial.

## Scaffolder un module

Dans la Preview : Settings → Modules, ou demande à un agent
`module.scaffold` (TESTER étapes 14–15). Partage le `.aospkg` en Discussion
(pas de CLA) ou sous [`community/`](../../community/README.md). L’install
fait toujours la revue de caps. SDK guest :
[`modules/sdk`](../../modules/sdk) (Apache-2.0).

## Proposer une ADR

Reprendre le style de
[`adr/0001-microkernel.md`](../../adr/0001-microkernel.md). Ouvrir une PR
titrée `adr: …`. Les ADR vivent avec les docs hôte : la licence de
contribution des chemins AGPL s’applique.

## Labels

`bug`, `cohort`, `area:modules`, `area:skills`, `good first issue`. Les
formulaires d’issue en posent certains ; les Discussions restent sans label
sauf action d’un mainteneur.

## Code de conduite

Voir [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Sécurité

Voir [SECURITY.md](SECURITY.md). N’ouvre pas d’issue publique.

## Licence des contributions

L’**OS hôte** est en double licence (AGPL-3.0-only + commerciale). Les
extensions guest ne le sont pas. Voir l’[ADR 0006](../../adr/0006-license-split.md).

En ouvrant une pull request qui touche des chemins **hôte** (`crates/`,
`packaging/`, `website/`, `docs/`, `adr/`, `vm/`, `demo/` et autres arbres
AGPL), tu certifies que :

1. tu es titulaire des droits sur ta contribution, ou tu as le droit de
   la soumettre ;
2. tu la places sous **GNU AGPL-3.0-only** (`LICENSE`) ;
3. tu accordes au concédant (Loïc Peaudecerf) une licence irrévocable,
   mondiale, gratuite, de redistribuer ta contribution aussi sous la
   **licence commerciale** décrite dans `LICENSE-COMMERCIAL.md`.

Si tu ne peux pas accorder le point 3, ne soumets pas de PR sur un chemin
hôte ; discute d’abord d’une exception écrite.

En ouvrant une pull request qui ne touche que des chemins **extension**
(`modules/sdk`, modules guest first-party, `skills/`, `community/`), tu
certifies que tu as le droit de soumettre et que tu places le travail sous
la licence de cet arbre (Apache-2.0 ou MIT, ADR 0006). Cette PR **n’accorde
pas** la licence commerciale.

Un post de Discussion n’est jamais une contribution au sens de cette
section.

## Marque

N’utilise pas « Akasha OS » comme nom d’un fork. Conserve
`NOTICE` et un lien vers
<https://github.com/azerothl/akasha-os>.
