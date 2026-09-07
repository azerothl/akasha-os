# Configuration du cluster LAN

Le cluster LAN est expérimental et désactivé par défaut. Il utilise
uniquement les nœuds ajoutés puis explicitement appairés ; il ne découvre ni
ne contacte de pairs sur Internet.

## Configuration dans l’interface

1. Ouvrez **Paramètres → Modèles** et activez **Cluster LAN**.
2. Renseignez l’identité du nœud local, son adresse d’écoute annoncée et le
   nom du secret de clé de session. Le nom par défaut est
   `lan_cluster_session_key`.
3. Renseignez l’empreinte de clé publique locale, activez **Découvrir
   automatiquement les candidats LAN** et utilisez le même port UDP de
   découverte sur les hôtes concernés.
4. Activez **Autoriser le transfert du modèle vers le LAN** uniquement après
   avoir accepté que le fichier GGUF puisse être copié vers un nœud appairé.
   Cette autorisation est indépendante de l’activation du cluster et désactivée
   par défaut.
5. Dans **Paramètres → Coffre à secrets**, saisissez une clé aléatoire de
   32 octets sous forme de 64 caractères hexadécimaux, puis enregistrez-la
   sous ce nom. Sa valeur est en écriture seule dans l’interface.
6. Dans la section Cluster LAN, saisissez l’identifiant du nœud distant, son
   nom, son adresse LAN (`192.168.x.y:port`, ainsi que loopback et link-local)
   et son empreinte de clé publique.
7. Cliquez sur **Ajouter le nœud**, ou attendez un candidat découvert, puis sur **Appairer** après vérification
   de l’empreinte. **Révoquer** exclut immédiatement un nœud des prochains
   travaux.

L’identité locale, l’adresse annoncée et le nom du secret sont conservés dans
les préférences locales. L’inventaire et l’état de confiance sont persistés
dans `var/run/lan-pairing.json`.

## Règles de clé et d’appairage

- Utilisez une clé de session aléatoire différente pour chaque groupe LAN de
  confiance.
- Ne collez jamais de prompt, de modèle ou de clé privée dans le champ
  d’empreinte.
- Un nœud n’est éligible aux travaux qu’après appairage.
- Une empreinte modifiée pour un nœud connu est refusée.
- Une révocation reste effective après redémarrage.
- Les annonces sont des indications non authentifiées. L’IP source doit
  correspondre à l’adresse LAN annoncée ; un nouvel hôte reste toujours
  `Unpaired` jusqu’à validation manuelle.

## État d’exécution actuel

`model.cluster.plan` calcule le placement des shards. Le champ optionnel
`layer_pipeline=true` demande des segments de couches contigus par worker,
nécessaires pour préserver l’ordre des activations. Le service interne
explicite `model.cluster.dispatch` peut envoyer des assignments typées et
chiffrées à un worker appairé lorsque son listener et la clé de session sont
disponibles. Le worker accuse réception des assignments, des heartbeats et
des annulations. `model.cluster.recover` peut renvoyer les assignments après
une perte de nœud, et `model.cluster.cancel` propage l’annulation si le nom
du secret de session est fourni.
Le coordinateur refuse la récupération et la replanification d’un travail
annulé ou échoué : une nouvelle tentative doit utiliser un nouvel identifiant.
Lors d’une récupération, les assignments des nœuds révoqués sont retirées et
leurs shards sont réaffectés ; sans survivant appairé, tous les shards concernés
sont conservés dans la liste des shards non assignés.

`aos-modeld` ouvre le listener LAN uniquement lorsque le cluster est activé
et que la clé de session est disponible dans le coffre. L’ACK d’un assignment
peut être donné avant le chargement du modèle ; le worker matérialise ensuite
le GGUF complet ou le segment sparse demandé et charge llama.cpp à la demande pour le prefill/decode
token-level. Les messages `Prefill`, `Decode`,
tokens, inférence texte explicite et pages KV sont définis, chiffrés et bornés.
`model.cluster.infer_chat` permet à un appelant autorisé de transmettre un
prompt au nœud choisi et retourne le texte avec ses métriques ; il exige
`allow_sensitive_data=true`, un secret de session valide et un nœud appairé.
Une annulation authentifiée reçue sur une connexion séparée est propagée au
drapeau du contexte de génération et termine le travail à la frontière de
token.
`model.cluster.kv_transfer` peut exporter l’état KV de la séquence 0 d’un
worker, le transporter par pages CBOR chiffrées de 512 KiB maximum, puis le
restaurer sur un autre worker appairé. L’état est limité à 64 MiB et les pages
doivent arriver dans l’ordre.
`model.cluster.weight_transfer` peut transférer explicitement une plage de
poids d’un shard vers le staging local d’un worker cible. Les pages sont
chiffrées, bornées à 512 KiB, validées dans l’ordre puis publiées
atomiquement avec un manifeste. Lorsque toutes les plages du manifeste sont
présentes, le worker réassemble automatiquement un fichier GGUF local et
l’adaptateur Akasha l’utilise pour exécuter uniquement les couches assignées.
Une couverture incomplète reste en attente et ne peut pas être exécutée.
`model.cluster.stage_local_model` automatise ce transfert depuis le GGUF local
du coordinateur, par blocs bornés, avant un pipeline réparti. Pour un segment,
seules les plages de l’en-tête/index et des tenseurs `blk.N` nécessaires sont
transférées ; une liste vide conserve le mode fichier complet. L’opération est
toujours explicite, chiffrée et soumise à `allow_sensitive_data`.
Le protocole contient aussi des pages d’activation typées et bornées. Le
service explicite `model.cluster.layer_infer` peut envoyer une activation F32
encodée par Akasha à un shard assigné ; le worker la réassemble, charge les
poids GGUF du bloc demandé avec l’adaptateur CPU indépendant, puis renvoie le
résultat par pages sur le même canal authentifié. L’opération exige toujours
`allow_sensitive_data=true`, un secret de session valide et un nœud appairé.
Une activation non supportée (modèle non-GGUF, métadonnées absentes ou forme
incompatible) produit une erreur contrôlée et n’empêche pas le fallback local.
Le daemon sonde `llama_supports_rpc()` pour informer le diagnostic, mais le
chemin Akasha indépendant ne dépend pas de ce symbole. Le statut UI distingue
donc le RPC natif de l’adaptateur CPU Akasha ; le second reste expérimental et
requiert des poids GGUF lisibles.

Pour tester le contrat RPC NPU/WebGPU sans matériel spécialisé, lancer le
runtime de référence local (il ne fait qu’écho au tenseur) :

```powershell
cargo run -p aos-placement --example adapter_echo_runtime -- npu 127.0.0.1:38471
```

Le backend expérimental doit ensuite pointer vers cette adresse et le service
diagnostic `model.adapter.status` vérifie ensuite la poignée de main, puis le
service explicite `model.adapter.execute` peut être utilisé. Ce fake valide
la poignée de main, les capacités, la phase prefill/décodage et les limites de
trame ; il ne fournit ni accélération ni génération de texte.

Une retransmission du même shard est acceptée si les octets sont identiques ;
un contenu différent est refusé sans écraser le fichier existant. Les publications
des connexions d’un même daemon sont sérialisées pour préserver le manifeste de
couverture. Cette garantie suppose un seul daemon par répertoire de staging.
Le routage implicite de `model.infer` reste local. `model.cluster.layer_infer`
est volontairement explicite. `model.cluster.layer_pipeline_infer` peut
désormais recevoir `input_tokens` à la place d’une activation : le coordinateur
construit localement les embeddings GGUF, exécute les couches distantes, puis
peut retourner les logits du dernier token avec `return_logits=true`. Avec
`max_tokens>0`, la même requête effectue une génération autoregressive ;
`params.temperature`, `params.top_p` et `params.seed` contrôlent le sampling.
Cela ne modifie pas le routage par défaut ; le transfert KV explicite est
disponible via `model.cluster.kv_transfer`. Par défaut, aucune donnée de modèle
ou de prompt ne quitte la machine.

`model.cluster.layer_pipeline_infer` enchaîne plusieurs couches dans un ordre
strict, éventuellement sur plusieurs workers. Il vérifie que chaque nœud et
shard appartiennent au travail planifié, conserve la séquence d’activation et
retourne le nombre de couches exécutées ainsi que les transferts inter-nœuds.

La découverte automatique prend effet au démarrage de `aos-modeld` lorsque le
cluster LAN et la découverte sont activés. Elle utilise des annonces UDP CBOR
bornées sur le port configuré et n’envoie ni prompt, ni poids, ni token, ni
valeur de clé de session.

Dans **Paramètres → Modèles**, le bouton **Lancer un test LAN (1 token)**
sélectionne un modèle GGUF local, planifie une partition contiguë entre les
nœuds appairés, transfère le fichier vers chaque worker puis exécute le pipeline
complet. Le test est limité à un token
synthétique et n’envoie pas le contenu d’une conversation.

Pour vérifier le transport sans deuxième machine ni modèle, lancez aussi le
smoke test loopback :

```powershell
cargo run -p aos-placement --example lan_loopback_smoke --offline
```

Il vérifie deux identités appairées, le handshake, le chiffrement, un heartbeat
et son accusé de réception. Il ne teste pas la performance d’un vrai réseau.

## Inventaire YAML facultatif

Les mêmes nœuds peuvent être préchargés dans la configuration du daemon :

```yaml
lan_cluster:
  enabled: false
  local_node_id: local
  listen_address: 127.0.0.1:9001
  session_key_secret: lan_cluster_session_key
  nodes:
    - node_id: worker-1
      display_name: Worker bureau
      address: 192.168.1.20:9001
      public_key_fingerprint: sha256:remplacer-par-empreinte-verifiee
      trust: unpaired
      capabilities: []
```

Laissez `enabled: false` jusqu’à validation de l’appairage et du worker sur le
LAN visé.
