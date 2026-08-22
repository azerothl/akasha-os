# GitHub Release — Akasha OS Preview

**Langue :** [English](../../packaging/RELEASE.md) | Français

## Automatique (recommandé)

Tag puis push :

```bash
git tag v0.11.0
git push origin v0.11.0
```

Le workflow [`.github/workflows/preview-release.yml`](../../.github/workflows/preview-release.yml)
construit Win + Linux **unifiés** (CUDA `aos-modeld` + CPU `aos-modeld-cpu`
dans le même zip ; sans GGUF), publie :

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `latest.json` (sha256 + métadonnées pour **deux** artefacts testeur)

`-CpuOnly` reste un hatch **builder**, pas un téléchargement testeur.

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
Akasha OS Preview 0.11.0 — E20 decode local

- E20 : KV Q8_0 sur GPU (F16 sur CPU) ; octets KV typés Placement
- Prefix cache : memory_seq_rm + warm llama_state_* (TTFT tour 2 / migrate E18)
- Speculative prompt-lookup en C1 ; batch N>1 inchangé
- Métriques : draft_accept / prefix_hit sur Models
- E21 : bande passante RAM + GPU/PCIe dans hardware.json ; ancres sémantiques de préfixe

Pas un OS bootable. Voir FIRST-RUN.md / INSTALL.md / TESTER.md
```
