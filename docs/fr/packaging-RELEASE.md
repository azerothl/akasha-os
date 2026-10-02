# GitHub Release — Akasha OS Preview

**Langue :** [English](../../packaging/RELEASE.md) | Français

## Automatique (recommandé)

Tag puis push :

```bash
git tag v0.19.0
git push origin v0.19.0
```

Le workflow [`.github/workflows/preview-release.yml`](../../.github/workflows/preview-release.yml)
construit Win + Linux **unifiés** (CUDA `aos-modeld` + CPU `aos-modeld-cpu`
dans le même zip ; sans GGUF), publie :

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `AgentOS-Preview-<ver>-macos-arm64.zip` (Apple Silicon + Metal ; CI non signé)
- `latest.json` (sha256 + métadonnées pour les artefacts testeur)

`-CpuOnly` reste un hatch **builder**, pas un téléchargement testeur.

Déclenchement manuel : Actions → **preview-release** → Run workflow.

- **macOS seul sur une Release existante** (sans retag) : `macos_only` = true, `release_version` = `0.19.0`, `upload_release` = true — attache le zip Apple Silicon et met à jour `latest.json` sur cette Release.
- **Rebuild complet** : `create_release` = true, `macos_only` = false.

## Manuel

| Asset | Commande |
|-------|----------|
| Windows GPU | `.\packaging\build-preview.ps1 -SkipModels -RequireCuda` puis Compress-Archive |
| Windows CPU | `.\packaging\build-preview.ps1 -SkipModels -CpuOnly` puis Compress-Archive |
| Linux GPU | `SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh` puis `tar czf` |
| Linux CPU | `CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh` puis `tar czf` |
| macOS Apple Silicon | `SKIP_MODELS=1 ./packaging/build-preview-macos.sh` puis `zip -r` (voir [packaging-MACOS-BUILD.md](../../packaging/MACOS-BUILD.md)) |

Les GGUF sont téléchargés au **premier run** via `share/models/manifest.json`.

## Checklist pré-release

Avant de taguer une release Preview `v*` :

- [ ] Les bumps de version (`VERSION`, workspace, site) correspondent au tag
- [ ] `docs/FEATURES.md` / `docs/STATUS.md` (et miroirs FR) décrivent cette release
- [ ] **Carte d’architecture à jour** — rafraîchir
  [`docs/architecture-graph.json`](../architecture-graph.json) et
  [`docs/architecture-map.html`](../architecture-map.html) pour que nœuds,
  arêtes et flux correspondent aux daemons/modules livrés ; mettre
  `meta.version` à cette release. Politique :
  [architecture.md](architecture.md).
  Omettre uniquement si la disposition des processus est inchangée depuis le
  tag précédent (le noter dans les notes de release).

## Notes de version (brouillon)

```
Akasha OS Preview 0.19.0 — aos-serverd + Dev-assistant P0

- aos-serverd : cycle de vie headless (ADR 0012), watchdogs, contrôle local, intake agents
- Services OS opt-in (systemd user / launchd / logon Windows) + aos-serverd dans les zips Preview
- Supervise opt-in aos-mcpd / aos-bridged ; aos-session attach UI-only
- Dev-assistant P0 : workspace.bind / fs.search / fs.apply_patch + contrat module
- S’appuie sur 0.18.0 (Memory V2, E22/E23, harness + aos-mcpd, Studio Illustration 0.7.23)

Pas un OS bootable. Voir FIRST-RUN.md / INSTALL.md / TESTER.md
Suivi tag : créer `v0.19.0` après merge de ce PR docs/VERSION (ne pas taguer depuis ce PR seul).
```
