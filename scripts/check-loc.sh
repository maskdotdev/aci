#!/usr/bin/env bash
set -euo pipefail

limit="${ACI_LOC_LIMIT:-800}"
status=0

while IFS= read -r file; do
  lines="$(wc -l < "$file" | tr -d ' ')"
  if [ "$lines" -gt "$limit" ]; then
    echo "$file has $lines lines, over limit $limit"
    status=1
  fi
done < <(find crates -type f -name '*.rs' -not -path '*/target/*' | sort)

exit "$status"
