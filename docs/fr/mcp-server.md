# Serveur MCP — exposer Akasha aux IDE externes

**Langue :** [English](../mcp-server.md) | Français

Façade [MCP](https://modelcontextprotocol.io/) stdio opt-in pour que **Claude
Code, Codex, Cursor, Copilot** (etc.) appellent Akasha pendant que Preview
tourne. Sens inverse du *client* MCP déjà présent (`var/mcp/servers.yaml`).

Binaire : `bin/aos-mcpd` (crate `aos-mcp`). Pas démarré par `aos-session`.

## Prérequis

1. Preview en cours (bus — défaut `127.0.0.1:24701`).
2. Pointer l’IDE vers `aos-mcpd` du zip Preview (ou d’un `cargo build -p aos-mcp`).

## Env

| Variable | Défaut | Rôle |
|----------|--------|------|
| `AOS_BUS_ADDR` | `127.0.0.1:24701` | Bus host:port |
| `AOS_MCP_FROM` | `service:mcp` | Identité `Intent.from` |

Logs sur **stderr** uniquement.

## Outils (MVP)

| Outil | Intent bus |
|-------|------------|
| `akasha_models` | `model.list` |
| `akasha_infer` | `model.infer` (flux bufférisé) |
| `akasha_mem_stats` | `mem.stats` |
| `akasha_mem_context` | `mem.context` |
| `akasha_mem_list` | `mem.list` |
| `akasha_mem_recall` | `mem.user.recall` |
| `akasha_mem_remember` | `mem.user.remember` |

Hors MVP : secrets, shell, FS, agents, modules notes/tasks.

## Exemple de config IDE

```json
{
  "mcpServers": {
    "akasha": {
      "command": "/chemin/vers/agentos-preview/bin/aos-mcpd",
      "env": {
        "AOS_BUS_ADDR": "127.0.0.1:24701"
      }
    }
  }
}
```

Voir aussi [`share/mcp/akasha-mcp.example.json`](../../share/mcp/akasha-mcp.example.json).

## Lié

- Bridge HTTP mem/secrets : [sibling-bridge.md](sibling-bridge.md) (`aos-bridged`)
- Akasha client MCP : `share/mcp/servers.yaml.example`
- CLI externes *dans* Akasha : Agents → `harness.run` / Runtime
