# GitHub Release — Akasha OS Preview

**Langue :** [English](../../packaging/RELEASE.md) | Français

## Automatique (recommandé)

Tag puis push :

```bash
git tag v0.5.0
git push origin v0.5.0
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
Akasha OS Preview 0.5.0 — mémorisation auto depuis le chat (E14)

- Opt-in Settings : après chaque tour de chat, extraction de faits durables → Mémoire
- mem.extract (local_only, basse priorité) + filtre secrets (jamais de clés auto)
- Auto-lien updates/supersedes ; badge Memory [chat] ; toast à l'écriture
- Extract non bloquant / coalescé

Pas un OS bootable. Voir FIRST-RUN.md / INSTALL.md / TESTER.md
```
