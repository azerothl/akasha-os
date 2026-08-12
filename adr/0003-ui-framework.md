# ADR 0003 — Framework UI

> Date : 12/08/2026
> Statut : **provisoire** (P1) — à confirmer avant P2
> Références : `plan-developpement-phases.md` P1.6 + risque « Choix de framework UI figeant mal », `specs-techniques.md` §8, §14

---

## 1. Contexte

Le plan P1 prévoyait : « Prototype egui ET iced sur une semaine, décision ADR
0003 avant de continuer ». Constat d'exécution de la session P1 : les gates P1
portent sur l'inférence, l'offload et l'isolation des agents — pas sur le rendu
graphique. Concentrer l'effort sur le chemin critique du gate est prioritaire.

## 2. Décision (provisoire)

- **P1 (démo) : TUI ratatui** (`crates/aos-ui`) — shell conversationnel +
  dashboard ressources. Justifications : zéro risque de build, démonstration
  possible partout (SSH, CI, WSL), la logique UI est découplée du bus IPC
  (tout passe par les intents `model.*` / `agent.*`), donc le frontend est
  remplaçable sans toucher aux services.
- **P2+ (produit) : réévaluation formelle egui vs iced vs tauri**, sur la base
  d'un prototype commun minimal (chat + dashboard + panneau agents) branché
  sur le même bus.

## 3. Critères d'évaluation retenus pour le prototype comparatif (P2)

| Critère | Pourquoi |
|---------|----------|
| Accès au bus IPC (tokio) | Les trois coexistent avec tokio ; iced est async-natif, egui demande un pont thread, tauri est webview+JS |
| Dashboard temps réel (jauges VRAM/RAM) | egui immédiat = trivial ; iced = redraw par subscription ; tauri = réactivité web |
| Accessibilité (F-UI-08) | egui : AccessKit intégré ; iced : partiel ; tauri : hérite du web (bon) |
| Mode `declarative_ui` des modules (§7.3) | tauri/webview est le plus naturel pour du contenu module sandboxé ; egui/iced demandent un langage déclaratif maison |
| Port aarch64 + microkernel (P4) | egui/iced (pur Rust) plus portables qu'une webview système |
| Vélocité de développement UI riche (panneau transparence, control bar) | à mesurer par le prototype |

## 4. Mitigation du risque (plan P1)

Le risque « figeage mal » est traité **structurellement** dès P1 : l'UI ne
connaît que le bus IPC (CBOR, intents) — aucun couplage aux internals des
services. Le coût de remplacement d'un frontend est borné par cette frontière.

## 5. Échéance

Avant le début de P2 : prototype egui + iced sur le cas « chat + dashboard +
liste d'agents », mesure des critères du §3, mise à jour de cet ADR (statut
« accepté ») avec le choix final.
