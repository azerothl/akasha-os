# aos-gate — Akasha OS tool-gate host

Binds [akasha-model](https://github.com/azerothl/akasha-model) Path A
(`run_gated_call` / `dispatch_plan`) to Akasha OS permissions and executors.
The model package never runs tools; this host does.

See OS doc: [`docs/tool-gate-host.md`](../../docs/tool-gate-host.md) and
upstream [using-the-tool-gate.md](https://github.com/azerothl/akasha-model/blob/cursor/gate-host-476f/docs/using-the-tool-gate.md) (Path C).

```sh
python -m venv .venv-aos-gate && source .venv-aos-gate/bin/activate
pip install -e 'python/aos_gate[dev]'
pytest python/aos_gate/tests -q
```
