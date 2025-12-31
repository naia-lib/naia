#!/usr/bin/env python3
"""
Calculate E2E test pass rate from cargo test output.

Usage:
    cargo test -p naia-test 2>&1 | python3 scripts/calculate_pass_rate.py
    OR
    cargo test -p naia-test 2>&1 > /tmp/test_output.txt
    python3 scripts/calculate_pass_rate.py < /tmp/test_output.txt
"""

import sys
import re
import argparse

def main():
    parser = argparse.ArgumentParser(description='Calculate E2E test pass rate')
    parser.add_argument('--percentage-only', action='store_true',
                        help='Output only the pass rate percentage')
    args = parser.parse_args()
    
    total_passed = 0
    total_failed = 0
    test_files = {}  # Maps test file name to (passed, failed)
    current_file = None
    
    for line in sys.stdin:
        # Track current test file
        file_match = re.search(r'Running tests/(\w+)\.rs', line)
        if file_match:
            current_file = file_match.group(1)
        
        # Match test result lines: "test result: ok. X passed; Y failed..." or "test result: FAILED. X passed; Y failed..."
        match = re.search(r'test result: (?:ok\.|FAILED\.)\s+(\d+)\s+passed;\s+(\d+)\s+failed', line)
        if match:
            passed = int(match.group(1))
            failed = int(match.group(2))
            
            # Skip empty test suites (0 passed, 0 failed)
            if passed == 0 and failed == 0:
                continue
            
            total_passed += passed
            total_failed += failed
            
            # Track results by file name
            if current_file:
                if current_file not in test_files:
                    test_files[current_file] = [0, 0]
                test_files[current_file][0] += passed
                test_files[current_file][1] += failed
    
    total = total_passed + total_failed
    
    if total == 0:
        print("ERROR: No test results found. Make sure you're piping cargo test output.")
        sys.exit(1)
    
    pass_rate = (total_passed / total) * 100
    
    if args.percentage_only:
        print(f"{pass_rate:.1f}%")
        return 0 if total_failed == 0 else 1
    
    print("=" * 60)
    print("E2E TEST SUITE PASS RATE")
    print("=" * 60)
    print(f"PASSED:  {total_passed}")
    print(f"FAILED:  {total_failed}")
    print(f"TOTAL:   {total}")
    print(f"PASS RATE: {pass_rate:.1f}%")
    print("=" * 60)
    
    # Show breakdown by test file if we have failures
    if total_failed > 0:
        print("\nBreakdown by test file:")
        print("-" * 60)
        for file_name in sorted(test_files.keys()):
            p, f = test_files[file_name]
            if f > 0:
                print(f"  {file_name}.rs: {p} passed, {f} failed")
    
    return 0 if total_failed == 0 else 1

if __name__ == "__main__":
    sys.exit(main())
