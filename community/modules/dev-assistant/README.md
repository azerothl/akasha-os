# Dev Assistant (reference module)

**License:** MIT (community tree)  
**Contract:** [docs/dev-assistant-p0.md](../../../docs/dev-assistant-p0.md) (DA.4 / #247)

Script + DeclUI v1 reference for large **host** projects. Not shipped in the
signed Preview catalogue by default — copy into `var/modules/src/dev-assistant/`
(or package from this tree) and install with cap review.

## Flow

1. Bind an absolute host folder → note `workspace_id` and returned caps.
2. Accept `fs.read/write:/host/<id>/**` via cap review (fail-closed).
3. Search with `dev-assistant.search`; patch with `dev-assistant.apply_patch`.
4. Undo via `undo_group_id`.

No `code_editor` / git / `process.run` (P1–P2).
