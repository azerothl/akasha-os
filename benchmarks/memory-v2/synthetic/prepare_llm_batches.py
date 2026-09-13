#!/usr/bin/env python3
"""Build grounded ``model.infer`` requests from real Memory V2 context calls.

The context batch must be collected without response summarisation. This keeps
the LLM gate tied to the actual retrieval result instead of copying the answer
directly from the fixture.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.write_text(
        "".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows),
        encoding="utf-8",
    )


def context_text(response: dict[str, Any]) -> str:
    passages: list[str] = []
    for obj in response.get("objects", []):
        if isinstance(obj, dict) and isinstance(obj.get("id"), int):
            passages.append(f"[memory:{obj['id']}] {obj.get('title', '')}: {obj.get('content', '')}")
    if not passages:
        for hit in response.get("hits", []):
            if isinstance(hit, dict):
                passages.append(f"[memory:legacy] {hit.get('text', '')}")
    return "\n".join(passages)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, default=Path(__file__).parent)
    parser.add_argument("--context-results", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--model-id", default=None)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    cases = read_jsonl(args.dataset / "llm_cases.jsonl")
    queries = {row["query_id"]: row for row in read_jsonl(args.dataset / "queries.jsonl")}
    context_rows = {row.get("fixture_id"): row for row in load_json(args.context_results)}
    requests: list[dict[str, Any]] = []
    runtime_cases: list[dict[str, Any]] = []
    for case in cases:
        query = queries[case["query_id"]]
        row = context_rows.get(case["case_id"], {})
        response = row.get("response", {}) if isinstance(row, dict) else {}
        evidence = context_text(response) if isinstance(response, dict) else ""
        prompt = (
            "Réponds en français à la question en t'appuyant exclusivement sur les "
            "éléments de mémoire ci-dessous. Si l'information manque, dis-le. "
            "Cite au moins une preuve avec son identifiant sous la forme [memory:ID].\n\n"
            f"Question : {query['query']}\n\nMémoire :\n{evidence}"
        )
        requests.append({
            "intent": "model.infer",
            "fixture_id": case["case_id"],
            "request": {
                "model_id": args.model_id,
                "messages": [
                    {"role": "system", "content": "Tu es un assistant factuel et traçable."},
                    {"role": "user", "content": prompt},
                ],
                "params": {"max_tokens": 160, "temperature": 0.2, "top_p": 0.9, "seed": 7},
                "priority": 0,
                "data_refs": [],
                "images": [],
                "routing": "local_only",
            },
        })
        runtime_cases.append({
            **case,
            "context_ok": bool(row.get("ok")),
            "context_object_ids": response.get("object_ids", []) if isinstance(response, dict) else [],
        })
    write_jsonl(args.output / "llm_requests.jsonl", requests)
    write_jsonl(args.output / "llm_cases.jsonl", runtime_cases)
    print(json.dumps({"cases": len(requests), "output": str(args.output)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
