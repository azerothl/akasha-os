# Configuration du cluster LAN

Le cluster LAN est expérimental et désactivé par défaut. Il utilise
uniquement les nœuds ajoutés puis explicitement appairés ; il ne découvre ni
ne contacte de pairs sur Internet.

## Configuration dans l’interface

1. Ouvrez **Paramètres → Modèles** et activez **Cluster LAN**.
2. Renseignez l’identité du nœud local, son adresse d’écoute annoncée et le
   nom du secret de clé de session. Le nom par défaut est
   `lan_cluster_session_key`.
3. Dans **Paramètres → Coffre à secrets**, saisissez une clé aléatoire de
   32 octets sous forme de 64 caractères hexadécimaux, puis enregistrez-la
   sous ce nom. Sa valeur est en écriture seule dans l’interface.
4. Dans la section Cluster LAN, saisissez l’identifiant du nœud distant, son
   nom, son adresse LAN (`192.168.x.y:port`, ainsi que loopback et link-local)
   et son empreinte de clé publique.
5. Cliquez sur **Ajouter le nœud**, puis sur **Appairer** après vérification
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

## Limite d’exécution actuelle

`model.cluster.plan` calcule le placement des shards. Le service interne
explicite `model.cluster.dispatch` peut envoyer des assignments typées et
chiffrées à un worker appairé lorsque son listener et la clé de session sont
disponibles. `model.cluster.recover` peut renvoyer les assignments après une
perte de nœud, et `model.cluster.cancel` propage l’annulation si le nom du
secret de session est fourni.

`aos-modeld` n’ouvre pas automatiquement de listener LAN. Le chargement réel
des poids, l’exécution des shards, le routage des tokens et le transfert du
cache KV nécessitent encore l’intégration du worker. Tant qu’elle n’est pas
activée, l’inférence CPU/GPU locale reste le comportement par défaut et
aucune donnée ne quitte la machine.

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
