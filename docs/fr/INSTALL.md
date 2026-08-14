# Installation — Agent OS Preview

**Langue :** [English](../../INSTALL.md) | Français

**Ce n'est pas un OS bootable.** Preview tourne **sur Windows ou Linux x64**
avec GPU **NVIDIA** (échafaudage hôte, ADR 0001). seL4 = piste séparée.

## Prérequis

| | |
|--|--|
| OS | Windows 10/11 x64 **ou** Linux x64 (glibc récent) |
| GPU | NVIDIA avec driver récent (`nvidia-smi -L` OK) |
| Disque | ~8 Go libre (pack mid recommandé) |
| CUDA | Runtime embarqué dans le paquet (driver suffit) |

Pas de macOS, pas de mode CPU-only en 0.1.

## Windows

1. Télécharger `AgentOS-Preview-<ver>-windows-x64.zip` depuis
   [GitHub Releases](https://github.com/azerothl/akasha-os/releases).
2. Décompresser, puis :
   ```powershell
   .\install.ps1
   ```
   Installe sous `%LOCALAPPDATA%\AgentOS-Preview` (préserve `var/` / `etc/`
   en cas de mise à jour).
3. Lancer **Agent OS Preview**.

Sans installateur :
```powershell
$env:AOS_HOME = (Resolve-Path .)
.\bin\aos-session.exe
```

## Linux

1. Télécharger `AgentOS-Preview-<ver>-linux-x64.tar.gz`, extraire.
2. ```bash
   ./install.sh
   ```
   Préfixe : `~/.local/share/agentos-preview` (overlay non destructif).
3. Lancer `agentos-preview`.

## Contenu du paquet

```
bin/            daemons + CUDA runtime
share/models/   manifest.json (GGUF téléchargés au 1er run)
share/modules/  notes.aospkg, ext-rt.aospkg
share/skills/   skills Preview
data/models/    catalog.yaml
VERSION         semver du build
FIRST-RUN.md    tutoriel texte
var/            données locales (créé au run)
```

## Premier lancement

1. `aos-session` vérifie NVIDIA + disque et sonde la VRAM.
2. **Choix des modèles** (1er run) : confirmer le pack auto-best, télécharger
   les GGUF (`catalog-offerings.json`).
3. Démarre bus → capkd → auditd → modeld → platformd → agentd.
4. Ouvre l'UI egui + **tutoriel**.
5. Fermer l'UI arrête les daemons.

Voir [FIRST-RUN.md](FIRST-RUN.md). Onglet **Models** pour d'autres profils.

## Mises à jour

Un bandeau apparaît dans l'UI si une Release plus récente existe.
**Télécharger** écrit l'archive dans `var/updates/` ; le **prochain**
lancement applique `bin/` + `share/` sans toucher à `var/` ni écraser
`etc/*.yaml` (fichiers `.new` si besoin).

## Réseau & recherche (optionnel)

Par défaut le réseau est **coupé** (`offline_strict`). Case
**Autoriser le réseau** pour `web.search` / `net.fetch`.

```yaml
# var/secrets/keys.yaml (optionnel)
keys:
  brave_search_api_key: "BSA..."
  github_token: "ghp_..."   # issues Feedback en un clic
```

## Dépannage

| Symptôme | Action |
|----------|--------|
| GPU NVIDIA requis | Driver NVIDIA ; `nvidia-smi -L` |
| Échec modèles | Réseau pour HF, ou copier les GGUF dans `share/models/` |
| healthcheck échoué | `var/run/*.stderr.log` (bouton **Dépannage**) |
| Bus injoignable | Toujours via `aos-session` |

## Build / CI (mainteneurs)

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda
```

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
```

GitHub Actions : `.github/workflows/preview-release.yml` (tags `v*`).

## Licence

AGPL-3.0-only (`LICENSE`) ; licence commerciale possible
(`LICENSE-COMMERCIAL.md`). Conservez `NOTICE` avec toute redistribution.
