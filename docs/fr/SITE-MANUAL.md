# Manuel site vs docs dépôt (source unique)

**Langue :** [English](../SITE-MANUAL.md) | Français

> La doc publique Preview reste en **HTML statique** sous `website/` (décision
> PRODUCT). Cette page dit quelle surface est canonique quand HTML et Markdown
> se chevauchent. Carte machine :
> [`packaging/site-docs-map.json`](../../packaging/site-docs-map.json).
> Garde-fou : `./packaging/check-site-docs.ps1`.

## Règles

1. **Pas de Docusaurus / MD→HTML** pour le site public tant que PRODUCT garde
   le stack HTML/CSS/JS.
2. **Éditer la SoT d’abord**, puis le jumeau (MD offline) ou le digest (HTML
   court).
3. **FR site** = `data-lang` dans le même HTML. **FR dépôt** = `docs/fr/`
   (certains noms diffèrent — voir [I18N.md](I18N.md)).
4. Quand un chapitre site pointe vers la profondeur dépôt, les pages bilingues
   doivent offrir le miroir `docs/fr/…` s’il existe.

## HTML = source de vérité (how-to public)

| Chapitre site | Jumeau offline |
|---------------|----------------|
| `website/install.html` | `INSTALL.md` · `fr/INSTALL.md` |
| `website/docs/first-run.html` | `FIRST-RUN.md` · `fr/FIRST-RUN.md` |
| `website/docs/skill.html` | `write-a-skill.md` · `fr/write-a-skill.md` |
| `website/docs/module.html` | `write-a-module.md` · `fr/write-a-module.md` |
| `website/docs/feedback.html` | `TESTER.md` |
| `website/community.html` | `community.md` |
| `use` / `network` / `troubleshoot` / `limits` / `whats-new` | site-only (profondeur FEATURES / specs) |

Les jumeaux offline doivent dire **Canonique :** et lier le chemin site.

## Markdown = source de vérité (digests développeur)

| Digest HTML | MD canonique |
|-------------|--------------|
| `module-sdk.html` | `module-sdk.md` · `fr/module-sdk.md` |
| `lan-cluster.html` | `lan-cluster.md` · `fr/cluster-lan.md` |
| `rich-apps.html` | `rich-app-contract.md` |
| `devices.html` | `device-capture.md`, `device-usb.md` |
| `build.html` | `INSTALL.md` (build from source) |

Specs, ADR, phases et tickets restent **dépôt seulement**.
