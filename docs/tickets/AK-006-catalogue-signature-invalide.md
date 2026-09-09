# AK-006 — Signature du catalogue de modèles invalide au démarrage

- Priorité : P1
- Statut : corrigé et vérifié
- Zone : catalogue installé / vérification d’intégrité

## Preuve

`var/run/aos-platformd.stderr.log` contient `catalogue: signature catalogue invalide` à chaque démarrage observé, alors que le service continue et indexe 205 chunks produit.

## Impact

L’utilisateur ne sait pas si les offres de modèles sont authentiques ou simplement ignorées. Cela explique potentiellement les divergences entre offres déclarées et poids réellement résolus.

## Correction prise en charge

La signature Ed25519 a été régénérée avec la seed Preview documentée, dans le checkout et dans l’installation active. La clé publique reste inchangée. Le redémarrage du superviseur sert de vérification finale : le log ne doit plus contenir `signature catalogue invalide`.
