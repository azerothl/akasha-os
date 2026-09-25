# ADR 0008 : profil de frontmatter partagé pour les skills

**Langue :** [English](../../../adr/0008-shared-skill-frontmatter-profile.md) | Français

> Date : 25/09/2026 · Statut : **accepté**

## Contexte

- Alignement entre Akasha (assistant personnel [azerothl/Akasha](https://github.com/azerothl/Akasha)) et les skills Preview akasha-os.
- Le *contenu* des recettes peut converger ; loaders, ids d’outils et ABIs WASM restent séparés.
- Pas de loader partagé. La double publication de paquets est en pause.
- Un doc compagnon figé vivra dans Akasha_skills (akasha-dev) ; laisser un placeholder Related vers `azerothl/Akasha_skills` quand disponible.

## Décision

Contrat de frontmatter uniquement (un YAML `SKILL.md`) :

- **Fichier unique :** champs communs portables + clés propres au produit dans le même frontmatter ; chaque loader lit ce qu’il connaît et **ignore les clés inconnues**.

**Commun (portable) :**

- `name` (requis) — id kebab ET nom de dossier `var/skills/<name>/` ; regex Preview `[a-z][a-z0-9-]{1,32}`
- `description` (requis)
- `license` (recommandé ; MIT pour les skills community selon [ADR 0006](0006-license-split.md))
- `when_to_use` (optionnel ; repli sur `description`)
- `runtime` (filtre catalogue optionnel) — **liste YAML uniquement** : `[akasha]` | `[akasha-os]` | `[akasha, akasha-os]`. Jamais le token `both`. Absent = pas de filtre / auteur non précisé.

**Clés produit dans le même fichier :**

- **aos / Preview :** `tools`, `required_caps` (garder `required_caps` séparé de `tools` ; l’install catalogue reste fail-closed sur la revue de caps si non vide — voir `crates/aos-platform/src/skill.rs`)
- **Akasha :** `compatibility`, `metadata`, et autres clés propres à Akasha

### Hors périmètre / non-objectifs

- Pas de fusion monorepo
- Pas de crates / glue partagés
- Pas de fusion d’ABI WASM
- Pas de pipeline double publication pour l’instant
- Pas de changement des contrats DeclUI / modules `.aospkg`

## Conséquences

- Les auteurs peuvent écrire un `SKILL.md` que les deux écosystèmes peuvent stocker ; chaque runtime interprète les clés produit indépendamment.
- Le loader Preview peut ignorer `runtime` / `license` jusqu’à ce que le filtrage catalogue en ait besoin (les clés inconnues sont déjà ignorées).
- Suivi (optionnel, pas ce PR) : parser le frontmatter complet de `SKILL.md` quand `skill.yaml` est absent.

## Liens

- [ADR 0006](0006-license-split.md)
- [docs/write-a-skill.md](../../write-a-skill.md)
- `azerothl/Akasha_skills` (doc compagnon — lien quand publié)
