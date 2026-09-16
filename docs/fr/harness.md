# CLI de code externes (harness)

**Langue :** [English](../harness.md) | Français

Comment Preview lance localement les CLI **Codex**, **Claude Code** et
**Grok**. Source de vérité : `crates/aos-agent/src/harness.rs` et
`harness_backend.rs`.

Sens **Akasha → CLI** (tirer les outils dans les agents). Le sens inverse
(IDE → Akasha) est le serveur MCP optionnel — voir [mcp-server.md](mcp-server.md).

## Deux modes

| Mode | Où | Comportement |
|------|-----|--------------|
| Outil `harness.run` | Boucle tools d’un agent natif | Spawn one-shot ; texte de résultat renvoyé au modèle |
| **Runtime** harness externe | Agents → Avancé → Runtime = Codex / Claude / Grok | Le worker saute la boucle tools native ; tours CLI séquentiels sous `aos-agent-worker` |

Les deux modes partagent `run_turn`. Aucun n’utilise un shell.
`command` / `argv` / `args` libres sont refusés.

## Prérequis

1. Installer le CLI pour que le binaire soit dans le **PATH** du process
   (`codex`, `claude` ou `grok` ; sous Windows aussi `.exe` / `.cmd` au stem
   correspondant).
2. Activer l’outil / la capacité **`harness.run`** sur l’agent (case **CLI
   externes**, ou ajout auto si Runtime ≠ Native).
3. Répertoire de travail optionnel : chemin absolu, ou relatif à `AOS_HOME`
   (sinon cwd du process). Doit être un dossier existant.

## Arguments de l’outil (`harness.run`)

| Champ | Requis | Notes |
|-------|--------|-------|
| `harness` | oui | `codex` \| `claude` (alias `claude-code`) \| `grok` (alias `grok-bot`) |
| `prompt` | oui | Max 24 000 caractères |
| `cwd` | non | Résolu comme ci-dessus |
| `timeout_sec` | non | Défaut 180 ; borné 15–600 |

## Argv figés

Pas de ligne shell. Résolution du binaire sur le `PATH`, puis spawn avec un
vecteur d’arguments fixe.

### Premier tour / one-shot

| CLI | Argv |
|-----|------|
| Codex | `codex exec --skip-git-repo-check <prompt>` |
| Claude | `claude -p <prompt> --output-format text` |
| Grok | `grok -p <prompt>` |

### Suite / steer (backend Runtime)

| CLI | Argv |
|-----|------|
| Codex | `codex exec resume --last --skip-git-repo-check <prompt>` |
| Claude | `claude -c -p <prompt> --output-format text` |
| Grok | `grok -p <prompt>` (pas de resume stable — nouveau prompt) |

## Cycle de vie du backend Runtime

Quand `AgentSpec.execution_backend` vaut `ExternalHarness` :

1. **Premier tour** — énoncé du goal, argv de démarrage.
2. **Steer** — argv continue/resume avec la directive.
3. **Pause** — flag cancel tue l’enfant en cours ; le worker attend.
4. **Kill** — `aos-agentd` arrête le worker ; enfant en `kill_on_drop`.

Timeout de tour : 180 s par défaut (mêmes bornes que l’outil). `max_steps` /
`timeout_secs` du goal bornent toujours la boucle worker.

Si l’agent est lié à une session chat, le **premier** spawn CLI passe par
l’act-gate (Autoriser / Refuser), même en chat autonome — comme l’outil.

## Sécurité

- Capacité `harness.run` (ou `harness.run:*`) obligatoire.
- Pas de shell, pas d’argv arbitraire, pas de secrets sur ce chemin.
- Stdin fermé ; stdout/stderr capturés et tronqués dans le résultat outil
  (≈32 k caractères stdout).
- Windows : `CREATE_NO_WINDOW` sur l’enfant.

## Lié

- Catalogue produit : [FEATURES.md](FEATURES.md) §5 Agents
- MCP *vers* Akasha : `share/mcp/servers.yaml.example`
- MCP *depuis* les IDE : [mcp-server.md](mcp-server.md)
