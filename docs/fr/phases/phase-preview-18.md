# Phase P18 — E22 instincts en session

**Langue :** [English](../../phases/phase-preview-18.md) | Français

## Objectif

Livrer **E22** : étendre le `skill.pass` Preview pour que Créer | Plus tard
apparaisse **dans le fil courant** sous pression de contexte (ou après steers
répétés), avec injection d’instincts bornée. La passe nocturne devient un
rattrapage.

Dépend du `skill.pass` 0.15. Pas un nouveau numéro P6. Pas fer nu.

Priorités : [plan-evolutions.md](../plan-evolutions.md) **E22**.

## Livrables

| # | Évolution | Livrable | Statut |
|---|-----------|----------|--------|
| P18.1 | E22 surface | `surface_now` / `source_session_id` ; `pending_surface_offer` ignore 05:00 si live | fait |
| P18.2 | E22 consider | Intent `skill.pass.consider` — heuristique session+steer, fire-once, anti-spam | fait |
| P18.3 | E22 hooks | UI post-tour ; worker budget / overflow / steer≥2 ; salon `run_infer` | fait |
| P18.4 | E22 instincts | Store `instincts.json` ; inject ≤3 si confiance ≥0.7 ; Créer → `mark_created_and_promote` | fait |
| P18.5 | E22 prefs/docs | Pref `instincts_in_session` (défaut true) ; FEATURES / STATUS / roadmap EN+FR | fait |

## Comportement

```text
tour chat / agent / salon
  └─ tokens ≥ 75% prompt_budget  OU  steers ≥ 2  OU  PromptTooLong
       └─ skill.pass.consider (une fois par session_id)
            ├─ pending surface_now → Créer | Plus tard dans ce fil
            └─ upsert instincts → injection prompt bornée
skill.pass nocturne (02–04h) ── rattrapage seul (surface après 05:00 si manqué)
```

- N’écrit jamais `var/skills/` automatiquement.
- Heuristique seule (Jaccard / seaux / steers) ; pas de LLM d’extract à chaque tour.
- Pref off → pas de consider / inject live ; le rattrapage nocturne reste.

## Gates de sortie

| Gate | Critère |
|------|---------|
| Unit | seuil 75%, fire-once, `surface_now` ignore l’heure du matin, steers dans le cluster |
| Unit | inject ≤3 / plancher de confiance ; Créer → skill + promote instinct |
| Unit | pas de double carte le même jour pour le même `pattern_id` |

## Hors scope

Observateur type ECC Homunculus, promotion auto project→global, fine-tune GGUF,
remplacement de la carte du matin (elle reste le rattrapage).
