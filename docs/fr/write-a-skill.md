# Écrire un skill en dix minutes

**Langue :** [English](../write-a-skill.md) | Français

> Date : 01/09/2026 · Preview **0.15.1**

Un skill est une recette Markdown. Vous pouvez en copier une dans la
Preview sans cargo, sans pull request noyau, et sans accorder la licence
commerciale ([ADR 0006](../../adr/0006-license-split.md)).

Chapitre site (même procédure) :
[azerothl.github.io/akasha-os/docs/skill.html](https://azerothl.github.io/akasha-os/docs/skill.html?lang=fr).

## Ce que ce n’est pas

Pas un module WASM, pas un crate, pas les cinq skills du zip
(`share/skills/`). Les vôtres vont sous `var/skills/` pour qu’un overlay
de mise à jour ne les efface pas.

## Étapes

1. Preview déjà installée ([INSTALL.md](INSTALL.md) ; cohorte
   [chemin de 15 minutes](TESTER.md#chemin-court-15-minutes)).
2. Nom de dossier = id du skill : minuscules, chiffres, tirets ; 2–33
   caractères ; commence par une lettre (`morning-brief`, `ping`).
3. Fichier : home Preview `var/skills/<name>/SKILL.md` — pas
   `share/skills/`.
4. Redémarrer la Preview. Lancer un agent avec ce skill, ou demander
   quelque chose qui correspond à `when_to_use`.

| OS | Destination |
|----|----------------|
| Windows | `%LOCALAPPDATA%\AgentOS-Preview\var\skills\<name>\SKILL.md` |
| Linux / macOS | `~/.local/share/agentos-preview/var/skills/<name>/SKILL.md` |

La Preview ne charge que **`SKILL.md`**. Un `SKILL.fr.md` est pour les
humains.

## Chemin A — copier l’exemple

Dans la Preview : Settings → Catalogue local de modules → activer le
**catalogue communautaire** → Récupérer l’index → Installer
`morning-brief` (même revue de caps qu’un module). Ou copier
[`community/skills/morning-brief/SKILL.md`](../../community/skills/morning-brief/SKILL.md)
→ `var/skills/morning-brief/SKILL.md`. Demander « Give me a morning
brief » (recette agent en anglais). Local seulement : mémoire, tâches
ouvertes, notes. Pas de réseau.

## Chemin B — écrire ping

Enregistrer comme `var/skills/ping/SKILL.md`. Corps agent en anglais,
comme les skills livrés. Garder `license: MIT`. Demander « ping ».

```markdown
---
name: ping
description: Reply with pong and stop
license: MIT
when_to_use: User says ping or asks if a custom skill is loaded.
tools:
  - goal.complete
---
# Ping

Reply with the single word pong. Then call goal.complete.
Do not search the web. Do not create notes or tasks.
```

## Partager

[Show and tell](https://github.com/azerothl/akasha-os/discussions) avec le
fichier collé — pas un CLA. Une PR va sous `community/` seulement, jamais
`crates/`. Voir [community.md](community.md) et
[CONTRIBUTING.md](CONTRIBUTING.md). Suite :
[premier module](write-a-module.md) (toujours sans cargo).
