# ASR hôte → texte pour le gate (#433)

**Langue :** [English](../speech-to-gate.md) | Français

La parole devient du texte **sur l'hôte** avant `evaluate_gate`. Pas
d'encodeur audio dans akasha-model, pas de poids STT dans ce dépôt.

- Coller un transcript OS : `speech.ingest_transcript` (cap `speech.ingest:*`)
- Commande opt-in : `AOS_ASR_COMMAND` + `speech.transcribe` (sinon
  `asr_not_configured`)
- Transcript = texte **non fiable** (ou **mixed** si l'opérateur a aussi tapé)
- Pas de STT always-on

Module : [`community/modules/voice-to-gate/`](../../community/modules/voice-to-gate/).
