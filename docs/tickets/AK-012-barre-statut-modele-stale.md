# AK-012 — Barre de statut affichant le mauvais modèle

- Priorité : P2
- Statut : corrigé, déployé et vérifié visuellement
- Zone : UI native egui, barre de statut Chat

## Constat

Sur la fenêtre Preview, le sélecteur de session affichait `local:qwen3.5-9b-instruct` tandis que la barre inférieure affichait `Modèle: local:sd-v1-5`. Le second provenait du premier modèle présent dans les métriques (un modèle média chargé), pas du contexte de chat actif.

## Correction

`UiApp::status_model_name` privilégie désormais le `model_id` de la session active, puis utilise les métriques comme repli lorsqu’aucune session ne le définit. Cela aligne le libellé de statut sur le sélecteur visible et évite de modifier le mauvais modèle.

## Vérification

Le changement compile avec `cargo check --workspace` et le smoke-test UI reste vert. Après déploiement, une capture native montre le sélecteur et la barre inférieure tous deux sur `local:qwen3.5-9b-instruct`.
