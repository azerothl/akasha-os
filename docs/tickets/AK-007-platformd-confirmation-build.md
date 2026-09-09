# AK-007 — `aos-platformd` ne compilait plus après l’évolution des confirmations

- Priorité : P0
- Statut : corrigé dans le checkout
- Zone : `crates/aos-platform/src/bin/aos-platformd.rs`

`ConfirmManager::ask` renvoie `ConfirmationResult { approved, persistent }`, mais trois chemins de confirmation utilisaient encore `unwrap_or(false)`. La compilation de `aos-platformd` échouait donc avec six erreurs de type.

Correction : extraire explicitement `result.approved` avec un fallback `false` (fail-closed). Validation à faire dans la construction release et dans les tests de confirmation.
