# UI / UX Preview

**Langue :** [English](../UI.md) | Français

Spécification produit durable pour l’**application hôte Preview** (`crates/aos-ui-egui`). Le site public (`website/`) conserve son design system orrery dans [DESIGN.md](../DESIGN.md) ; ce document régit le chrome in-app, la navigation et les libellés sur Preview Windows/Linux.

> Périmètre : shell hôte Preview 0.10.x. Pas l’image seL4 bootable.

## Objectifs

1. **Chat d’abord** — l’accueil par défaut est une conversation, pas une checklist testeur.
2. **Divulgation progressive** — tâches courantes sur le rail principal ; outils avancés et protocole cohorte sous **Plus**.
3. **Honnêteté sans bruit** — les limites Preview restent visibles sans dominer le chrome.
4. **Langage humain d’abord** — confirmations et libellés primaires expliquent l’intention ; les identifiants techniques restent en infobulles ou replis expert.
5. **Palette orrery partout** — réutiliser void / signal / hydrogen / paper du site public ; pas de nouvelle marque couleur.

## Architecture de l’information

### Navigation à deux niveaux

| Niveau | Éléments | Notes |
|--------|----------|-------|
| **Rail principal** | Chat · Agents · Créer · Mémoire | Toujours visible dans le rail gauche. Clavier : `Ctrl+1` … `Ctrl+4`. |
| **Plus (overflow)** | Notes · Tâches · Modèles · Paramètres · Caps · Audit · Providers · modules DeclUI · *(testeur)* Scénarios · *(testeur)* Retour | Section repliable. Les surfaces testeur ne sont **pas** des onglets pairs du rail. |

**Créer** correspond à l’onglet Studio Image (`Tab::Image`). Les contrôles expert sd.cpp restent dans le studio derrière **Mode expert** — pas promus sur le rail.

Les modules DeclUI installés avec `ui.mode=declarative_ui` apparaissent sous **Plus → Modules**, pas comme pairs du rail principal.

### Ce qui sort de la liste plate d’onglets

Avant cette spec, ~13 onglets latéraux traitaient Chat, Scénarios, Retour, Caps et Providers à égalité. Après :

- **Rail** = usage quotidien (chat, agents, image, mémoire).
- **Plus** = administration, confiance, extensions.
- **Scénarios + Retour** = protocole cohorte / testeur ([TESTER.md](TESTER.md)) ; accessibles via Plus, pas la destination post-tutoriel par défaut.

## Jetons de design (orrery)

Palette canonique (identique à [DESIGN.md](../DESIGN.md) et `website/styles.css`) :

| Jeton | Hex | Rôle |
|-------|-----|------|
| `void` | `#070b14` | Fond nuit, texte principal sur thèmes clairs |
| `signal` | `#3ee0c4` | Chrome actif : sélection, focus, rail actif, liens |
| `hydrogen` | `#e85d4c` | Avertissements, emphase destructive, CTA mise à jour |
| `paper` | `#e8eef6` | Corps sur void ; fond thème clair |
| `mute` | ~42 % signal dans void | Libellés inactifs, chrome secondaire |

### Mapping des thèmes (egui `Visuals`)

| Thème | Fond | Texte | Accent | Notes |
|-------|------|-------|--------|-------|
| **dark** (défaut) | void | paper | signal | Aligné sur l’éther nuit du site. |
| **light** | paper | void | signal | Plaques inversées ; texte void sur paper ≥ AA corps. |
| **soft** | paper + 3 % void | void @ 90 % | signal @ 85 % | Contraste adouci ; teintes orrery conservées. |
| **high_contrast** | void | paper | signal + focus 2px | Paper sur void ≥ 12:1 ; signal pour focus/sélection, pas seul indicateur d’état. |

Pas de glow violet, coins arrondis carte, ni « marque app » séparée — biseaux carrés, densité instrument.

## Chrome OS

### Barre d’état persistante (bas)

Toujours visible ; branchée sur prefs/runtime existants (pas de nouveau backend) :

| Segment | Source | Interaction |
|---------|--------|-------------|
| **Réseau** | `preferences.network_online` + `NetSetMode` runtime | Affiche Hors ligne / En ligne. Clic bascule l’opt-in (ex-case latérale). |
| **Modèle** | Défaut session ou premier modèle chargé dans `SystemMetrics` | Libellé ; clic ouvre Modèles. |
| **Capacités** | Nombre du dernier `cap.list` pour le détenteur actif (ou `—`) | Clic ouvre Caps. |
| **Mise à jour** | `var/updates/pending.json` ou `var/run/update_available.json` | Version en attente ou masqué. |
| **Langue** | `preferences.language` | Clic bascule EN ↔ FR. |

### Bandeau haut (réduit)

- **Honnêteté Preview** — une ligne discrète (`Preview {version} — app hôte, pas OS bootable`).
- **Notices agent, ligne download update, confirmations** — restent ici si actives.
- Tutoriel / Signaler / Dépannage — actions compactes.

### Confirmations

Ordre de présentation :

1. **Phrase humaine** — ex. « L’agent veut supprimer un fichier. »
2. **Détail technique** — id d’action, chemin cible (monospace, secondaire).
3. **Boutons** — **Autoriser** / **Refuser** (pas GRANT comme seul libellé).

Les confirmations d’extension OS (module.install, cap.request, …) gardent l’appel à revue caps/manifeste.

## Accessibilité

**Cible :** [WCAG 2.2 niveau AA](https://www.w3.org/TR/WCAG22/) pour le chrome Preview et les parcours primaires.

| Exigence | Comportement Preview |
|----------|----------------------|
| Contraste | Thème contraste élevé : corps paper sur void ; l’état n’est jamais codé par la couleur seule. |
| Focus | Contour `signal` 2px ; focus clavier visible sur rail et Plus. |
| Clavier | `Ctrl+1`…`Ctrl+4` rail principal ; `Ctrl+K` palette légère (onglets + hint slash). |
| Mouvement | Respect de `prefers-reduced-motion` là où il y a animation ; chrome egui statique. |
| Échelle | Préférence taille de police dans Paramètres → Général (tranche future). |
| Langue | Parité EN/FR selon [I18N.md](I18N.md). |

## Divulgation progressive

### Studio Image (Créer)

Surface par défaut : prompt, taille, steps, générer, historique.

Repli expert : backends sd.cpp, flow-shift, budget VRAM, upscale/img2img — capacité inchangée, pas sur le rail.

### Paramètres

Regroupement mental :

| Groupe | Contenu |
|--------|---------|
| **Moi** | Langue, thème, échelle police, mémorisation auto, mises à jour |
| **Modèles** | Routage, modèle par défaut, mode inférence, packs média |
| **Confiance** | Réseau, secrets, défauts agents, planifications, catalogue |

## Premier lancement

Séquence (remplace « fin tutoriel → onglet Scénarios ») :

1. Langue depuis la locale OS si possible (`en` / `fr`), modifiable à l’étape 2.
2. Un tour de chat — l’utilisateur envoie un message, l’assistant répond.
3. **Récap des autorisations** — ce que l’agent a pu faire (écriture mémoire, outils, caps).
4. Orienter les testeurs vers **Plus → Scénarios** pour le protocole cohorte ; Scénarios n’est pas l’accueil par défaut.

## Lignes directrices copy

| Éviter en surface primaire | Préférer | Nom technique dans |
|----------------------------|----------|-------------------|
| `local_only` | « Modèles locaux uniquement » | Infobulle Paramètres |
| `holder` | « Sujet » ou « Agent » | Panneau expert Caps |
| `capkd` | « Service des capacités » | Infobulle |
| `TTFT` seul | « Délai premier jeton » ou masquer sur la barre d’état | Ligne métriques Modèles |

Le jargon testeur reste acceptable dans Scénarios, Retour et export audit — pas sur le rail ni la barre d’état.

## Documents liés

- [PRODUCT.md](../PRODUCT.md) — positionnement et engagements marque
- [DESIGN.md](../DESIGN.md) — système orrery site public
- [I18N.md](I18N.md) — règles langue EN/FR
- [FIRST-RUN.md](FIRST-RUN.md) — installation et premier lancement
- [TESTER.md](TESTER.md) — protocole cohorte (Scénarios / Retour)

## Tranches d’implémentation

| Tranche | Statut |
|---------|--------|
| Spec (ce doc) | fait |
| Rail principal + overflow Plus | en cours |
| Barre d’état (réseau, modèle, caps, update, langue) | en cours |
| Mapping jetons orrery sur quatre thèmes | en cours |
| Premier lancement → chat + récap autorisations | fait |
| Regroupement Paramètres Moi/Modèles/Confiance | planifié |
| Préférence échelle police | planifié |
| Audit WCAG complet | planifié |
