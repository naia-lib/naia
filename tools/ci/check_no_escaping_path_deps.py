#!/usr/bin/env python3
"""Fail if any workspace member depends on a path outside this repo.

Standing invariant (Connor ruling 2026-09-17, Kepler seq9606): sibling path
dependencies are dev-only and must never reach naia `dev` or `main`, because
naia is public and external consumers clone it with no siblings present.
Any `path =` dep resolving outside the repo root breaks `cargo metadata`
for those consumers.

Usage: python3 tools/ci/check_no_escaping_path_deps.py [--root DIR]
Exit 0 when clean, 1 listing every escaping dep otherwise.
Stdlib only. Regex-based: manifests are machine-written TOML and this is a
lint, not a resolver.
"""

import re
import sys
from pathlib import Path

MEMBER_RE = re.compile(r'"([^"]+)"')
PATH_DEP_RE = re.compile(r'path\s*=\s*"([^"]+)"')


def members_of(root_manifest: Path) -> list[str]:
    text = root_manifest.read_text()
    ws = text.index("[workspace]")
    members_idx = text.index("members", ws)
    # take the [...] block following "members"
    start = text.index("[", members_idx)
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "[":
            depth += 1
        elif text[i] == "]":
            depth -= 1
            if depth == 0:
                block = text[start : i + 1]
                break
    else:
        raise SystemExit("could not parse members block")
    # strip line comments: a disabled member (e.g. `# "bench/iai"`) is not a member
    block = "\n".join(line.split("#", 1)[0] for line in block.splitlines())
    return MEMBER_RE.findall(block)


def main() -> int:
    root = Path(sys.argv[sys.argv.index("--root") + 1]) if "--root" in sys.argv else Path(__file__).resolve().parents[2]
    root = root.resolve()
    offenders: list[str] = []
    members = members_of(root / "Cargo.toml")
    for member in members:
        manifest = root / member / "Cargo.toml"
        if not manifest.is_file():
            offenders.append(f"{member}: manifest missing")
            continue
        for lineno, line in enumerate(manifest.read_text().splitlines(), 1):
            m = PATH_DEP_RE.search(line.split("#", 1)[0])
            if not m:
                continue
            resolved = (root / member / m.group(1)).resolve()
            try:
                resolved.relative_to(root)
            except ValueError:
                offenders.append(f"{member}/Cargo.toml:{lineno}: path {m.group(1)!r} escapes repo")
    if offenders:
        print(f"FAIL: {len(offenders)} repo-escaping path dep(s) in {len(members)} workspace members:")
        for o in offenders:
            print(f"  {o}")
        return 1
    print(f"OK: {len(members)} workspace members, no path dep escapes {root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
