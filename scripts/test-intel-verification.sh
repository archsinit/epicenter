#!/usr/bin/env bash
# Offline regression: a failed verification command must remain failed through tee.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
temporary=$(mktemp -d "${TMPDIR:-/tmp}/whispering-intel-ci.XXXXXX")
trap 'rm -rf "$temporary"' EXIT

status=0
bash -eo pipefail -c '(exit 42) | tee "$1"' -- "$temporary/failure.txt" || status=$?
[[ "$status" -eq 42 ]] || { echo "ERROR: tee masked exit 42 (got $status)"; exit 1; }
echo 'PASS: Pipeline preserves a nonzero command exit.'

bash -eo pipefail -c 'printf "success\n" | tee "$1"' -- "$temporary/success.txt" >/dev/null
[[ -s "$temporary/success.txt" ]] || { echo 'ERROR: Success report missing'; exit 1; }
echo 'PASS: Successful pipeline writes its report.'

# A nonexistent bundle must fail, even when the output is captured. This runs
# without launching any app and without microphone, network or GUI permissions.
if bash -eo pipefail -c 'bash "$1" "$2" 2>&1 | tee "$3"' -- \
  "$root/scripts/verify-intel-app.sh" "$temporary/missing.app" "$temporary/invalid-bundle.txt" >/dev/null; then
  echo 'ERROR: Invalid bundle passed verification through tee.'
  exit 1
fi
[[ -s "$temporary/invalid-bundle.txt" ]] || { echo 'ERROR: Failure diagnostics missing'; exit 1; }
echo 'PASS: Invalid app bundle fails verification and preserves diagnostics.'
