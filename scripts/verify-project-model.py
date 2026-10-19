"""Real local model -> two scoped proposals -> confirmed native apply/test/undo.

Uses a newly generated fixture only. It is not an autonomous agentd benchmark,
a representative monorepo refactor, or concurrent GPU inference evidence.
"""

import argparse
import ast
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time

FRONT = """def render(records):
    result = []
    for record in records:
        if record.get("admin", False):
            result.append("ADMIN:" + str(record.get("name", "")).strip().lower())
        else:
            result.append("USER:" + str(record.get("name", "")).strip().lower())
    return result
"""
BACK = """def subtotal(items):
    total = 0.0
    for item in items:
        total += round(float(item["price"]) * max(0, int(item["quantity"])), 2)
    return round(total, 2)


def total_with_tax(items, rate):
    total = 0.0
    for item in items:
        total += round(float(item["price"]) * max(0, int(item["quantity"])), 2)
    return round(total * (1 + rate), 2)
"""

VALIDATOR = """import ast, json, pathlib, sys
ROOT = pathlib.Path(__file__).parent / "project"
ALLOWED_CALLS = {"str", "int", "float", "len", "sum", "round", "abs", "list", "tuple", "zip", "range", "min", "max", "sorted", "enumerate", "isinstance", "bool"}
ALLOWED_ATTRS = {"get", "strip", "lower", "append", "join", "items", "values", "keys"}
ALLOWED_NODES = {"Module", "FunctionDef", "arguments", "arg", "Return", "Assign", "AnnAssign", "AugAssign", "For", "If", "IfExp", "Expr", "Pass", "List", "Tuple", "Dict", "Set", "ListComp", "GeneratorExp", "comprehension", "Name", "Load", "Store", "Constant", "Call", "keyword", "Attribute", "Subscript", "Slice", "BinOp", "UnaryOp", "BoolOp", "Compare", "Add", "Sub", "Mult", "Div", "FloorDiv", "Mod", "USub", "UAdd", "Not", "And", "Or", "Eq", "NotEq", "Lt", "LtE", "Gt", "GtE", "Is", "IsNot", "In", "NotIn", "JoinedStr", "FormattedValue"}
def load(part):
    source = (ROOT / part / "module.py").read_text(encoding="utf-8")
    assert len(source.encode()) <= 65536
    tree = ast.parse(source)
    assert all(isinstance(node, ast.FunctionDef) or (isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant) and isinstance(node.value.value, str)) for node in tree.body), "only pure function definitions and a docstring may occur at module level"
    definitions = [node for node in tree.body if isinstance(node, ast.FunctionDef)]
    functions = {node.name for node in definitions}
    assert len(functions) == len(definitions), "duplicate function definitions are forbidden"
    for node in ast.walk(tree):
        assert type(node).__name__ in ALLOWED_NODES, type(node).__name__
        if isinstance(node, ast.Name): assert not node.id.startswith("__")
        if isinstance(node, ast.Attribute): assert node.attr in ALLOWED_ATTRS
        if isinstance(node, ast.Constant) and isinstance(node.value, (int,float)): assert abs(node.value) <= 10000
        if isinstance(node, ast.FunctionDef): assert not node.decorator_list and node.returns is None
        if isinstance(node, ast.Call):
            assert (isinstance(node.func, ast.Name) and node.func.id in ALLOWED_CALLS | functions) or (isinstance(node.func, ast.Attribute) and node.func.attr in ALLOWED_ATTRS)
    builtins = {name: getattr(__import__("builtins"), name) for name in ALLOWED_CALLS}
    env = {"__builtins__": builtins}
    exec(compile(tree, f"fixture/{part}/module.py", "exec"), env)
    return env, len(functions)
front, front_functions = load("frontend")
back, back_functions = load("backend")
cases = 0
for name in [" Alice ", "BOB", "", None, 42, "éLÈNE"]:
    for admin in [True, False]:
        expected = [("ADMIN:" if admin else "USER:") + str(name).strip().lower()]
        assert front["render"]([{"name":name,"admin":admin}]) == expected
        cases += 1
assert front["render"]([]) == []
cases += 1
for price in [0, 1.25, 3.333, "2.75"]:
    for quantity in [-2, 0, 1, 3, "2"]:
        items = [{"price":price,"quantity":quantity}, {"price":0.99,"quantity":2}]
        expected = round(sum(round(float(x["price"]) * max(0,int(x["quantity"])),2) for x in items),2)
        assert back["subtotal"](items) == expected
        for rate in [0, 0.05, 0.2]:
            assert back["total_with_tax"](items,rate) == round(sum(round(float(x["price"]) * max(0,int(x["quantity"])),2) for x in items) * (1+rate),2)
            cases += 1
        cases += 1
if "--require-refactor" in sys.argv:
    assert front_functions >= 2, "frontend helper extraction was not implemented"
    assert back_functions >= 3, "backend helper extraction was not implemented"
    for part, public in [("frontend",{"render"}),("backend",{"subtotal","total_with_tax"})]:
        tree=ast.parse((ROOT/part/"module.py").read_text(encoding="utf-8"))
        defs={node.name:node for node in tree.body if isinstance(node,ast.FunctionDef)}
        helpers=set(defs)-public
        calls={name:{node.func.id for node in ast.walk(fn) if isinstance(node,ast.Call) and isinstance(node.func,ast.Name)} for name,fn in defs.items()}
        def reachable(name,seen=None):
            seen=set() if seen is None else seen
            if name in seen:return set()
            seen.add(name)
            result=calls.get(name,set()) & helpers
            for target in calls.get(name,set()) & set(defs):result |= reachable(target,seen)
            return result
        shared=set.intersection(*(reachable(name) for name in public))
        assert shared, f"{part}: public functions do not use a shared extracted helper"
print(json.dumps({"ok":True,"cases":cases,"frontend_functions":front_functions,"backend_functions":back_functions}))
"""


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--model-probe", required=True, type=Path)
    parser.add_argument("--host-probe", required=True, type=Path)
    args = parser.parse_args()
    for path in [args.model, args.model_probe, args.host_probe]:
        if not path.is_absolute() or not path.is_file():
            parser.error("all paths must be existing absolute files")
    fixture = Path(tempfile.mkdtemp(prefix="aos-project-model-"))
    print(f"fixture: {fixture}", flush=True)
    (fixture / ".akasha-os-model-probe").write_text(
        "generated acceptance fixture only", encoding="utf-8"
    )
    (fixture / "verify.py").write_text(VALIDATOR, encoding="utf-8")
    for part, source in [("frontend", FRONT), ("backend", BACK)]:
        folder = fixture / "project" / part
        folder.mkdir(parents=True)
        (folder / "module.py").write_text(source, encoding="utf-8")
    prepared = subprocess.run(
        [str(args.host_probe), "--prepare", str(fixture), sys.executable],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=30,
    )
    (fixture / "context-host.log").write_text(
        prepared.stdout + prepared.stderr, encoding="utf-8"
    )
    assert prepared.returncode == 0, f"native context preparation failed: {fixture}"
    contexts = json.loads(prepared.stdout.splitlines()[-1])
    (fixture / "native-source-context.json").write_text(
        json.dumps(contexts), encoding="utf-8"
    )
    baseline = subprocess.run(
        [sys.executable, "-I", "-S", "-B", str(fixture / "verify.py")],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=5,
    )
    (fixture / "baseline.log").write_text(
        baseline.stdout + baseline.stderr, encoding="utf-8"
    )
    assert baseline.returncode == 0, baseline.stderr
    started = time.monotonic()
    generations = {}
    for part, source, task in [
        (
            "frontend",
            FRONT,
            "Extract the repeated name normalization into one pure helper, called in both branches. Keep render(records) and exact behavior.",
        ),
        (
            "backend",
            BACK,
            "Extract the duplicated per-item line-total calculation into one pure helper used by both public functions. Keep subtotal(items), total_with_tax(items,rate), rounding order and exact behavior.",
        ),
    ]:
        source = contexts[part]["content"]
        messages = [
            [
                "system",
                "You refactor one Python source file in an isolated test project. Output exactly one JSON object with the only key content, containing the complete replacement Python source. No markdown, no reasoning, no imports, no I/O, no type annotations or decorators, no module-level execution. Replace the file with final versions only: each function name must be defined exactly once; never append replacements after retaining originals. Use only pure functions, ordinary loops/comprehensions and str/int/float/round/sum/max/min builtins.",
            ],
            ["user", f"Task: {task}\nSource:\n{source}"],
        ]
        for attempt in range(2):
            request = fixture / f"{part}-request-{attempt}.json"
            request.write_text(
                json.dumps(
                    {
                        "messages": messages,
                        "source_file_sha256": contexts[part]["sha256"],
                    }
                ),
                encoding="utf-8",
            )
            result = fixture / f"{part}-generation-{attempt}.json"
            completed = subprocess.run(
                [str(args.model_probe), str(args.model), str(request), str(result)],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=180,
            )
            (fixture / f"{part}-model-{attempt}.log").write_text(
                completed.stdout + completed.stderr, encoding="utf-8"
            )
            assert completed.returncode == 0, f"model generation failed: {fixture}"
            generation = json.loads(result.read_text(encoding="utf-8"))
            raw = generation["raw_text"].strip()
            if "</think>" in raw:
                raw = raw.rsplit("</think>", 1)[1].strip()
            if raw.startswith("```json") and raw.endswith("```"):
                raw = raw[7:-3].strip()
            elif raw.startswith("```") and raw.endswith("```"):
                raw = raw[3:-3].strip()
            try:
                candidate = json.loads(raw)
                assert set(candidate) == {"content"} and isinstance(
                    candidate["content"], str
                )
                assert candidate["content"] != source, "model returned unchanged source"
                assert len(candidate["content"].encode()) <= 65536
                parsed = ast.parse(candidate["content"])
                functions = [
                    node.name
                    for node in parsed.body
                    if isinstance(node, ast.FunctionDef)
                ]
                assert len(functions) == len(
                    set(functions)
                ), "duplicate function definitions; remove originals and return final versions only"
                break
            except (ValueError, AssertionError, SyntaxError) as error:
                if attempt == 1:
                    raise RuntimeError(
                        f"two model candidates failed validation; retained at {fixture}"
                    ) from error
                messages.extend(
                    [
                        ["assistant", raw],
                        [
                            "user",
                            f"Validation of your candidate failed: {type(error).__name__}: {error}. Return a corrected complete JSON object with content only. Preserve the original behavior and pure helper extraction.",
                        ],
                    ]
                )
        generation["attempts"] = attempt + 1
        (fixture / f"{part}-proposal.json").write_text(
            json.dumps({**candidate, "base_sha256": contexts[part]["sha256"]}),
            encoding="utf-8",
        )
        generations[part] = {
            key: value for key, value in generation.items() if key != "raw_text"
        }
        print(f"{part}: real model proposal generated", flush=True)
    host = subprocess.run(
        [str(args.host_probe), str(fixture), sys.executable],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=90,
    )
    (fixture / "host.log").write_text(host.stdout + host.stderr, encoding="utf-8")
    assert host.returncode == 0, f"native apply/test/undo failed: {fixture}"
    native = json.loads(host.stdout.splitlines()[-1])
    report = {
        "ok": True,
        "fixture": str(fixture),
        "model": args.model.name,
        "model_sha256": digest(args.model),
        "model_probe_sha256": digest(args.model_probe),
        "host_probe_sha256": digest(args.host_probe),
        "python": {"executable": sys.executable, "version": sys.version},
        "source_context_from_native_host": True,
        "source_context": {
            part: {key: value for key, value in context.items() if key != "content"}
            for part, context in contexts.items()
        },
        "generation": generations,
        "native": native,
        "elapsed_ms": round((time.monotonic() - started) * 1000),
        "scope": "two real-model-authored proposals through scoped actor identities and confirmed host apply/test/undo; not autonomous agentd, representative monorepo quality, or overlapping GPU inference",
    }
    (fixture / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
