# Assistant de mix hôte (Path A / #432)

**Langue :** [English](../mix-assistant.md) | Français

Hôte Preview des contrats mix akasha-model (descripteurs, catalogue de
presets, lots `mix_proposal` confirmables). **Path A seulement** — pas de
scorer MASK, pas d’encodeur audio, pas d’audio protégé dans git.

## Flux

Descripteurs JSON calculés **hôte** (aucun WAV dans le guest) →
`mix.propose` (preset + confiance) → confirmation humaine → `mix.apply`
(cap `mix.apply:*`) → `mix.undo` = inverse du lot.

Le DAW reste **hors sandbox**. Caps fail-closed.

Module référence : [`community/modules/song-maker/`](../../community/modules/song-maker/).
Détail EN : [mix-assistant.md](../mix-assistant.md).
