#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
source="$root/directories.env"
dest="$root/.env"

if [[ ! -f "$source" ]]; then
  echo "directories.env not found at $source" >&2
  exit 1
fi

quote_value() {
  local value="${1%\"}"
  value="${value#\"}"
  value="${value//\\//}"
  printf '"%s"' "$value"
}

{
  echo "# Generated from directories.env — edit directories.env and re-run scripts/sync-env.ps1"
  echo "# or scripts/sync-env.sh to refresh."
  echo
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%%$'\r'}"
    [[ "$line" =~ ^[[:space:]]*# ]] && continue
    [[ "$line" =~ ^[[:space:]]*$ ]] && continue
    if [[ "$line" =~ ^[[:space:]]*([^=]+)=(.*)$ ]]; then
      key="${BASH_REMATCH[1]// /}"
      value="${BASH_REMATCH[2]}"
      printf '%s=%s\n' "$key" "$(quote_value "$value")"
    fi
  done <"$source"
  echo
} >"$dest"

echo "Wrote $dest from directories.env"
