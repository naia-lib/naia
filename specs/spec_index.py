#!/usr/bin/env python3
"""
spec_index.py - Fast adequacy checking without subprocess loops

Scans:
- specs/contracts/*.md for contracts + obligation IDs
- test/tests/*.rs for Contract: blocks, function bodies, and spec_expect labels

Outputs JSON with ranked queues:
- NEEDS_TESTS: contracts without test annotations
- MISSING_OBLIGATIONS: contracts with obligations but some labels missing
- NEEDS_LABELS: contracts with tests but no labels
- OK: contracts meeting adequacy requirements

Must run fast (<2s) without cargo.
"""

import re
import sys
import json
from pathlib import Path
from typing import Dict, List, Set, Tuple
from collections import defaultdict


def extract_contracts_from_specs(contracts_dir: Path) -> Tuple[Set[str], Dict[str, List[str]]]:
    """
    Extract contract IDs and their obligations from spec files.

    Returns:
        (all_contracts, contract_obligations)
        - all_contracts: set of all contract IDs
        - contract_obligations: dict mapping contract_id -> list of obligation IDs (t1, t2, etc.)
    """
    all_contracts = set()
    contract_obligations = defaultdict(list)

    contract_pattern = re.compile(r'###\s+\[([a-z][a-z0-9-]*-[0-9]+[a-z]*)\]')
    obligations_header_pattern = re.compile(r'^\*\*Obligations:\*\*')
    obligation_item_pattern = re.compile(r'^-\s+\*\*(t[0-9]+)\*\*:')

    for spec_file in sorted(contracts_dir.glob('*.md')):
        with open(spec_file, 'r', encoding='utf-8') as f:
            lines = f.readlines()

        current_contract = None
        in_contract = False
        in_obligations = False

        for line in lines:
            line = line.rstrip()

            # Detect contract heading
            match = contract_pattern.match(line)
            if match:
                current_contract = match.group(1)
                all_contracts.add(current_contract)
                in_contract = True
                in_obligations = False
                continue

            # End of contract (next ### or ##)
            if in_contract and (line.startswith('### ') or line.startswith('## ')):
                current_contract = None
                in_contract = False
                in_obligations = False
                continue

            # Detect **Obligations:** header
            if in_contract and obligations_header_pattern.match(line):
                in_obligations = True
                continue

            # Extract obligation IDs
            if in_obligations:
                match = obligation_item_pattern.match(line)
                if match:
                    obligation_id = match.group(1)
                    if obligation_id not in contract_obligations[current_contract]:
                        contract_obligations[current_contract].append(obligation_id)
                # End obligations section if we hit a non-matching line that's not blank
                elif line and not line.startswith(' ') and not line.startswith('-'):
                    in_obligations = False

    return all_contracts, dict(contract_obligations)


def extract_test_annotations(test_dir: Path) -> Tuple[Dict[str, Set[str]], Dict[str, Set[str]]]:
    """
    Extract contract annotations and labels from test files.

    Returns:
        (contract_to_testfiles, contract_to_labels)
        - contract_to_testfiles: dict mapping contract_id -> set of test filenames
        - contract_to_labels: dict mapping contract_id -> set of assertion labels
    """
    contract_to_testfiles = defaultdict(set)
    contract_to_labels = defaultdict(set)

    contract_annotation_pattern = re.compile(r'///\s*Contract:\s*(.+)')
    contract_id_pattern = re.compile(r'\[([a-z][a-z0-9-]*-[0-9]+[a-z]*)\]')
    label_pattern = re.compile(r'\.?(spec_expect|expect_msg)\("([^"\\]+(?:\\.[^"\\]*)*)"')

    for test_file in sorted(test_dir.glob('*.rs')):
        test_basename = test_file.name

        with open(test_file, 'r', encoding='utf-8') as f:
            content = f.read()

        lines = content.split('\n')
        i = 0
        while i < len(lines):
            line = lines[i]

            # Check for contract annotation
            match = contract_annotation_pattern.match(line.strip())
            if match:
                annotation_line = match.group(1)
                # Extract all contract IDs from this annotation line
                contract_ids = contract_id_pattern.findall(annotation_line)

                # Now collect the function body to extract labels
                # Skip ahead to find the function
                j = i + 1
                while j < len(lines) and not lines[j].strip().startswith('fn '):
                    j += 1

                if j < len(lines):
                    # Found function, extract body until braces balance
                    brace_depth = 0
                    fn_body_lines = []
                    k = j
                    seen_opening_brace = False
                    while k < len(lines):
                        fn_line = lines[k]
                        fn_body_lines.append(fn_line)
                        brace_depth += fn_line.count('{') - fn_line.count('}')
                        if '{' in fn_line:
                            seen_opening_brace = True
                        k += 1
                        # Stop when braces are balanced after seeing at least one opening brace
                        if seen_opening_brace and brace_depth == 0:
                            break

                    fn_body = '\n'.join(fn_body_lines)

                    # Extract labels from function body
                    labels = []
                    for label_match in label_pattern.finditer(fn_body):
                        label = label_match.group(2)
                        if label not in labels:  # Deduplicate
                            labels.append(label)

                    # Record test files and labels for each contract
                    for cid in contract_ids:
                        contract_to_testfiles[cid].add(test_basename)
                        for label in labels:
                            contract_to_labels[cid].add(label)

                    i = k
                    continue

            i += 1

    return contract_to_testfiles, contract_to_labels


def categorize_contracts(
    all_contracts: Set[str],
    contract_obligations: Dict[str, List[str]],
    contract_to_testfiles: Dict[str, Set[str]],
    contract_to_labels: Dict[str, Set[str]]
) -> Dict[str, List[Dict]]:
    """
    Categorize contracts into ranked queues.

    Returns:
        Dict with keys: NEEDS_TESTS, MISSING_OBLIGATIONS, NEEDS_LABELS, OK
    """
    needs_tests = []
    missing_obligations = []
    needs_labels = []
    ok_contracts = []

    for cid in sorted(all_contracts):
        test_files = contract_to_testfiles.get(cid, set())
        labels = contract_to_labels.get(cid, set())
        obligations = contract_obligations.get(cid, [])

        # Category 1: NEEDS_TESTS
        if not test_files:
            needs_tests.append({"contract": cid, "obligations": obligations})
            continue

        # Category 2: NEEDS_LABELS
        if not labels:
            needs_labels.append({
                "contract": cid,
                "test_files": sorted(test_files),
                "obligations": obligations
            })
            continue

        # Category 3: Check obligation coverage
        if obligations:
            missing_obls = []
            for obl in obligations:
                # Check if any label matches pattern: cid.obl:
                pattern = f"{cid}.{obl}:"
                if not any(pattern in label for label in labels):
                    missing_obls.append(obl)

            if missing_obls:
                missing_obligations.append({
                    "contract": cid,
                    "test_files": sorted(test_files),
                    "obligations": obligations,
                    "missing": missing_obls,
                    "labels": sorted(labels)
                })
                continue
        else:
            # No obligations: check for contract-level label
            has_contract_label = any(
                label.startswith(f"{cid}.") or label.startswith(f"{cid}:")
                for label in labels
            )
            if not has_contract_label:
                needs_labels.append({
                    "contract": cid,
                    "test_files": sorted(test_files),
                    "obligations": [],
                    "labels": sorted(labels)
                })
                continue

        # Category 4: OK
        ok_contracts.append({
            "contract": cid,
            "test_files": sorted(test_files),
            "obligations": obligations,
            "labels": sorted(labels)
        })

    return {
        "NEEDS_TESTS": needs_tests,
        "MISSING_OBLIGATIONS": missing_obligations,
        "NEEDS_LABELS": needs_labels,
        "OK": ok_contracts
    }


def main():
    if len(sys.argv) < 3:
        print("Usage: spec_index.py <contracts_dir> <test_dir>", file=sys.stderr)
        sys.exit(1)

    contracts_dir = Path(sys.argv[1])
    test_dir = Path(sys.argv[2])

    if not contracts_dir.is_dir():
        print(f"Error: contracts_dir not found: {contracts_dir}", file=sys.stderr)
        sys.exit(1)

    if not test_dir.is_dir():
        print(f"Error: test_dir not found: {test_dir}", file=sys.stderr)
        sys.exit(1)

    # Extract data
    all_contracts, contract_obligations = extract_contracts_from_specs(contracts_dir)
    contract_to_testfiles, contract_to_labels = extract_test_annotations(test_dir)

    # Categorize
    categories = categorize_contracts(
        all_contracts,
        contract_obligations,
        contract_to_testfiles,
        contract_to_labels
    )

    # Output JSON
    output = {
        "total_contracts": len(all_contracts),
        "categories": categories,
        "summary": {
            "needs_tests": len(categories["NEEDS_TESTS"]),
            "missing_obligations": len(categories["MISSING_OBLIGATIONS"]),
            "needs_labels": len(categories["NEEDS_LABELS"]),
            "ok": len(categories["OK"])
        }
    }

    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
