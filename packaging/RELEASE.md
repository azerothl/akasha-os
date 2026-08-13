# GitHub Release — Agent OS Preview 0.1

## Assets à joindre

| Asset | Comment construire |
|-------|-------------------|
| `AgentOS-Preview-0.1-windows-x64.zip` | `.\packaging\build-preview.ps1` puis Compress-Archive |
| `AgentOS-Preview-0.1-linux-x64.tar.gz` | `./packaging/build-preview.sh` puis `tar czf` |

Machine de build : Windows + CUDA (hôte de dev) et WSL/Linux + CUDA.

## Notes de version (brouillon)

```
Agent OS Preview 0.1 — cohorte de test

Application hôte (Windows / Linux x64 + NVIDIA), pas un OS bootable.

- aos-session : démarre les services et l'UI egui
- Modèles embarqués : Qwen2.5-3B + 0.5B embed (offline)
- Retours : onglet « Retour » → var/feedback/ (aucun envoi auto)

Prérequis : nvidia-smi OK, ~4 Go disque.
Install : voir INSTALL.md dans l'archive.
Protocole : TESTER.md
```
