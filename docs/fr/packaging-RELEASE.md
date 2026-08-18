# GitHub Release — Akasha OS Preview

**Langue :** [English](../../packaging/RELEASE.md) | Français

## Automatique (recommandé)

Tag puis push :

```bash
git tag v0.7.0
git push origin v0.7.0
```

Le workflow [`.github/workflows/preview-release.yml`](../../.github/workflows/preview-release.yml)
construit Win + Linux **CUDA** et **CPU** (sans GGUF), publie :

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-windows-x64-cpu.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `AgentOS-Preview-<ver>-linux-x64-cpu.tar.gz`
- `latest.json` (sha256 + métadonnées pour **les quatre** artefacts)

Déclenchement manuel : Actions → **preview-release** → Run workflow.

## Manuel

| Asset | Commande |
|-------|----------|
| Windows GPU | `.\packaging\build-preview.ps1 -SkipModels -RequireCuda` puis Compress-Archive |
| Windows CPU | `.\packaging\build-preview.ps1 -SkipModels -CpuOnly` puis Compress-Archive |
| Linux GPU | `SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh` puis `tar czf` |
| Linux CPU | `CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh` puis `tar czf` |

Les GGUF sont téléchargés au **premier run** via `share/models/manifest.json`.

## Notes de version (brouillon)

```
Akasha OS Preview 0.7.0 — hôte d’UI de module déclarative (E15)

- Onglets egui dynamiques pour les modules installés avec ui.mode=declarative_ui (arbre de widgets fermé dans ui/index.html)
- Intent module.ui valide fail-closed ; bind des résultats d’outils ; bouton/formulaire → module.invoke sous revue de caps
- module.scaffold champ ui optionnel ; package/compile écrivent un vrai arbre (défaut heading + form + table)
- JSON Schema : docs/bridge/aos-proto-decl-ui.json

Pas un OS bootable. Voir FIRST-RUN.md / INSTALL.md / TESTER.md
```
