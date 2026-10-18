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

## Outils Create et modules optionnels (#439)

Les outils OS supplémentaires sont masqués et refusés sans une liste explicite
et des capacités configurées dans l'environnement du serveur :

```json
"AOS_MCP_TOOLS": "akasha_image_generate;akasha_video_generate;notes.list",
"AOS_MCP_CAPS": "media.generate;fs.write:/downloads/**;tool.invoke:notes"
```

`akasha_image_generate`, `akasha_video_generate` et `akasha_image_upscale`
appellent les intents Create existants. La vidéo utilise `sd_mode=vid_gen`,
17 images à 8 FPS par défaut. Les chemins d'entrée demandent `fs.read` ; les
sorties restent dans `/downloads` et demandent `fs.write`. Les résultats sont des
chemins VFS et des métadonnées, sans binaire dans le flux MCP.

Les outils de module sont découverts depuis les manifestes installés et revus.
`notes.list` devient `akasha_module__notes__list` ; seuls `notes.list`,
`notes.read`, `notes.search`, `notes.links`, `notes.related` et `tasks.list` sont
admis. Un module absent n'apparaît pas. Chaque appel garde `module.invoke` et
ses contrôles de capacités.

L'acteur transmis est `mcp:<AOS_MCP_FROM>` : les permissions ne sont pas
déduites du statut de service. Le client ne peut fournir ni acteur ni caps.
Les refus sont journalisés sur stderr et les appels dans `mcp.tool.call`
quand le service d'audit est joignable. Les modèles nécessaires doivent déjà
être installés ; voir les limites détaillées dans la [version anglaise](../mcp-server.md).

Secrets, shell, FS arbitraire, agents, outils de module destructifs et annulation
globale de média restent hors périmètre. `notes.create` peut écraser une note et
nécessite donc un contrat de confirmation avant exposition. Le serveur ne
déclare pas de prise en charge de progression ou d'annulation MCP.

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
  ([harness.md](harness.md))
