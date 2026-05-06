#!/usr/bin/env python3
"""SDD migration coverage diff: legacy_tests vs namako features.

Walks `test/harness/legacy_tests/` and `test/specs/features/` for
`[contract-id]` tags and produces a per-ID status table.

Usage:

    python3 _AGENTS/scripts/coverage_diff.py
    python3 _AGENTS/scripts/coverage_diff.py --markdown    # markdown table
    python3 _AGENTS/scripts/coverage_diff.py --legacy-only # contracts pending migration
    python3 _AGENTS/scripts/coverage_diff.py --json        # machine-readable

Run from the repo root.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

# Regex matches `[contract-id-NN]` or `[contract-id-NN-suffix]` where
# `contract-id` is one of the known contract-area names. We anchor on
# the area names to avoid matching unrelated bracketed text.
CONTRACT_AREAS = [
    "common",
    "connection",
    "transport",
    "messaging",
    "time-ticks",
    "observability",
    "entity-scopes",
    "entity-replication",
    "entity-ownership",
    "entity-publication",
    "entity-delegation",
    "entity-authority",
    "server-events",
    "client-events",
    "world-integration",
    "scope-exit",
    "scope-propagation",
    "update-candidate",
    "spawn-with-components",
    "immutable-components",
    "priority-accumulator",
    "replicated-resources",
]
CONTRACT_RE = re.compile(
    r"\[(" + "|".join(CONTRACT_AREAS) + r")-[0-9a-z]+\]",
)


def collect_ids(root: Path, glob: str) -> dict[str, set[Path]]:
    """Walk `root` for files matching `glob`. Return {contract_id: {file, ...}}."""
    ids: dict[str, set[Path]] = defaultdict(set)
    for path in sorted(root.rglob(glob)):
        try:
            text = path.read_text()
        except UnicodeDecodeError:
            continue
        for match in CONTRACT_RE.findall(text):
            # match looks like "messaging-04" — strip the brackets the regex
            # already excluded.
            ids[match].add(path)  # the regex captured the area; reconstruct full id
    # Re-extract with the full match so the dict keys include the suffix.
    full_ids: dict[str, set[Path]] = defaultdict(set)
    for path in sorted(root.rglob(glob)):
        try:
            text = path.read_text()
        except UnicodeDecodeError:
            continue
        for full in re.findall(r"\[((?:" + "|".join(CONTRACT_AREAS) + r")-[0-9a-z]+)\]", text):
            full_ids[full].add(path)
    return full_ids


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--markdown", action="store_true", help="emit markdown table")
    ap.add_argument("--legacy-only", action="store_true", help="emit only IDs missing from namako")
    ap.add_argument("--json", action="store_true", help="emit JSON")
    args = ap.parse_args()

    repo_root = Path(__file__).resolve().parents[2]
    legacy_dir = repo_root / "test/harness/legacy_tests"
    features_dir = repo_root / "test/specs/features"

    if not legacy_dir.exists():
        legacy: dict[str, set[Path]] = {}
        legacy_msg = "(legacy_tests/ deleted — Phase F complete)"
    else:
        legacy = collect_ids(legacy_dir, "*.rs")
        legacy_msg = f"({len(legacy)} contract IDs in legacy_tests/)"

    namako = collect_ids(features_dir, "*.feature")

    legacy_only = sorted(set(legacy) - set(namako))
    namako_only = sorted(set(namako) - set(legacy))
    both = sorted(set(legacy) & set(namako))

    if args.json:
        print(
            json.dumps(
                {
                    "legacy_count": len(legacy),
                    "namako_count": len(namako),
                    "both": both,
                    "legacy_only": legacy_only,
                    "namako_only": namako_only,
                    "legacy_message": legacy_msg,
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0

    if args.legacy_only:
        for cid in legacy_only:
            print(cid)
        return 0

    if args.markdown:
        print("# SDD Migration Coverage Diff\n")
        print("> **AUTO-GENERATED** by `_AGENTS/scripts/coverage_diff.py`. To refresh:")
        print("> `python3 _AGENTS/scripts/coverage_diff.py --markdown > _AGENTS/SDD_COVERAGE_DIFF.md`")
        print(">")
        print("> Living artifact for Phase D progress: every contract ID currently in")
        print("> the **Pending migration** table is a Phase D target. When the table")
        print("> empties, the parity gate for Phase F (delete legacy_tests) is met.\n")
        print(f"- Legacy {legacy_msg}")
        print(f"- Namako: {len(namako)} contract IDs in features/")
        print(f"- Both: **{len(both)}**")
        print(f"- Legacy-only (PENDING migration): **{len(legacy_only)}**")
        print(f"- Namako-only (new in SDD): {len(namako_only)}\n")
        if legacy_only:
            print("## Pending migration\n")
            print("| Contract ID | Source files |")
            print("|---|---|")
            for cid in legacy_only:
                paths = ", ".join(p.relative_to(repo_root).as_posix() for p in sorted(legacy[cid]))
                print(f"| `{cid}` | {paths} |")
        return 0

    # Default human-readable summary.
    print(f"=== Coverage diff ===")
    print(f"Legacy {legacy_msg}")
    print(f"Namako: {len(namako)} contract IDs in features/")
    print(f"Both: {len(both)}")
    print(f"Legacy-only (pending): {len(legacy_only)}")
    print(f"Namako-only: {len(namako_only)}")
    if legacy_only:
        print(f"\nPending migration ({len(legacy_only)}):")
        for cid in legacy_only:
            print(f"  {cid}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
