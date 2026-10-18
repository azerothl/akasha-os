# Dev Assistant (reference module)

**License:** MIT (community tree)  
**Version:** 0.3.0 — host API 3, DeclUI contract 2, ext-rt 1.1.0.

Optional script module for bound host projects. It is not installed by default.
Copy to `var/modules/src/dev-assistant/`, package with the rebuilt host/runtime,
and install with capability review. Existing installed copies require a package update.

1. Bind an absolute host folder. Add only that workspace's required capabilities
   to the local source manifest, repackage/update and approve the declared grants.
   The generic reference requests no host access; actor grants alone do not grant
   permissions to the installed module.
2. Search, propose replacements with undo, inspect Git status/diff.
3. Configure named host tasks, staged-only commits or DAP launches in the host
   session policies. Review scoped grants and confirm each launch/mutation.
4. Map the same directory to an existing Code Studio project for persistent
   project memory, indexing and read-only LSP navigation.
5. Existing agents can submit separate subtree proposals; review their exact
   fingerprint before confirmed application or recovery/undo.

Native forms expose commands, memory, debugging and refactor review. Code Studio
retains the full editor UX. MCP host clients require online mode; host tools
have account rights and are not network-sandboxed.

[Configuration, capabilities and limits](../../../docs/dev-assistant-studio.md) ·
[Observed evidence and remaining work](../../../docs/tickets/dev-assistant-follow-up-2026-10-18.md).
