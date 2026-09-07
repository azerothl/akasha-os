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
4. Dans **Paramètres → Coffre à secrets**, saisissez une clé aléatoire de
   32 octets sous forme de 64 caractères hexadécimaux, puis enregistrez-la
   sous ce nom. Sa valeur est en écriture seule dans l’interface.
5. Dans la section Cluster LAN, saisissez l’identifiant du nœud distant, son
   nom, son adresse LAN (`192.168.x.y:port`, ainsi que loopback et link-local)
   et son empreinte de clé publique.
6. Cliquez sur **Ajouter le nœud**, ou attendez un candidat découvert, puis sur **Appairer** après vérification
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

`model.cluster.plan` calcule le placement des shards. Le service interne
explicite `model.cluster.dispatch` peut envoyer des assignments typées et
chiffrées à un worker appairé lorsque son listener et la clé de session sont
disponibles. Le worker accuse réception des assignments, des heartbeats et
des annulations. `model.cluster.recover` peut renvoyer les assignments après
une perte de nœud, et `model.cluster.cancel` propage l’annulation si le nom
du secret de session est fourni.

`aos-modeld` ouvre le listener LAN uniquement lorsque le cluster est activé
et que la clé de session est disponible dans le coffre. Le worker charge le
modèle local avant l’ACK de l’assignment et sait exécuter le prefill/decode
token-level dans son contexte courant. Les messages `Prefill`, `Decode`,
tokens, inférence texte explicite et pages KV sont définis, chiffrés et bornés.
`model.cluster.infer_chat` permet à un appelant autorisé de transmettre un
prompt au nœud choisi et retourne le texte avec ses métriques ; il exige
`allow_sensitive_data=true`, un secret de session valide et un nœud appairé.
Une annulation authentifiée reçue sur une connexion séparée est propagée au
drapeau du contexte de génération et termine le travail à la frontière de
token.
Le routage implicite de `model.infer` reste local. La partition réelle des
poids/shards et le transfert KV inter-processus restent à intégrer. Par défaut,
aucune donnée de modèle ou de prompt ne quitte la machine.

La découverte automatique prend effet au démarrage de `aos-modeld` lorsque le
cluster LAN et la découverte sont activés. Elle utilise des annonces UDP CBOR
bornées sur le port configuré et n’envoie ni prompt, ni poids, ni token, ni
valeur de clé de session.

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
