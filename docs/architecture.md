# Architecture map — Preview host

**Language:** English | [Français](fr/architecture.md)

Interactive diagram and machine-readable graph of the **Akasha OS Preview**
host architecture (daemons, bus, modules, trust/model/agent planes).

This is the **Preview scaffold** (Windows / Linux / macOS host app), not the
seL4 bare-metal target. See [technical-specs.md](technical-specs.md) and
[ADR 0001](../adr/0001-microkernel.md).

## Files

| File | Use |
|------|-----|
| [architecture-map.html](architecture-map.html) | Standalone interactive map (open in a browser) |
| [architecture-graph.json](architecture-graph.json) | `{ nodes, edges, flows }` for agents and tooling |

Open the HTML locally (double-click or a static file server). No build step.

## Release policy (required)

**Update this diagram on every Preview release** (each `v*` tag / packaging
pass documented in [packaging/RELEASE.md](../packaging/RELEASE.md)):

1. Diff daemons, modules, intents, and major flows since the last tag.
2. Refresh `architecture-graph.json` (`nodes`, `edges`, `flows`).
3. Rebuild or sync `architecture-map.html` so it embeds the same graph
   (the HTML is self-contained; keep JSON and HTML in lockstep).
4. Bump `meta.version` in the JSON to the release version.
5. Note the refresh in the release checklist / notes.

Do not ship a release whose architecture map still describes the previous
version’s process layout when daemons, modules, or primary flows changed.
