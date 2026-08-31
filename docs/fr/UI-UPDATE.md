# Mise à jour visuelle UI Preview

**Langue :** [English](../UI-UPDATE.md) | Français

**Statut :** ready-for-implement  
**Date :** 2026-08-31  
**Cible :** Preview 0.15.x (`crates/aos-ui-egui`)

Contrat d’implémentation visuelle pour la passe chrome hybride chambre. L’IA produit, l’a11y et les tables de jetons restent dans [UI.md](UI.md). Ce document porte le **look cible + mocks** jusqu’à fusion dans `UI.md` une fois la passe livrée.

> Périmètre : affichage de l’app hôte Preview uniquement. Pas le site marketing, pas seL4.

## Thèse

| Source | À conserver |
|--------|-------------|
| Proposition **B** (plaque raffinée) | Densité shell directe, chat-first, biseaux instrument fins |
| Proposition **C** (chambre split) | Palette chambre + graphe de traces MEMORY / CAPS / GPU / AGENTS |
| Contrainte utilisateur | Caps / mémoire / métriques **ne doivent jamais** voler en permanence la largeur du chat |

**Règle du tiroir chambre :** défaut **replié** (languette seule). Déplié ≈ 30 % à droite avec le graphe de traces. Grant/Deny sur le tiroir ou une bannière compacte — jamais une colonne permanente dans le fil de chat.

### Anti-objectifs

- Colonne Caps/Memory permanente à côté du chat
- Chrome mystique / bindu / or / purple-glow
- Cartes marketing douces ou pills rounded-full comme conteneurs primaires
- Changer l’IA du rail (Chat · Agents · Create · Memory) ou promouvoir les onglets testeur sur le rail

## Jetons

Réutiliser la palette chambre de [UI.md](UI.md) — void `#070b14`, ice-track `#5ee7ff`, signal `#2ef0c8`, hydrogen `#ff5a48`, paper `#e8eef6`. Pas de seconde palette app.

## Architecture d’information (inchangée)

| Couche | Entrées |
|--------|---------|
| Rail principal | Chat · Agents · Create · Memory |
| Plus | Notes · Library · Tasks · Models · Settings · Caps · Audit · Providers · modules DeclUI · Scenarios · Feedback |

Seul l’**affichage** (densité, tiroir chambre, graphe, plaque de confirmation) évolue dans cette passe.

## Catalogue des mocks

Assets canoniques : [`docs/assets/ui-update/`](../assets/ui-update/) (suivis dans git). Les explorations Impeccable locales (`.impeccable/mocks/`, gitignored) sont un brouillon de travail — pas le contrat partagé.

### Chrome partagé + Chat

| Id | Surface | Fichier | Acceptation |
|----|---------|---------|-------------|
| 00 | Shell rail + statut | [`00-shell-rail.png`](../assets/ui-update/00-shell-rail.png) | Rail Chat/Agents/Create/Memory + Plus ; barre d’état bas ; pas de colonne métriques permanente à droite |
| 01 | Chat · chambre repliée | [`01-chat-chamber-collapsed.png`](../assets/ui-update/01-chat-chamber-collapsed.png) | Chat plein largeur ; languette Chamber seule ; conversation = surface primaire |
| 02 | Chat · chambre dépliée | [`02-chat-chamber-expanded.png`](../assets/ui-update/02-chat-chamber-expanded.png) | Split ~70/30 ; LIVE CLOUD CHAMBER avec traces MEMORY/CAPS/GPU/AGENTS ; affordance Masquer/replier visible |
| 03 | Chat · Grant/Deny | [`03-chat-grant-deny.png`](../assets/ui-update/03-chat-grant-deny.png) | Phrase humaine + détail technique + Grant/Deny ; plaque sur tiroir ou bannière, pas plein écran |

### Rail principal

| Id | Surface | Fichier | Acceptation |
|----|---------|---------|-------------|
| 10 | Agents | [`10-agents.png`](../assets/ui-update/10-agents.png) | Liste / roster densité B ; rail Agents actif ; chambre optionnellement repliée |
| 11 | Create (Image Studio) | [`11-create.png`](../assets/ui-update/11-create.png) | Studio défaut : prompt, taille, steps, generate, historique — expert replié |
| 12 | Memory | [`12-memory.png`](../assets/ui-update/12-memory.png) | Memory en page pleine (pas colonne chat) ; rail Memory actif |

### Plus (overflow)

| Id | Surface | Fichier | Acceptation |
|----|---------|---------|-------------|
| 20 | Notes | [`20-notes.png`](../assets/ui-update/20-notes.png) | Espace Notes sous Plus ; palette chambre ; densité instrument |
| 21 | Library | [`21-library.png`](../assets/ui-update/21-library.png) | Liste Library ; pas look marketing en cartes |
| 22 | Tasks | [`22-tasks.png`](../assets/ui-update/22-tasks.png) | Liste / board tâches cohérent avec le shell |
| 23 | Models | [`23-models.png`](../assets/ui-update/23-models.png) | Onglet catalogue LLM représentatif ; labels locaux / honnêtes |
| 24 | Settings | [`24-settings.png`](../assets/ui-update/24-settings.png) | Groupe Me visible (langue, thème, échelle) ; Models/Trust en pairs |
| 25 | Caps | [`25-caps.png`](../assets/ui-update/25-caps.png) | Caps en page dédiée ; pas dupliqué en colonne chat permanente |
| 26 | Audit | [`26-audit.png`](../assets/ui-update/26-audit.png) | Journal / liste d’événements avec filets ice-track |
| 27 | Providers | [`27-providers.png`](../assets/ui-update/27-providers.png) | Liste backends/providers ; ids techniques secondaires |
| 28 | Scenarios | [`28-scenarios.png`](../assets/ui-update/28-scenarios.png) | Surface cohorte testeur sous Plus — pas peer du rail |
| 29 | Feedback | [`29-feedback.png`](../assets/ui-update/29-feedback.png) | Formulaire retour testeur sous Plus |
| 30 | Module DeclUI | [`30-module-declui.png`](../assets/ui-update/30-module-declui.png) | Module déclaratif installé sous Plus → Modules |

### États transverses

| Id | Surface | Fichier | Acceptation |
|----|---------|---------|-------------|
| 40 | Chat vide | [`40-empty-chat.png`](../assets/ui-update/40-empty-chat.png) | Session vide + composeur ; chambre repliée ; pas de faux contenu |
| 41 | Premier lancement · autorisations | [`41-first-run-allowance.png`](../assets/ui-update/41-first-run-allowance.png) | Après premier chat : récap autorisations ; pointe vers Plus → Scénarios pour testeurs |
| 42 | Create expert ouvert | [`42-create-expert-fold.png`](../assets/ui-update/42-create-expert-fold.png) | Mode expert ouvert (sd.cpp / VRAM / avancé) sans promotion sur le rail |

## Mapping code

| Id | `Tab` / surface | Entrée de rendu |
|----|-----------------|-----------------|
| 00 | Shell | rail + barre d’état dans `main.rs` |
| 01–03, 40 | `Tab::Chat` | `ui_chat` + futur tiroir chambre |
| 10 | `Tab::Agents` | `ui_agents` |
| 11, 42 | `Tab::Image` | Create / Image Studio |
| 12 | `Tab::Memory` | `ui_memory` |
| 20 | `Tab::Notes` | `ui_notes` |
| 21 | `Tab::Library` | `ui_library` |
| 22 | `Tab::Tasks` | `ui_tasks` |
| 23 | `Tab::Models` | `ui_models` / `models_page` |
| 24 | `Tab::Settings` | `ui_settings` |
| 25 | `Tab::Caps` | `ui_caps` |
| 26 | `Tab::Audit` | `ui_audit` |
| 27 | `Tab::Providers` | `ui_providers` |
| 28 | `Tab::Scenarios` | `ui_scenarios` |
| 29 | `Tab::Feedback` | `ui_feedback` |
| 30 | `Tab::Module(_)` | hôte DeclUI |
| 41 | Premier lancement | onboarding / récap autorisations |

Helpers rail / overflow : [`crates/aos-ui-egui/src/nav.rs`](../../crates/aos-ui-egui/src/nav.rs).

## Ordre d’implémentation suggéré

1. Polish jetons shell + tiroir chambre Chat (replié / déplié / Grant-Deny) — mocks 00–03
2. Pages rail principal — 10–12
3. Pages Plus — 20–27, 30
4. Surfaces testeur — 28–29
5. Vide / premier lancement / Create expert — 40–42

## Gate

**ready-for-implement** exige :

- [x] Thèse + anti-objectifs rédigés
- [x] IA inchangée vs [UI.md](UI.md)
- [x] Chaque ligne du catalogue a un PNG nommé sous `docs/assets/ui-update/`
- [x] Chaque ligne a des notes d’acceptation
- [x] Lié depuis [UI.md](UI.md)

L’implémentation egui est un **plan séparé** après ce gate.
