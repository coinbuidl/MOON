#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

patterns=(
  "/Users/"
  "C:\\Users\\"
  "moon-system"
  "oc-token-optim"
)

failed=0
for pattern in "${patterns[@]}"; do
  if rg -n --hidden \
    -g '!target' \
    -g '!.git' \
    -g '!.env' \
    -g '!scripts/guard_no_hardcoding.sh' \
    -F "$pattern" .; then
    echo "hardcoding-guard: found forbidden pattern: $pattern" >&2
    failed=1
  fi
done

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi

echo "hardcoding-guard: ok"
