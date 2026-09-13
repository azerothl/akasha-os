#!/usr/bin/env python3
"""Generate a deterministic, privacy-safe Memory V2 validation corpus.

The generated JSONL files are request-shaped fixtures. They deliberately use
fixture identifiers in metadata so a runner can map them to runtime object
IDs returned by ``mem.object.create`` before inserting relations.
"""

from __future__ import annotations

import argparse
import json
import random
from pathlib import Path


SEED = 20260913
BASE_MS = 1_704_067_200_000  # 2024-01-01T00:00:00Z
NAMESPACE_PREFIX = "synthetic:validation"

PROJECTS = [
    ("atlas", "Atlas", "observabilité industrielle", "Personne A"),
    ("boreal", "Boréal", "agriculture durable", "Personne B"),
    ("cobalt", "Cobalt", "sécurité des données", "Personne C"),
    ("delta", "Delta", "mobilité urbaine", "Personne D"),
    ("echo", "Écho", "éducation hybride", "Personne E"),
    ("fenix", "Fénix", "reconditionnement matériel", "Personne F"),
    ("gaia", "Gaïa", "mesure énergétique", "Personne G"),
    ("helios", "Hélios", "photographie scientifique", "Personne H"),
    ("iris", "Iris", "accessibilité numérique", "Personne I"),
    ("junon", "Junon", "logistique bas carbone", "Personne J"),
    ("kappa", "Kappa", "outillage développeur", "Personne K"),
    ("luna", "Luna", "santé préventive", "Personne L"),
]

PEOPLE = ["Personne A", "Personne B", "Personne C", "Personne D"]
DECISION_OPTIONS = [
    "déployer un pilote local",
    "acheter une solution existante",
    "attendre le prochain trimestre",
]


def source(project: str, kind: str, number: int) -> dict:
    return {
        "source_type": "synthetic_note",
        "source_id": f"synthetic:{project}:{kind}:{number:03d}",
        "excerpt": f"Extrait synthétique {project}/{kind}/{number:03d}",
    }


def temporal(day: int, *, duration: int | None = None) -> dict:
    start = BASE_MS + day * 86_400_000
    value = {"observed_at": start, "valid_from": start, "last_confirmed_at": start}
    if duration is not None:
        value["valid_to"] = start + duration * 86_400_000
    return value


def request(
    fixture_id: str,
    namespace: str,
    kind: str,
    title: str,
    content: str,
    *,
    status: str = "accepted",
    confidence: float = 0.92,
    importance: float = 0.55,
    day: int = 0,
    duration: int | None = None,
    project: str,
    scenario: str,
    decision: dict | None = None,
) -> dict:
    return {
        "namespace": namespace,
        "kind": kind,
        "title": title,
        "content": content,
        "status": status,
        "confidence": confidence,
        "importance": importance,
        "temporal": temporal(day, duration=duration),
        "source_refs": [source(project, kind, int(fixture_id.rsplit("-", 1)[-1], 16) % 1000)],
        "visibility": "private",
        "metadata": {
            "dataset": "memory-v2-synthetic-2026-09",
            "fixture_id": fixture_id,
            "project": project,
            "scenario": scenario,
        },
        "decision": decision,
        "idempotency_key": f"memory-v2-synthetic:{fixture_id}",
    }


def decision_payload(project_name: str, index: int, selected: str | None) -> dict:
    return {
        "question": f"Quelle stratégie {index + 1} retenir pour le projet {project_name} ?",
        "options": DECISION_OPTIONS,
        "selected_option": selected,
        "rationale": (
            f"Le pilote local réduit le risque de migration pour {project_name}."
            if selected == DECISION_OPTIONS[0]
            else "La décision reste ouverte en attente de données comparables."
        ),
        "participants": [PEOPLE[index % len(PEOPLE)], PEOPLE[(index + 1) % len(PEOPLE)]],
        "consequences": [
            "mesurer la qualité sur un périmètre limité",
            "réexaminer le choix après quatre semaines",
        ],
        "review_at": BASE_MS + (index + 45) * 86_400_000,
    }


def build() -> tuple[list[dict], list[dict], list[dict], list[dict], dict]:
    rng = random.Random(SEED)
    objects: list[dict] = []
    relations: list[dict] = []
    queries: list[dict] = []
    llm_cases: list[dict] = []
    counts: dict[str, int] = {}

    def add(obj: dict) -> None:
        objects.append(obj)
        counts[obj["kind"]] = counts.get(obj["kind"], 0) + 1

    def relate(from_id: str, kind: str, to_id: str, project: str) -> None:
        relations.append(
            {
                "from_fixture_id": from_id,
                "kind": kind,
                "to_fixture_id": to_id,
                "confidence": 0.9,
                "source_refs": [source(project, "relation", len(relations))],
            }
        )

    for project_index, (slug, name, theme, owner) in enumerate(PROJECTS):
        namespace = f"{NAMESPACE_PREFIX}:{slug}"
        project_id = f"{slug}-entity-000"
        add(request(project_id, namespace, "entity", f"Projet {name}", f"{name} porte le programme de {theme}.", importance=0.9, project=slug, scenario="entity"))

        goal_ids = []
        for i in range(2):
            fixture_id = f"{slug}-goal-{i:03d}"
            goal_ids.append(fixture_id)
            add(request(fixture_id, namespace, "goal", f"Objectif {name} {i + 1}", f"Atteindre un indicateur mesurable de {theme} avant la fin du semestre {i + 1}.", importance=0.8, day=4 + i * 20, project=slug, scenario="goal"))
            relate(fixture_id, "targets", project_id, slug)

        problem_ids = []
        for i in range(2):
            fixture_id = f"{slug}-problem-{i:03d}"
            problem_ids.append(fixture_id)
            add(request(fixture_id, namespace, "problem", f"Problème {name} {i + 1}", f"Le projet {name} doit résoudre une limitation de capacité liée à {theme}, cas {i + 1}.", confidence=0.86, importance=0.7, day=8 + i * 18, project=slug, scenario="problem"))
            relate(fixture_id, "targets", project_id, slug)

        decision_ids = []
        for i in range(8):
            fixture_id = f"{slug}-decision-{i:03d}"
            decision_ids.append(fixture_id)
            status = ["accepted", "candidate", "rejected", "accepted", "superseded", "archived", "accepted", "candidate"][i]
            selected = DECISION_OPTIONS[0] if status in {"accepted", "superseded"} else None
            add(request(
                fixture_id,
                namespace,
                "decision",
                f"Décision {name} {i + 1}",
                f"Pour {name}, la question {i + 1} porte sur l'adoption progressive d'une solution de {theme}.",
                status=status,
                confidence=0.96 if status == "accepted" else 0.72,
                importance=0.95 if i == 0 else 0.65,
                day=12 + i * 17,
                project=slug,
                scenario="decision",
                decision=decision_payload(name, i, selected),
            ))
            relate(fixture_id, "involves", project_id, slug)
            relate(fixture_id, "depends_on", problem_ids[i % len(problem_ids)], slug)
            relate(fixture_id, "supports", goal_ids[i % len(goal_ids)], slug)
            if i > 0:
                relate(fixture_id, "derived_from", f"{slug}-event-{(i - 1):03d}", slug)
            if i == 4:
                relate(fixture_id, "supersedes", decision_ids[0], slug)

        for i in range(48):
            fixture_id = f"{slug}-event-{i:03d}"
            day = 20 + i * 5 + project_index * 2
            add(request(fixture_id, namespace, "event", f"Événement {name} {i + 1}", f"Le projet {name} a réalisé l'étape {i + 1} dans le domaine {theme}.", confidence=0.9, importance=0.4 + (i % 4) * 0.08, day=day, project=slug, scenario="event"))
            if i < 8:
                relate(decision_ids[i], "derived_from", fixture_id, slug)

        for i in range(16):
            fixture_id = f"{slug}-claim-{i:03d}"
            duration = 45 if i % 5 == 0 else None
            add(request(fixture_id, namespace, "claim", f"Affirmation {name} {i + 1}", f"La mesure {i + 1} du projet {name} indique une progression de {10 + i}% sur {theme}.", confidence=0.78 + (i % 4) * 0.05, importance=0.45, day=30 + i * 11, duration=duration, project=slug, scenario="claim"))

        for i in range(4):
            fixture_id = f"{slug}-preference-{i:03d}"
            status = "candidate" if i == 3 else "accepted"
            add(request(fixture_id, namespace, "preference", f"Préférence {name} {i + 1}", f"Pour {name}, l'équipe préfère un outil local et explicable pour {theme}, variante {i + 1}.", status=status, confidence=0.68 if status == "candidate" else 0.88, importance=0.5, day=42 + i * 23, project=slug, scenario="preference"))

        # One explicit contradiction pair and one cross-project distractor marker.
        old_claim = f"{slug}-claim-000"
        new_claim = f"{slug}-claim-005"
        relate(new_claim, "contradicts", old_claim, slug)

        query_specs = [
            ("decision_question", f"Quelle stratégie retenir pour le projet {name} ?", [decision_ids[0]], "mem.context", {"assertion": "top5"}),
            ("decision_choice", f"Quel choix a été retenu pour {name} ?", [decision_ids[0]], "mem.context", {"assertion": "top5"}),
            ("decision_project", f"Quelles décisions concernent {name} ?", [decision_ids[0]], "mem.context", {"assertion": "top5"}),
            ("goal", f"Quel est l'objectif principal de {name} ?", [goal_ids[0]], "mem.context", {"assertion": "top5"}),
            ("problem", f"Quel problème de capacité touche {name} ?", [problem_ids[0]], "mem.context", {"assertion": "top5"}),
            ("event", f"Quelle étape {name} a été réalisée récemment ?", [f"{slug}-event-047"], "mem.context", {"assertion": "top5"}),
            ("claim", f"Quelle mesure concerne la progression de {name} ?", [f"{slug}-claim-005"], "mem.context", {"assertion": "top5"}),
            ("preference", f"Quelle préférence d'outil existe pour {name} ?", [f"{slug}-preference-000"], "mem.context", {"assertion": "top5"}),
            ("temporal", f"Quels événements du projet {name} ont eu lieu en dernier ?", [f"{slug}-event-047"], "mem.timeline", {"assertion": "latest_observed"}),
            ("contradiction", f"Quelle affirmation contredit la première mesure de {name} ?", [old_claim], "mem.graph.query", {"assertion": "relation_exists", "root_fixture_id": new_claim, "relation_kind": "contradicts"}),
            ("candidate", f"Quelle préférence de {name} doit encore être confirmée ?", [f"{slug}-preference-003"], "mem.object.list", {"assertion": "contains", "kind": "preference", "status": "candidate"}),
            ("namespace_isolation", f"Donne les informations du projet {name} uniquement.", [project_id], "mem.object.list", {"assertion": "namespace_only"}),
            ("graph", f"Quelles décisions soutiennent les objectifs de {name} ?", decision_ids, "mem.graph.query", {"assertion": "relation_targets", "root_fixture_id": goal_ids[0], "relation_kind": "supports"}),
            ("source", f"Quelle source justifie la décision {name} 1 ?", [decision_ids[0]], "mem.explain", {"assertion": "proof"}),
            ("replay", f"Retrouve la décision {name} 1 après une réindexation.", [decision_ids[0]], "mem.object.list", {"assertion": "contains", "kind": "decision"}),
            ("narrative", f"Résume l'évolution de {name} sur la période.", [decision_ids[0]], "mem.narrative.generate", {"assertion": "narrative_sources"}),
        ]
        for query_index, (kind, query, expected, endpoint, assertion) in enumerate(query_specs):
            queries.append({
                "query_id": f"{slug}-query-{query_index:03d}",
                "namespace": namespace,
                "query": query,
                "scenario": kind,
                "endpoint": endpoint,
                "expected_fixture_ids": expected,
                "assertion": assertion,
                "max_rank": 5,
                "must_not_include_namespaces": [f"{NAMESPACE_PREFIX}:{other[0]}" for other in PROJECTS if other[0] != slug],
            })
        llm_cases.append({
            "case_id": f"{slug}-llm-decision-choice",
            "query_id": f"{slug}-query-001",
            "expected_fixture_id": decision_ids[0],
            "required_terms": [name, DECISION_OPTIONS[0]],
            "require_memory_citation": True,
        })

    rng.shuffle(objects)
    rng.shuffle(relations)
    rng.shuffle(queries)
    endpoint_counts = {}
    for query in queries:
        endpoint_counts[query["endpoint"]] = endpoint_counts.get(query["endpoint"], 0) + 1
    manifest = {
        "dataset": "memory-v2-synthetic-2026-09",
        "seed": SEED,
        "privacy": "synthetic-fictional-identifiers-only",
        "namespace_prefix": NAMESPACE_PREFIX,
        "object_count": len(objects),
        "relation_count": len(relations),
        "query_count": len(queries),
        "objects_by_kind": dict(sorted(counts.items())),
        "projects": len(PROJECTS),
        "objects_per_project": len(objects) // len(PROJECTS),
        "queries_per_project": 16,
        "llm_case_count": len(llm_cases),
        "query_endpoints": dict(sorted(endpoint_counts.items())),
        "validation_targets": {
            "top5_recall_min": 0.90,
            "llm_recall_min": 0.85,
            "duplicate_on_replay": 0,
            "unauthorized_evidence": 0,
            "contradiction_without_history_loss": 0,
        },
    }
    return objects, relations, queries, llm_cases, manifest


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.write_text("".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path(__file__).parent)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    objects, relations, queries, llm_cases, manifest = build()
    write_jsonl(args.output / "create_requests.jsonl", objects)
    write_jsonl(args.output / "relations.jsonl", relations)
    write_jsonl(args.output / "queries.jsonl", queries)
    write_jsonl(args.output / "llm_cases.jsonl", llm_cases)
    (args.output / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(manifest, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
