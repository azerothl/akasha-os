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
var/          données locales (audit, notes, sessions, memory, feedback, secrets)
```

## Premier lancement

1. `aos-session` vérifie NVIDIA, crée `var/` (dont `var/sessions`), écrit `etc/*.yaml`
   avec `net_mode: offline_strict` et `sessions_dir: var/sessions`.
2. Démarre bus → capkd → auditd → modeld → platformd → agentd.
3. Ouvre l'UI egui (onboarding si premier run).
4. Fermer l'UI arrête les daemons.

## Réseau & recherche (optionnel)

Par défaut le réseau est **coupé** (`offline_strict`). Dans l'UI, case
**Autoriser le réseau** pour activer `web.search` / `net.fetch`.

Backend search :

1. **DuckDuckGo HTML** (sans clé) — défaut.
2. **Brave Search** si vous ajoutez dans `var/secrets/keys.yaml` :

```yaml
keys:
  brave_search_api_key: "BSA..."
```

La clé n'est jamais exposée aux agents (`service:platformd` seulement).

### Issues GitHub (retours UI)

L'onglet **Retour** crée une issue sur
[azerothl/akasha-os](https://github.com/azerothl/akasha-os/issues) :

1. **Sans jeton** (défaut) : ouverture du formulaire GitHub prérempli dans
   le navigateur — cliquez **Submit new issue** (compte GitHub).
2. **Création directe** si `gh` est authentifié, ou un PAT `issues:write` :

```yaml
keys:
  github_token: "ghp_..."
```

Les rapports **security** restent locaux (pas d'issue publique).

Téléchargements et fichiers générés : `var/storage/data/downloads/`
(chemins logiques `/downloads/**`).

## Dépannage

| Symptôme | Action |
|----------|--------|
| `GPU NVIDIA requis` | Installer / mettre à jour le driver ; tester `nvidia-smi` |
| `healthcheck échoué` | Voir `var/run/*.pid` ; relancer ; logs stderr des daemons |
| Modèle introuvable | Copier les GGUF dans `share/models/` (noms exacts du README package) |
| Bus injoignable (UI seule) | Toujours lancer via `aos-session`, pas `aos-ui-egui` seul |
| `réseau désactivé` | Cocher **Autoriser le réseau** dans l'UI |
| Sessions vides au restart | Vérifier `var/sessions/<id>/` et `sessions_dir` dans `etc/platformd.yaml` |

## Build depuis les sources (mainteneurs)

```powershell
# Windows (machine CUDA)
.\packaging\build-preview.ps1

# Linux
./packaging/build-preview.sh
```

Les artefacts GPU ne sont pas produits en CI sans runner CUDA : job manuel
documenté, puis upload GitHub Release.

## Licence

AGPL-3.0-only (`LICENSE`) ; licence commerciale possible
(`LICENSE-COMMERCIAL.md`). Conservez `NOTICE` avec toute redistribution.
