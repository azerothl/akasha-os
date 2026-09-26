# ADR 0012 : propriétaire du cycle de vie Preview (`aos-serverd` vs `aos-session`)

**Langue :** [English](../../adr/0012-aos-serverd.md) | Français

> Date : 26/09/2026 · Statut : **proposé** (P21.0)  
> Suivi : [issue #403](https://github.com/azerothl/akasha-os/issues/403) · Preview **0.19.0 / P21**

## Contexte

Preview **0.18.0** livre `aos-session` comme superviseur de session
interactive : il démarre l’arbre (`aos-busd` → `aos-capkd` → `aos-auditd` →
`aos-modeld` → `aos-platformd` → `aos-agentd`), lance `aos-ui-egui`, et ne
surveille que **auditd / platformd / modeld**. Fermer l’UI **arrête** tout
l’arbre. `aos-mcpd` / `aos-bridged` restent optionnels. `schedule.*` (E2) et
la santé E23 existent dans une session vivante, mais ce n’est pas un daemon
serveur always-on.

Besoin produit **0.19.0** : faire tourner Akasha OS **sans egui**, mieux
récupérer des crashes, accepter un **intake de jobs agents** en headless, et
optionnellement s’installer en **service utilisateur**.

Le sibling [Akasha](https://github.com/azerothl/akasha) a déjà un daemon
assistant 24/7. L’anti-roadmap Preview refuse fusion, canaux messaging et
admin réseau sans caps.

## Décision

### 1. Owner unique — nouveau `aos-serverd` (option B)

Introduire le binaire **`aos-serverd`** comme **propriétaire du cycle de vie**
de l’arbre Preview. Garder **`aos-session`** pour le bootstrap interactif +
client UI.

| Rôle | Owner |
|------|--------|
| Start / stop / restart ordonné de busd…agentd | `aos-serverd` |
| Watchdogs étendus (dont **agentd**) | `aos-serverd` |
| Plan de contrôle local | `aos-serverd` |
| Onboarding desktop, UX modèles, egui | `aos-session` |
| Runtime agents / schedule / workers | `aos-agentd` (inchangé) |

**Compat 0.19 :** sans serverd déjà up, `aos-session` peut encore spawn
l’arbre (legacy 0.18) *ou* démarrer/attacher serverd. Un seul owner — jamais
de double supervision.

### 2. Plan de contrôle — local, caps, audit

Transport 0.19 : socket Unix `$AOS_HOME/var/run/aos-serverd.sock` (ou named
pipe Windows). **Pas** de bind `0.0.0.0` non authentifié. Pas de mint de caps
dans serverd. Commandes : `status` / `restart` / `stop` / intake agent.

### 3. Attach UI vs start

Fermer egui **ne doit pas** tuer l’arbre quand serverd en est owner. MVP
(P21.2) = arbre headless sans egui ; « attach UI only » = P21.6.

### 4. Intake vs `aos-agentd`

Façade vers les chemins agentd existants. Pas de nouveau runtime agent.

### 5. Services OS opt-in

systemd user / launchd / tâche Windows = P21.5, défaut = pas de service.

## Non-objectifs (0.19.0)

Fusion sibling, multi-tenant, control plane réseau, K8s, remplacer E23 /
Placement / ADR 0010, mcpd/bridged par défaut, `workspace.bind` (Track B).

## Conséquences

Crate `aos-serverd` ; factorisation spawn/stop/health (P21.1) ; MVP =
P21.2–P21.4 ; docs EN+FR à la release.

## Lots

P21.0 (cet ADR + scaffold) → P21.7 (docs / VERSION 0.19.0). Détail : issue
[#403](https://github.com/azerothl/akasha-os/issues/403).

## Liens

- [#403](https://github.com/azerothl/akasha-os/issues/403) · [#247](https://github.com/azerothl/akasha-os/issues/247)
- `crates/aos-session`, `crates/aos-agent`
- [FEATURES.md](../FEATURES.md)
