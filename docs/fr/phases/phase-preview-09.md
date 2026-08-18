# Phase P09 — Preview 0.9.0

**Langue :** [English](../../phases/phase-preview-09.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.9.0** : **migration de device en milieu de
token** (E18) et **génération média locale extensible** (E19) — d’autres
modèles d’image (Flux2, Ideogram4, …), un ensemble fermé d’options sd.cpp /
Piper, et une surface Settings + intents pour les choisir. Après E17 (0.8),
la bascule de device annule encore le `model.infer` en cours. 0.9 conserve
le **même flux** à la migration, et cesse de figer `-W 512 -H 512 --steps 20`
/ les défauts Piper.

Dépend de P08 (moteurs E16 + artefact E17). Pas seL4 / fer nu. Pas de nouveau
numéro de gate P6. Pas d’argv CLI arbitraire venant d’un agent.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon B (E18, E19).
Séquençage : P09.1 → P09.2 ; P09.3 ∥ P09.4 → P09.5 → P09.7 → P09.6.

## Pourquoi après 0.8

0.8 doit d’abord livrer un artefact unifié, un chemin cancel-then-reload, et
un download sd.cpp / Piper qui marche. Copier le KV entre backends llama.cpp
est un changement fail-closed à part. Passer des flags sd.cpp en plus l’est
aussi : 0.8 ne transmet que `-m -p -o -W -H --steps`. Les flags inconnus
doivent être **refusés** (même règle fail-closed que les kinds de widgets
E15) — jamais de `Command::arg` de chaînes libres venant d’un agent.

## Pourquoi un schéma d’options fermé

sd.cpp et Piper exposent des dizaines de flags CLI. Un agent n’a pas droit
à un shell. Preview 0.9 publie un **objet JSON fermé** sur
`media.image.generate` / `media.audio.generate` et dans Settings. Les clés
hors schéma sont rejetées et auditées. Les extras dont le moteur a besoin
(VAE, CLIP, T5 pour Flux) vivent sur l’**offering** (`extra_files` /
`engine_args` dans `catalog-offerings.json`), pas saisis comme chemins par
l’utilisateur.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P09.1 | E18 migrate | Le Placement Manager peut déplacer un infer actif CPU ↔ GPU sans abort du stream ; KV/état chez le Model Subsystem | prévu |
| P09.2 | E18 policy | Pin UI et `auto` (charge / pression VRAM E16) utilisent migrate si un stream est live ; cancel+restart 0.8 reste le fallback | prévu |
| P09.3 | E19 schéma | Objets d’options fermés sur `media.*` + Settings persistés ; clés inconnues refusées ; `aos-sd` les mappe vers des flags sd.cpp / Piper allowlistés | prévu |
| P09.4 | E19 catalogue | Packs image **optionnels** supplémentaires (au moins une famille Flux2 + une Ideogram4) et voix Piper en plus ; `extra_files` VAE/CLIP/T5 ; Placement `min_vram_mib` | prévu |
| P09.5 | E19 surface | Models / Settings : pack image + options par défaut ; voix Piper + params de synthèse ; `/image` et les outils les honorent | prévu |
| P09.7 | Hygiène | Finir le reliquat P08.8 : découper les modules hôte encore trop gros ; chrome bilingue restant ; clé de rôle chat | prévu |
| P09.6 | Docs / ship | FEATURES/STATUS/TESTER, version 0.9.0, site, packaging | prévu |

Catalogue (une fois livré) : [`docs/fr/FEATURES.md`](../FEATURES.md).

### Options image (allowlist → sd.cpp)

Mappées depuis le schéma ; les valeurs 0.8 restent les défauts :

| Clé schéma | CLI | Défaut |
|------------|-----|--------|
| `width` / `height` | `-W` / `-H` | 512 |
| `steps` | `--steps` | 20 |
| `cfg_scale` | `--cfg-scale` | défaut moteur |
| `seed` | `--seed` | aléatoire |
| `sampling_method` | `--sampling-method` | défaut moteur |
| `negative_prompt` | `-n` | vide |
| `threads` | `-t` | défaut moteur |

Possédés par l’offering (pas des chemins inventés par l’agent) : `--vae`,
`--clip_l`, `--clip_g`, `--t5xxl`, `--diffusion-model` si le pack les
déclare.

### Options Piper (allowlist)

| Clé schéma | CLI | Rôle |
|------------|-----|------|
| `length_scale` | `--length_scale` | débit (plus haut = plus lent) |
| `noise_scale` | `--noise_scale` | variation du générateur |
| `noise_w` | `--noise_w` | variation de largeur de phonème |
| `sentence_silence` | `--sentence_silence` | secondes après chaque phrase |
| `speaker` | `--speaker` | id de voix multi-speaker |

Plus des packs Piper optionnels supplémentaires au catalogue (au-delà de
`en_US` / `fr_FR`).

### Reliquat nettoyage / refacto (P09.7)

P08.8 a livré le premier passage d’hygiène hôte (découpe egui
runtime/cmd, modules `media`/`providers` de modeld, presets providers,
i18n Agents/Notes/Tâches/Audit, code mort). **Encore trop gros / dérivé —
à faire en 0.9**, après E18/E19 et **avant** le tag (même place que P08.8) :

- Découper `crates/aos-ui-egui/src/main.rs` (~4850 lignes : Chat, Settings,
  apply d’événements) selon les frontières d’onglet / service déjà là.
- Découper `crates/aos-platform/src/bin/aos-platformd.rs` (~3295 lignes)
  selon les groupes d’intents (`fs.*`, `mem.*`, `module.*`, …).
- Chrome bilingue restant : onglet Scénarios (encore surtout du français
  dur) ; chaînes chat/statut encore figées en français.
- La clé de rôle chat est encore `"vous"` (filtre / persistance internes,
  pas un libellé i18n) — passer à un `user` neutre sans casser les sessions
  chargées.

Gate : pas de changement de comportement (`cargo test --workspace` + p4/p5
inchangés). Pas une réécriture seL4, pas un nouveau toolkit UI.

## Gates de sortie

| Gate | Critère |
|------|---------|
| P09.1 | Démarrer une longue complétion GPU (ou CPU) ; changer de device en cours de stream ; la réponse continue sans Stop et sans tour « cancelled » tronqué |
| P09.2 | `auto` sous pression VRAM migre sans que le testeur appuie sur Stop ; pin gpu/cpu surcharge toujours ; migrate en échec → fallback 0.8 + audit |
| P09.3 | Settings / intent peuvent fixer steps et taille ; une clé d’option inconnue est refusée (auditée) ; le PNG n’est pas toujours 512² / 20 steps |
| P09.4 | Le catalogue liste ≥1 famille image extra (Flux2 ou Ideogram4) et ≥1 voix Piper extra ; Download + `model_id` l’utilisent ; VRAM comptabilisée |
| P09.5 | Le testeur choisit un pack image non défaut et une voix Piper dans l’UI ; `/image` et le TTS utilisent ce choix après restart |
| P09.7 | PR(s) d’hygiène sans changement de comportement volontaire ; `main.rs` / `aos-platformd.rs` découpés selon les frontières existantes ; Scénarios + clé de rôle chat suivent la langue Settings |
| P09.6 | FEATURES/STATUS/TESTER + version 0.9.0 |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | Deux artefacts testeur (Win / Linux) + `latest.json` complet |

## Hors périmètre

Génération vidéo (Wan / LTX / …), img2img / inpaint en intent de première
classe, micro always-on / STT, passthrough CLI brut, E7 TPM, daemon HTTP
sibling live, E9 / P5.2 multi-GPU, compositor E13, fermeture cohort PC,
macOS, fer nu (E11–E13). Speculative decode / fusion multi-backend de tokens.

## Suite

Tag `v0.9.0` seulement quand les gates P09.1–P09.7 passent. Après 0.9 :
reste Horizon B (E7 TPM, adaptateur HTTP live si un daemon est planifié, E9
quand un 2e GPU existe).
