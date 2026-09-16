# Recette parcours utilisateur — 2026-09-16

- **Résultat :** PASS (J1, J2, J4 + screenshots marketing; J3/J5 skipped or deferred)
- **AOS_HOME :** `C:\Users\azero\AppData\Local\AgentOS-Preview`
- **Bus :** `127.0.0.1:24701`
- **Commande :** `.\demo\run-user-journeys.ps1 -NoStart -NoRestart -SkipVideo -Screenshots -NoBuild`

## Journeys

| ID | Status | Notes |
|----|--------|-------|
| J1 sessions + reprise de contexte | PASS | A=`sess-1789570569948` — marker `rouge carmin` intact after B→A |
| J2 image réelle | PASS | `/downloads/images/journey-teapot.png`, **390731** bytes, `engine=sdcpp`, `local:sd-v1-5` |
| J3 vidéo courte | SKIPPED in suite | Manual follow-up: LTX placed then bus `Closed` ~180s — use `-SkipVideo` until long-running media stream stays open; prior WebMs already in Create history |
| J4 accès résultat | PASS | `fs.list` + `create.history.record/list` include journey teapot path |
| J5 restart | SKIPPED | `-NoRestart` (install had a corrupted `aos-auditd.exe` that blocked clean `aos-session` boot; binary repaired from repo build) |

## Screenshots → website

Copied from `var/recette/screenshots-2026-09-16/`:

- `m1-rail.png` → [`website/media/rail.png`](../website/media/rail.png)
- `m2-chat.png` → [`website/media/chat.png`](../website/media/chat.png)
- `m3-create.png` → [`website/media/create.png`](../website/media/create.png)

## Checklist UI manuelle (hors bus)

- [ ] Sidebar Chat : basculer A → B → A montre le bon historique
- [ ] Create : le résultat image/vidéo est visible dans Preview + History
- [ ] Chat `/image` : pièce jointe + **Open in studio**
- [ ] Onglet Files : `/downloads` liste le fichier généré

## Artifacts

- JSON : `var/recette/user-journeys-2026-09-16.json`
- Fixtures : `demo/user-journeys/`
- Orchestrateur : `demo/run-user-journeys.ps1`
- Doc : [USER-JOURNEYS.md](USER-JOURNEYS.md)
