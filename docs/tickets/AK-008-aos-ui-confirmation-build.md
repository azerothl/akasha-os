# AK-008 — Le binaire CLI `aos-ui` ne compilait plus avec les confirmations persistantes

- Priorité : P0
- Statut : corrigé dans le checkout
- Zone : `crates/aos-ui/src/main.rs`

`ConfirmResponseRequest` possède le champ `persistent`, mais `/confirm` et `/deny` ne le renseignaient pas. La compilation workspace échouait.

Correction : le CLI envoie explicitement `persistent: false`; le choix « toujours » reste réservé à l’UI qui l’expose. Validation : `cargo check --workspace`.
