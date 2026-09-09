# AK-021 — Pack vidéo proposé dans le mode Image

- Priorité : P1
- Statut : corrigé, déployé et vérifié

L’entrée LTX du catalogue déclarait `modality: video` tout en conservant un profil
`image`. Le studio ajoutait alors LTX au sélecteur Image et pouvait envoyer une
requête incohérente. Le filtrage UI donne désormais priorité à `modality` ; le mode
Image démarre sur Stable Diffusion 1.5 et le mode Vidéo sur LTX 2.3 Dev.
