#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
source="$root/directories.env"
dest="$root/.env"

if [[ ! -f "$source" ]]; then
  echo "directories.env not found at $source" >&2
  exit 1
fi

{
  echo "# Generated from directories.env — edit directories.env and re-run scripts/sync-env.ps1"
  echo "# or scripts/sync-env.sh to refresh."
  echo
  grep -v '^#' "$source" | sed '/^[[:space:]]*$/d'
  echo
} >"$dest"

echo "Wrote $dest from directories.env"
