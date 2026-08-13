# Installation — Agent OS Preview 0.1

**Ce n'est pas un OS bootable.** Preview tourne **sur Windows ou Linux x64**
avec GPU **NVIDIA** (échafaudage hôte, ADR 0001). seL4 = piste séparée.

## Prérequis

| | |
|--|--|
| OS | Windows 10/11 x64 **ou** Linux x64 (glibc récent) |
| GPU | NVIDIA avec driver récent (`nvidia-smi -L` OK) |
| Disque | ~4 Go libre (binaires + GGUF 3B + 0.5B) |
| CUDA | Runtime compatible avec le build (driver suffit en pratique) |

Pas de macOS, pas de mode CPU-only en 0.1.

## Windows

1. Télécharger l'archive `AgentOS-Preview-0.1-windows-x64` (GitHub Releases).
2. Décompresser, puis :
   ```powershell
   .\install.ps1
   ```
   Installe sous `%LOCALAPPDATA%\AgentOS-Preview` et crée un raccourci Bureau.
3. Lancer **Agent OS Preview**.

Sans installateur :
```powershell
$env:AOS_HOME = (Resolve-Path .)
.\bin\aos-session.exe
```

## Linux

1. Télécharger `AgentOS-Preview-0.1-linux-x64.tar.gz`, extraire.
2. ```bash
   ./install.sh
   ```
   Préfixe : `~/.local/share/agentos-preview`, lien `~/.local/bin/agentos-preview`.
3. Lancer `agentos-preview` (ou l'entrée du menu applications).

## Contenu du paquet

```
bin/          aos-session, daemons, aos-ui-egui
share/models/ GGUF embarqués (3B instruct + 0.5B embed)
share/modules/notes.aospkg/
data/models/  catalog.yaml
etc/          généré au premier lancement par aos-session
var/          données locales (audit, notes, feedback)
```

## Premier lancement

1. `aos-session` vérifie NVIDIA, crée `var/`, écrit `etc/*.yaml`.
2. Démarre bus → capkd → auditd → modeld → platformd → agentd.
3. Ouvre l'UI egui (onboarding si premier run).
4. Fermer l'UI arrête les daemons.

## Dépannage

| Symptôme | Action |
|----------|--------|
| `GPU NVIDIA requis` | Installer / mettre à jour le driver ; tester `nvidia-smi` |
| `healthcheck échoué` | Voir `var/run/*.pid` ; relancer ; logs stderr des daemons |
| Modèle introuvable | Copier les GGUF dans `share/models/` (noms exacts du README package) |
| Bus injoignable (UI seule) | Toujours lancer via `aos-session`, pas `aos-ui-egui` seul |

## Build depuis les sources (mainteneurs)

```powershell
# Windows (machine CUDA)
.\packaging\build-preview.ps1

# Linux
./packaging/build-preview.sh
```

Les artefacts GPU ne sont pas produits en CI sans runner CUDA : job manuel
documenté, puis upload GitHub Release.

## Suite testeurs

Voir [docs/TESTER.md](docs/TESTER.md) — protocole cohorte et retours depuis l'UI.
