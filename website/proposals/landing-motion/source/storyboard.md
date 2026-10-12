# Akasha OS : storyboard de l'accueil

Source de contenu : [site Akasha OS](https://azerothl.github.io/akasha-os/), consulté en français le 9 octobre 2026. Périmètre et synopsis validés par Lou : l'accueil uniquement. Les fenêtres produit et leurs données d'exemple sont recréées en HTML/CSS.

## Direction artistique

Un film d'interface au scroll, dans l'univers d'un système d'exploitation. Noir profond, cobalt lumineux, cyan technique, touches de corail. Typographie condensée et très grande, grilles, barres d'état, fenêtres hôte et schémas de capacités. La direction graphique est propre à Akasha OS, à la demande de Lou. La méthode de scrollytelling du skill est conservée.

La référence d'intention est la structure cinématique de [Cold Club / Scrolltide](https://www.scrolltide.co/templates/cold-club) : un parcours continu, des plans très lisibles et des transformations causées par le défilement.

| Chapitre | Idée | Desktop | Mobile | Durée |
|---|---|---|---|---|
| 1. Démarrage | Agents, modèles et outils deviennent des services système. | Trois services convergent vers le noyau Akasha ; le noyau s'efface pour révéler la fenêtre hôte. | Composition statique lisible, puis titre et description. | 300 vh |
| 2. Bureau | Chat, Create, Memory et Caps partagent le même rail. | La fenêtre entre et montre chaque surface à son tour, avec une pause par surface. | Quatre onglets tactiles changent réellement la fenêtre. | 360 vh |
| 3. Chat | Conversation locale, hors ligne par défaut. | Le message puis la réponse apparaissent dans la fenêtre. L'état hors ligne conclut la scène. | Échange complet en lecture naturelle. | 270 vh |
| 4. Create | Pack, recette, job, résultat. | La recette, la progression et l'image d'exemple apparaissent dans cet ordre. | Formulaire et résultat empilés, sans défilement intérieur. | 330 vh |
| 5. Caps | Capacité révocable, sortie réseau refusée, audit. | Le flux relie l'agent au jeton et s'arrête avant le réseau. Les états et l'audit se révèlent. | Schéma vertical complet, avec libellés visibles. | 270 vh |
| 6. Premier parcours | Installation et premières étapes. | Grand panneau d'appel à l'action, étapes et plateformes. | Étapes verticales et lien d'installation. | Section naturelle |

## Règles de mouvement

- Les progressions sont bornées, liées au scroll natif et réversibles.
- Chaque étape affiche un état complet avant l'étape suivante.
- Sous 760 px et avec `prefers-reduced-motion: reduce`, les scènes ne sont plus épinglées et tout le contenu reste accessible.
- Le paramètre `?scene=boot|desktop|chat|create|caps|install&progress=0..1` fige un chapitre pour la revue visuelle.

