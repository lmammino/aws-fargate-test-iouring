#!/usr/bin/env bash
# Replaces account ids, subnet/ENI ids, MAC and private IP addresses in
# collected results, so they can be shared. Edits files in place.
# Usage: scripts/redact-results.sh [results-dir]   (default: results/)
set -euo pipefail

dir="${1:-$(cd "$(dirname "$0")/.." && pwd)/results}"
[[ -d "$dir" ]] || { echo "no such directory: $dir" >&2; exit 1; }

find "$dir" -type f \( -name '*.json' -o -name '*.log' \) -print0 | while IFS= read -r -d '' file; do
  sed -E -i.bak \
    -e 's/(arn:aws:[a-z0-9-]+:[a-z0-9-]*:)[0-9]{12}:/\1ACCOUNT_ID:/g' \
    -e 's/[0-9]{12}(\.dkr\.ecr\.)/ACCOUNT_ID\1/g' \
    -e 's/subnet-[0-9a-f]{8,17}/subnet-REDACTED/g' \
    -e 's/eni-[0-9a-f]{8,17}/eni-REDACTED/g' \
    -e 's/([0-9a-f]{2}:){5}[0-9a-f]{2}/MAC-REDACTED/g' \
    -e 's/ip-10-[0-9]+-[0-9]+-[0-9]+/ip-REDACTED/g' \
    -e 's/10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}/PRIVATE-IP-REDACTED/g' \
    "$file"
  rm -f "$file.bak"
done
echo "redacted $dir" >&2
