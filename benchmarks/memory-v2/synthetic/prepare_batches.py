#!/usr/bin/env python3
"""Prepare JSONL batch requests for the Preview benchmark client.

Run once without ``--runtime-results`` for ingestion batches, then again with
the V2 create result file to materialize runtime IDs for relations and API
assertions.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.write_text("".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows), encoding="utf-8")


def runtime_map(path: Path | None) -> dict[str, int]:
    if path is None:
        return {}
    return {
        row["fixture_id"]: row["response"]["id"]
        for row in load_json(path)
        if row.get("ok") and isinstance(row.get("fixture_id"), str) and isinstance(row.get("response", {}).get("id"), int)
    }


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, default=Path(__file__).parent)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runtime-results", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    objects = read_jsonl(args.dataset / "create_requests.jsonl")
    relations = read_jsonl(args.dataset / "relations.jsonl")
    queries = read_jsonl(args.dataset / "queries.jsonl")
    llm_cases = read_jsonl(args.dataset / "llm_cases.jsonl") if (args.dataset / "llm_cases.jsonl").exists() else []
    ids = runtime_map(args.runtime_results)

    write_jsonl(args.output / "v2_create.jsonl", [
        {"intent": "mem.object.create", "fixture_id": obj["metadata"]["fixture_id"], "request": obj}
        for obj in objects
    ])
    write_jsonl(args.output / "v1_write.jsonl", [
        {
            "intent": "mem.episodic_write",
            "fixture_id": obj["metadata"]["fixture_id"],
            "request": {
                "namespace": obj["namespace"],
                "text": obj["content"],
                "metadata": {"dataset": obj["metadata"]["dataset"], "fixture_id": obj["metadata"]["fixture_id"]},
                "pinned": False,
                "kind": "episode" if obj["kind"] in {"event", "decision"} else "fact",
                "auto_link": False,
            },
        }
        for obj in objects
    ])

    if ids:
        write_jsonl(args.output / "v2_relations.jsonl", [
            {
                "intent": "mem.object.relate",
                "request": {
                    "from": ids[rel["from_fixture_id"]],
                    "kind": rel["kind"],
                    "to": ids[rel["to_fixture_id"]],
                    "confidence": rel["confidence"],
                    "source_refs": rel["source_refs"],
                },
            }
            for rel in relations
            if rel["from_fixture_id"] in ids and rel["to_fixture_id"] in ids
        ])

        api_rows: list[dict[str, Any]] = []
        for query in queries:
            spec = query["assertion"]
            endpoint = query["endpoint"]
            if endpoint == "mem.context":
                request = {"namespace": query["namespace"], "query": query["query"], "k": 5, "product_k": 0, "user_doc_k": 0}
            elif endpoint == "mem.graph.query":
                request = {"root_id": ids[spec["root_fixture_id"]], "depth": 2, "max_nodes": 64}
            elif endpoint == "mem.timeline":
                request = {"namespace": query["namespace"], "limit": 512}
            elif endpoint == "mem.object.list":
                request = {"namespace": query["namespace"], "limit": 512, "include_archived": True}
                if "kind" in spec:
                    request["kind"] = spec["kind"]
                if "status" in spec:
                    request["status"] = spec["status"]
            elif endpoint == "mem.explain":
                request = {"id": ids[query["expected_fixture_ids"][0]]}
            elif endpoint == "mem.narrative.generate":
                request = {"namespace": query["namespace"], "persist": False, "title": f"Synthèse {query['namespace'].rsplit(':', 1)[-1]}"}
            else:
                raise ValueError(f"endpoint inconnu: {endpoint}")
            api_rows.append({"intent": endpoint, "fixture_id": query["query_id"], "request": request})
        write_jsonl(args.output / "query_requests.jsonl", api_rows)
        query_by_id = {query["query_id"]: query for query in queries}
        write_jsonl(args.output / "llm_context_requests.jsonl", [
            {
                "intent": "mem.context",
                "fixture_id": case["case_id"],
                "request": {
                    "namespace": query_by_id[case["query_id"]]["namespace"],
                    "query": query_by_id[case["query_id"]]["query"],
                    "k": 5,
                    "product_k": 0,
                    "user_doc_k": 0,
                },
            }
            for case in llm_cases
            if case["query_id"] in query_by_id
        ])
    else:
        write_jsonl(args.output / "relations_fixture.jsonl", relations)
    print(json.dumps({"objects": len(objects), "relations": len(relations), "queries": len(queries), "runtime_ids": len(ids)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
