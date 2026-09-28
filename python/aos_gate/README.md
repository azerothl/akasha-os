# aos-gate — Akasha OS tool-gate host

Binds [akasha-model](https://github.com/azerothl/akasha-model) Path A
(`run_gated_call` / `dispatch_plan`) and Path D (`outcomes`) to Akasha OS
permissions and executors. The model package never runs tools.

See OS doc: [`docs/tool-gate-host.md`](../../docs/tool-gate-host.md) and
upstream [using-the-tool-gate.md](https://github.com/azerothl/akasha-model/blob/main/docs/using-the-tool-gate.md).

Covers:

- Path D production outcomes JSONL (`outcomes_log`)
- Source-trust / authority confusion (`trust`)
- HITL escalate approve/deny (`hitl`)
- Out-of-band credential injection (`credentials`)
- MCP `tools/list` → `ToolSpec` (`mcp_catalog`)

```sh
python -m venv .venv-aos-gate && source .venv-aos-gate/bin/activate
pip install -e 'python/aos_gate[dev]'
pytest python/aos_gate/tests -q
```
