# Reprise des builds Preview — 2 novembre 2026

Les PR #471 et #472 sont fusionnées dans `main` (`2256386`). Les tickets
#439, #444, #247, #460, #434 et #435 restent ouverts ; aucune validation de
l'interface native n'est déduite de ces fusions.

## Run sans publication

Le [run 41527122297](https://github.com/azerothl/akasha-os/actions/runs/41527122297)
utilise exactement `22563860e396e46df5984f5712238e3318ec134d`, avec :

```text
macos_only=false
include_macos_in_full_build=true
create_release=false
upload_release=false
cache_profile=reuse
module_reuse_mode=prefer
```

Ces options lancent les trois builds sans créer de tag, publier de release ni
mettre à jour ses assets. Ce run doit observer les nouveaux rapports, pas prouver
un gain de temps ou une installation native sur les machines de cohorte.

## Blocage Linux et correctif

Le [job Linux 125557622189](https://github.com/azerothl/akasha-os/actions/runs/41527122297/job/125557622189)
échoue avant le build, dans `Install module index verifier`, avec :

```text
python: not found
Process completed with exit code 127
```

Le conteneur CUDA Ubuntu 22.04 installe `python3`, mais l'étape ajoutée pour
vérifier les index appelle `python` sans préparer cet interpréteur ni pip.
Le correctif ajoute `python3-venv`, crée un environnement dédié dans
`RUNNER_TEMP`, installe le vérificateur via son Python et exporte son répertoire
`bin` pour les étapes suivantes. Il évite de dépendre d'un alias système ou de
modifier l'environnement Python géré par la distribution.

Le script exact extrait du workflow corrigé a été exécuté dans Ubuntu 24.04
sous WSL (Python 3.12.3), sur ce même PC. L'environnement a été créé, le chemin
exporté a été vérifié et les dix tests de packaging ont réussi. Le répertoire
temporaire de test a été supprimé par son propriétaire. Ce test ne constitue
ni une exécution dans l'image CUDA Ubuntu 22.04, ni une recette de cohorte.

La relance du job Linux corrigé reste nécessaire après publication de la
branche de revue. Les builds Windows/macOS du run initial peuvent continuer à
fournir leurs mesures ; aucun rapport Linux de build complet n'est revendiqué
pour cet échec précoce. Les [preuves structurées](evidence/2026-11-02-release-python-bootstrap.json)
conservent les références et les empreintes des journaux.
