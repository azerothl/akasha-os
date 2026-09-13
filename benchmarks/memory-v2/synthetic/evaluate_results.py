#!/usr/bin/env python3
"""Evaluate Memory V2 JSONL-batch results against the synthetic manifest.

The runner intentionally keeps transport out of this module. A batch client
produces JSON arrays and this evaluator scores each query using its declared
endpoint/assertion, so semantic retrieval is not used as a proxy for graph,
timeline or evidence checks.
"""

from __future__ import annotations

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def response_ids(response: dict[str, Any]) -> list[int]:
    if isinstance(response.get("object_ids"), list):
        return [int(value) for value in response["object_ids"] if isinstance(value, int)]
    if isinstance(response.get("objects"), list):
        return [int(item["id"]) for item in response["objects"] if isinstance(item, dict) and isinstance(item.get("id"), int)]
    if isinstance(response, list):
        return [int(item["id"]) for item in response if isinstance(item, dict) and isinstance(item.get("id"), int)]
    return []


def relation_pairs(response: dict[str, Any]) -> list[tuple[int, str, int]]:
    raw = response.get("relation_pairs", response.get("relations", []))
    if not isinstance(raw, list):
        return []
    return [
        (int(item["from"]), str(item["kind"]), int(item["to"]))
        for item in raw
        if isinstance(item, dict)
        and isinstance(item.get("from"), int)
        and isinstance(item.get("to"), int)
        and item.get("kind")
    ]


def fixture_maps(rows: list[dict[str, Any]]) -> tuple[dict[str, int], dict[int, str]]:
    fixture_to_id: dict[str, int] = {}
    id_to_fixture: dict[int, str] = {}
    for row in rows:
        if row.get("ok") and isinstance(row.get("fixture_id"), str):
            response = row.get("response", {})
            if isinstance(response, dict) and isinstance(response.get("id"), int):
                fixture_to_id[row["fixture_id"]] = response["id"]
                id_to_fixture[response["id"]] = row["fixture_id"]
    return fixture_to_id, id_to_fixture


def evaluate_llm(cases: list[dict[str, Any]], rows: list[dict[str, Any]]) -> dict[str, Any]:
    by_id = {row.get("fixture_id"): row for row in rows}
    results = []
    for case in cases:
        row = by_id.get(case.get("case_id"), {})
        text = row.get("response", {}).get("text", "") if isinstance(row, dict) else ""
        folded = text.casefold()
        missing = [term for term in case.get("required_terms", []) if term.casefold() not in folded]
        citation_ok = not case.get("require_memory_citation") or "[memory:" in folded
        passed = bool(row.get("ok")) and not missing and citation_ok
        results.append({
            "case_id": case.get("case_id"),
            "passed": passed,
            "missing_terms": missing,
            "citation_ok": citation_ok,
            "elapsed_ms": row.get("elapsed_ms"),
            "text": text,
        })
    latencies = [item["elapsed_ms"] for item in results if isinstance(item.get("elapsed_ms"), (int, float))]
    return {
        "total": len(results),
        "passed": sum(item["passed"] for item in results),
        "recall": sum(item["passed"] for item in results) / len(results) if results else 0.0,
        "mean_elapsed_ms": statistics.mean(latencies) if latencies else None,
        "failed": [item for item in results if not item["passed"]],
    }


def score_query(query: dict[str, Any], row: dict[str, Any], v2: dict[str, int], v1: dict[str, int]) -> tuple[bool, bool | None, str]:
    if not row.get("ok"):
        return False, None, f"transport: {row.get('error', 'unknown error')}"
    response = row.get("response", {})
    if not isinstance(response, dict):
        return False, None, "response non structurée"
    assertion = query.get("assertion", {}).get("assertion", "top5")
    expected = query.get("expected_fixture_ids", [])
    expected_v2 = [v2[item] for item in expected if item in v2]
    ids = response_ids(response)

    if assertion == "top5":
        shadow = response.get("shadow", {})
        if not isinstance(shadow, dict):
            return False, None, "shadow absent"
        v2_ids = [int(item) for item in shadow.get("v2_ids", []) if isinstance(item, int)]
        v1_ids = [int(item) for item in shadow.get("v1_ids", []) if isinstance(item, int)]
        expected_v1 = [v1[item] for item in expected if item in v1]
        return bool(set(expected_v2) & set(v2_ids[:5])), bool(set(expected_v1) & set(v1_ids[:5])), f"v2_top5={v2_ids[:5]} expected={expected_v2}; v1_top5={v1_ids[:5]} expected={expected_v1}"
    if assertion == "contains":
        return bool(set(expected_v2) & set(ids)), None, f"ids={ids[:10]} expected={expected_v2}"
    if assertion == "latest_observed":
        return bool(expected_v2) and bool(ids) and ids[-1] == expected_v2[0], None, f"last_id={ids[-1] if ids else None} expected={expected_v2}"
    if assertion == "relation_exists":
        spec = query["assertion"]
        root = v2.get(spec.get("root_fixture_id", ""))
        target = expected_v2[0] if expected_v2 else None
        wanted = (root, spec.get("relation_kind"), target)
        return wanted in relation_pairs(response), None, f"wanted={wanted} pairs={relation_pairs(response)}"
    if assertion == "relation_targets":
        spec = query["assertion"]
        root = v2.get(spec.get("root_fixture_id", ""))
        wanted_targets = {(v2[item], spec.get("relation_kind"), root) for item in expected if item in v2}
        actual = set(relation_pairs(response))
        return wanted_targets.issubset(actual), None, f"missing={sorted(wanted_targets - actual)}"
    if assertion == "namespace_only":
        namespace = query["namespace"]
        namespaces = response.get("namespaces", [])
        return bool(namespaces) and all(item == namespace for item in namespaces), None, f"namespaces={namespaces}"
    if assertion == "proof":
        return int(response.get("source_count", 0)) > 0, None, f"source_count={response.get('source_count')}"
    if assertion == "narrative_sources":
        return response.get("kind") == "narrative" and int(response.get("source_count", 0)) > 0, None, f"kind={response.get('kind')} sources={response.get('source_count')}"
    return False, None, f"assertion inconnue: {assertion}"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, default=Path(__file__).parent)
    parser.add_argument("--v2-results", type=Path, required=True)
    parser.add_argument("--query-results", type=Path, required=True)
    parser.add_argument("--v1-results", type=Path)
    parser.add_argument("--replay-results", type=Path)
    parser.add_argument("--api-results", type=Path)
    parser.add_argument("--llm-results", type=Path)
    parser.add_argument("--llm-cases", type=Path)
    parser.add_argument("--fail-on-gate", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    queries = [json.loads(line) for line in (args.dataset / "queries.jsonl").read_text(encoding="utf-8").splitlines()]
    v2_rows = load_json(args.v2_results)
    query_rows = load_json(args.query_results)
    v1_rows = load_json(args.v1_results) if args.v1_results else []
    replay_rows = load_json(args.replay_results) if args.replay_results else []
    api_rows = load_json(args.api_results) if args.api_results else []
    llm_rows = load_json(args.llm_results) if args.llm_results else []
    v2, _ = fixture_maps(v2_rows)
    v1, _ = fixture_maps(v1_rows)
    query_by_id = {row.get("fixture_id"): row for row in query_rows}
    query_results = []
    by_scenario: dict[str, list[bool]] = defaultdict(list)
    by_scenario_v1: dict[str, list[bool]] = defaultdict(list)
    latencies = [row["elapsed_ms"] for row in query_rows if row.get("ok") and isinstance(row.get("elapsed_ms"), (int, float))]
    for index, query in enumerate(queries):
        row = query_by_id.get(query["query_id"], query_rows[index] if index < len(query_rows) else {"ok": False, "error": "résultat manquant"})
        passed, v1_passed, detail = score_query(query, row, v2, v1)
        query_results.append({"query_id": query["query_id"], "scenario": query["scenario"], "endpoint": query["endpoint"], "passed": passed, "v1_passed": v1_passed, "detail": detail})
        by_scenario[query["scenario"]].append(passed)
        if v1_passed is not None:
            by_scenario_v1[query["scenario"]].append(v1_passed)

    semantic_results = [
        item for item in query_results
        if item["endpoint"] == "mem.context" and item["detail"].startswith("v2_top5=")
    ]

    replay_same_ids = None
    if replay_rows:
        original_ids = {row.get("fixture_id"): row.get("response", {}).get("id") for row in v2_rows if row.get("ok")}
        replay_ids = {row.get("fixture_id"): row.get("response", {}).get("id") for row in replay_rows if row.get("ok")}
        replay_same_ids = sum(original_ids.get(key) == value for key, value in replay_ids.items()) == len(original_ids) == len(replay_ids)

    api_ok = sum(bool(row.get("ok")) for row in api_rows) if api_rows else None
    manifest = load_json(args.dataset / "manifest.json")
    llm_report = evaluate_llm(load_json(args.llm_cases), llm_rows) if args.llm_results and args.llm_cases else None
    gate = {
        "object_count_exact": len(v2) == manifest.get("object_count") and len(v2_rows) == manifest.get("object_count"),
        "semantic_top5_min": (
            sum(item["passed"] for item in semantic_results) / len(semantic_results)
            if semantic_results else 0.0
        ) >= manifest.get("validation_targets", {}).get("top5_recall_min", 0.0),
        "replay_same_runtime_ids": replay_same_ids if replay_rows else None,
        "api_all_success": api_ok == len(api_rows) if api_rows else None,
        "llm_min": (
            llm_report["recall"] >= manifest.get("validation_targets", {}).get("llm_recall_min", 0.0)
            if llm_report else None
        ),
    }
    gate_passed = all(value is not False for value in gate.values())
    report = {
        "dataset": manifest,
        "ingestion": {"objects": len(v2), "expected_objects": manifest.get("object_count"), "successful_results": sum(bool(row.get("ok")) for row in v2_rows), "errors": sum(not row.get("ok") for row in v2_rows), "unique_fixture_ids": len(v2)},
        "queries": {
            "total": len(query_results),
            "passed": sum(item["passed"] for item in query_results),
            "recall": sum(item["passed"] for item in query_results) / len(query_results) if query_results else 0.0,
            "v1_top5": {"passed": sum(item["v1_passed"] for item in query_results if item["v1_passed"] is not None), "total": sum(item["v1_passed"] is not None for item in query_results)},
            "mean_elapsed_ms": statistics.mean(latencies) if latencies else None,
            "by_scenario": {key: {"passed": sum(values), "total": len(values), "rate": sum(values) / len(values)} for key, values in sorted(by_scenario.items())},
            "v1_by_scenario": {key: {"passed": sum(values), "total": len(values), "rate": sum(values) / len(values)} for key, values in sorted(by_scenario_v1.items())},
            "semantic_top5": {"passed": sum(item["passed"] for item in semantic_results), "total": len(semantic_results), "rate": sum(item["passed"] for item in semantic_results) / len(semantic_results) if semantic_results else 0.0},
        },
        "replay": {"same_runtime_ids": replay_same_ids},
        "api_checks": {"successful": api_ok, "total": len(api_rows) if api_rows else None},
        "llm_checks": llm_report,
        "gate": {"passed": gate_passed, "checks": gate},
        "failed_queries": [item for item in query_results if not item["passed"]],
    }
    serialized = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.write_text(serialized, encoding="utf-8")
    print(serialized, end="")
    if args.fail_on_gate and not gate_passed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
