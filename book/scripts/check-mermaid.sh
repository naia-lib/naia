#!/usr/bin/env bash
set -euo pipefail

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
npm --prefix "$tmpdir" install mermaid jsdom dompurify >/dev/null
checker="$tmpdir/check.mjs"
cat > "$checker" <<'EOF'
import { readFile } from 'node:fs/promises';
import { JSDOM } from './node_modules/jsdom/lib/api.js';
import createDOMPurify from './node_modules/dompurify/dist/purify.es.mjs';

const { window } = new JSDOM('<!doctype html><html><body></body></html>');
globalThis.window = window;
globalThis.document = window.document;
globalThis.DOMPurify = createDOMPurify(window);

const mermaid = (await import('./node_modules/mermaid/dist/mermaid.esm.mjs')).default;
mermaid.initialize({ startOnLoad: false });

const file = process.argv[2];
await mermaid.parse(await readFile(file, 'utf8'));
EOF

count=0

while IFS= read -r file; do
  in_block=0
  diagram=""
  start_line=0
  line_no=0

  while IFS= read -r line || [[ -n "$line" ]]; do
    line_no=$((line_no + 1))
    if [[ "$in_block" -eq 0 && "$line" == '```mermaid' ]]; then
      in_block=1
      diagram=""
      start_line=$line_no
      continue
    fi
    if [[ "$in_block" -eq 1 && "$line" == '```' ]]; then
      count=$((count + 1))
      diagram_file="$tmpdir/diagram-$count.mmd"
      printf '%s\n' "$diagram" > "$diagram_file"
      echo "Checking Mermaid diagram $file:$start_line"
      node "$checker" "$diagram_file"
      in_block=0
      continue
    fi
    if [[ "$in_block" -eq 1 ]]; then
      diagram+="$line"$'\n'
    fi
  done < "$file"

  if [[ "$in_block" -eq 1 ]]; then
    echo "Unclosed Mermaid block in $file:$start_line" >&2
    exit 1
  fi
done < <(find book/src -name '*.md' -print | sort)

echo "Checked $count Mermaid diagram(s)."
