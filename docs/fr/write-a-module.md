# Premier module (sans cargo)

**Langue :** [English](../write-a-module.md) | Français

> Date : 03/09/2026 · Preview **0.17.0**

Un module est dual-surface : des outils pour les agents, un arbre de
widgets fermé pour l’humain. La Preview peut scaffolder, packager et
installer un module **script** sans cargo. Plus long qu’un
[skill](write-a-skill.md) ; toujours pas une PR noyau
([ADR 0006](../../adr/0006-license-split.md)).

Chapitre site :
[azerothl.github.io/akasha-os/docs/module.html](https://azerothl.github.io/akasha-os/docs/module.html?lang=fr).

## Ce que ce n’est pas

Pas `notes` / `tasks` / `canvas` / `ext-rt` (bundlés). Pas d’HTML/JS ni de
webview (`declarative_ui` seulement). Pas de marketplace public — catalogue
local signé plus revue de caps.

## Chemin A — demander à l’agent

1. Preview installée ([INSTALL.md](INSTALL.md) ;
   [chemin de 15 minutes](TESTER.md#chemin-court-15-minutes)).
2. Nom : 2–32 caractères, `[a-z][a-z0-9-]*`, pas un nom bundlé
   (`cohortmod`, `pingmod`).
3. Scenarios → **Launch agent: create module cohortmod**, ou Chat
   « crée un module pingmod ».
4. **Revue de caps** : lire la liste (souvent
   `fs.write:/documents/<name>/**` et `module.install`). Deny = fail-closed ;
   timeout = refus ; Accept pour installer.
5. Barre **Modules → &lt;name&gt;** (pas Notes/Tasks). Heading, formulaire
   ou bouton, table. Soumettre une fois.

## Où vont les fichiers

Sous le home Preview `var/modules/` :

- source : `src/<name>/` (`handlers.yaml` pour kind `script`)
- paquet : `packages/<name>.aospkg`

Windows : `%LOCALAPPDATA%\AgentOS-Preview`. Linux / macOS :
`~/.local/share/agentos-preview`.

## Chemin B — Settings

Settings → Modules : `module.scaffold` (kind `script`) →
`module.package` → `module.install`. Même revue de caps. Désinstall :
Settings → Installed modules (pas les quatre bundlés).

## Compile Rust (pas le premier)

`module.scaffold` kind `rust` + `module.compile` : cap critique, refus
statique de `unsafe` / fs/net/process ambiants,
`CARGO_NET_OFFLINE=true`. SDK : [`modules/sdk`](../../modules/sdk)
(Apache-2.0). Passer tant qu’un module script n’a pas tourné.

## Partager

Coller ou joindre `var/modules/packages/<name>.aospkg` en
[Show and tell](https://github.com/azerothl/akasha-os/discussions) — pas un
CLA. PR sous `community/modules/` seulement, jamais `crates/`. L’install
fait toujours la revue de caps. Voir [community.md](community.md).
