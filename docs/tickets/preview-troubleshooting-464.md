# Dépannage Preview et rejeu du cache hybride — #464

## Constat du 2 novembre 2026

Le rapport public mêlait trois problèmes distincts :

- `agent.list` renvoie tout le registre persistant. Les 315 enregistrements
  étaient présentés comme des agents actifs, et seuls les douze premiers
  étaient examinés. Les six échecs cités dataient d'août ou septembre.
- Les journaux manuels et les anciens échecs de démarrage du 15 septembre
  étaient comptés comme des anomalies récentes, sans tenir compte de leur date.
- Les erreurs de chat étaient remplacées par une phrase générique. L'heure et
  la session connues par l'interface étaient abandonnées lors du diagnostic.

La lecture ciblée du journal moteur a également retrouvé un défaut réel :

```text
the tokens of sequence 0 ... have inconsistent sequence positions
last position ... X = 3675
starting position ... Y = 3664
for M-RoPE ... X < Y
llama_decode ... ret = -1
```

Ces erreurs concernent le cache du modèle Qwen3.5 hybride. Le contrat natif de
`llama_memory_seq_rm` autorise explicitement un retour `false` lorsqu'une partie
de la séquence ne peut pas être retirée. Le wrapper ignorait ce résultat, puis
tronquait quand même son miroir de tokens : les positions natives et Rust ne
correspondaient plus au rejeu du prompt ou après rejet d'un draft.

## Correctifs

Le moteur vérifie désormais le résultat de chaque retrait de séquence. Il
réinitialise son cache et son miroir ensemble lorsque le retrait est impossible.
Le rafraîchissement des logits du dernier token est intégré à la préparation du
préfixe, ce qui évite deux retours arrière successifs sans décodage intermédiaire.
Les modèles récurrents/hybrides utilisent le décodage standard dans le chemin
lookup ; la métrique expose `cache-incompatible`. Les modèles à cache compatible
conservent le lookup et la réutilisation de préfixe. Un échec de vérification de
draft reconstruit les tokens déjà engagés avant le repli standard.
La collation des résultats en batch distingue également une vraie erreur native
`Decode(-1)` du résultat initialement non renseigné, afin de conserver l'échec.

Le rapport de dépannage :

- distingue les agents actifs des états terminaux sauvegardés ;
- examine tous les échecs récents avant de limiter la liste affichée ;
- utilise une fenêtre explicite de trente minutes, les dates de sauvegarde
  locales et les dates de modification des journaux ;
- montre les fichiers anciens comme un historique, sans les compter comme
  panne actuelle ; une date absente ou future ne prouve pas un incident récent ;
- lit au maximum 64 Kio et quarante lignes par journal, sur trente-deux fichiers,
  avec priorité aux fichiers les plus récents et un plafond global du rapport ;
- conserve l'heure et la session des erreurs de chat, regroupe les répétitions
  identiques dans la même session et ajoute un détail technique borné lorsqu'il
  ne contient pas de chemin ou de secret détecté ;
- classe les échecs de décodage sous `model.decode_failed` ;
- traite l'absence de `nvidia-smi` comme une information matérielle indisponible.

La date d'un fichier n'est pas l'horodatage de chacune de ses lignes. Le rapport
le dit explicitement. Les causes historiques n'ont pas été supprimées, et une
erreur générique déjà enregistrée ne permet pas de reconstruire son message brut.
La détection de secrets du détail technique reste une heuristique conservatrice.

## Vérification reproductible

`prefix_replay_smoke` charge un GGUF local, exécute deux prompts identiques,
puis deux prompts raccourcis identiques, avec les mêmes paramètres et le lookup
demandé. Il vérifie le succès et l'égalité des sorties rejouées. Il ne télécharge
aucun poids et refuse un offload GPU demandé sans périphérique natif disponible.

```text
cargo run -p aos-llama --no-default-features --example prefix_replay_smoke --locked -- <absolute-Qwen3.5-4B-Q4_K_M.gguf> 0
cargo run -p aos-llama --example prefix_replay_smoke --locked -- <absolute-Qwen3.5-4B-Q4_K_M.gguf> 999
```

Sur le même Qwen3.5-4B Q4_K_M, le code initial échouait sur les deux rejeux
(deux succès sur quatre). Après correction, les quatre cas CPU réussissent et
les sorties rejouées sont identiques. Le témoin Spark-X2.5-4B Q4_K_M réussit
également les quatre cas CPU ; ses rejeux conservent des hits de préfixe non nuls
et le premier scénario propose des tokens en spéculation.

Les mêmes quatre cas réussissent pour chacun des deux modèles avec CUDA sur la
RTX 4080 SUPER (un seul GPU). Les journaux natifs attestent l'offload et les
buffers CUDA, et les rapports vérifient l'égalité des deux paires de sorties.
Spark conserve les hits de préfixe de 31 et 22 tokens et les drafts ; Qwen expose
le repli `cache-incompatible`. Le [rapport JSON](evidence/2026-11-02-troubleshooting-464.json)
conserve les résultats, les empreintes des poids et celles des journaux locaux.
Ces essais ne couvrent ni Metal ni le placement sur plusieurs GPU.

Les vérifications locales passent :

- `cargo test -p aos-ui-egui --locked` : 550 succès, un test ignoré ;
- `cargo test -p aos-llama --no-default-features --lib --locked` : 20 succès ;
- `cargo clippy -p aos-llama -p aos-model -p aos-ui-egui --no-default-features --all-targets --locked -- -D warnings` : succès.

Les tests du dépannage couvrent notamment les dates anciennes/futures, la lecture
bornée, la priorité aux journaux récents, l'échec récent situé après douze agents
historiques et le regroupement des erreurs par session.

## Acceptation restante

La recette dans l'application Preview installée et la publication de ces
correctifs restent à réaliser. Les mesures synthétiques valident le chemin
technique du cache, pas la qualité générale des réponses. Le repli peut augmenter
le temps de préremplissage sur les modèles hybrides ; aucun gain de vitesse n'est
annoncé. Le ticket reste ouvert jusqu'à l'intégration et à la recette appropriée.
