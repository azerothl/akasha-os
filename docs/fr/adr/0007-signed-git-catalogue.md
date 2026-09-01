# ADR 0007 : source catalogue Git signée, opt-in

**Langue :** [English](../../../adr/0007-signed-git-catalogue.md) | Français

> Date : 01/09/2026 · Statut : **accepté**

## Contexte

E10 livre déjà un catalogue **local** signé (`share/modules/catalogue.yaml`,
ed25519, vérif hash, revue de caps à `module.install`). Horizon 1 a besoin
d’un chemin pour publier un skill ou un `.aospkg` **hors** du zip Preview,
sans store payant ni clone de ClawHub.

L’[ADR 0006](0006-license-split.md) a déjà tranché les licences : hôte
AGPL + CLA ; `community/` MIT par défaut, pas d’octroi commercial ; un
index de catalogue futur est Apache-2.0 et chaque paquet garde sa licence.

## Décision

La Preview peut charger un **second catalogue, opt-in**, depuis un index
Git signé (même format YAML). L’URL par défaut est le
`community/catalogue.yaml` brut de ce dépôt.

- **Désactivé par défaut.** Settings → Catalogue local de modules → activer.
- **L’authenticité est la signature ed25519** vérifiée avec la clé Preview
  épinglée — pas « faire confiance à HTTPS GitHub ».
- **Hors ligne d’abord.** Une copie vérifiée est mise en cache sous
  `var/catalogue/community/`. Le boot n’exige pas le réseau.
- **L’install** utilise la même revue de caps que le catalogue bundlé.
  Index altéré ou hash non conforme → **refus**.
- Licence de l’index : Apache-2.0. Chaque paquet garde son champ licence.

Ce n’est pas un marketplace public, pas payant, et pas Discord/Telegram
dans le noyau OS.

## Conséquences

- Les auteurs ouvrent une PR sous `community/` (signer l’index ; lister
  les caps ; pas de `crates/`).
- Un dépôt catalogue dédié plus tard est permis ; il doit garder le même
  contrat index signé + revue de caps.
