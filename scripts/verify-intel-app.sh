#!/usr/bin/env bash
# Run on an Intel Mac after building. No microphone access or provider requests.
set -euo pipefail

app="${1:?Usage: verify-intel-app.sh path/to/Whispering Intel.app}"
[[ "$(uname -s)" == Darwin ]] || { echo 'ERROR: This check requires macOS'; exit 1; }
[[ "$(uname -m)" == x86_64 ]] || { echo 'ERROR: This check requires an Intel Mac'; exit 1; }
plist="$app/Contents/Info.plist"
executable=$(/usr/libexec/PlistBuddy -c 'Print CFBundleExecutable' "$plist")
identifier=$(/usr/libexec/PlistBuddy -c 'Print CFBundleIdentifier' "$plist")
binary="$app/Contents/MacOS/$executable"
[[ "$identifier" == com.archsinit.whispering-intel ]] || { echo 'ERROR: Wrong app identity'; exit 1; }
architectures=$(/usr/bin/lipo -archs "$binary")
[[ "$architectures" == x86_64 ]] || { echo "ERROR: Unexpected architectures: $architectures"; exit 1; }
/usr/bin/codesign --verify --deep --strict --verbose=2 "$app"
printf 'PASS: Intel-only Mach-O, fork app identity, and bundle signature verification.\n'
printf 'Host macOS: %s; architecture: %s\n' "$(sw_vers -productVersion)" "$(uname -m)"

# Some hosted runners have no logged-in GUI session. Do not equate a skipped
# launch with passing UI tests, or try to change OS permission/security settings.
if ! /bin/launchctl print "gui/$(id -u)" >/dev/null 2>&1; then
  printf 'SKIP: App launch smoke test; no GUI login session available.\n'
  exit 0
fi

log=$(mktemp -t whispering-intel-launch)
pid=''
cleanup() {
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill -TERM "$pid" 2>/dev/null || true
    for _ in {1..5}; do
      kill -0 "$pid" 2>/dev/null || break
      sleep 1
    done
    if kill -0 "$pid" 2>/dev/null; then kill -KILL "$pid" 2>/dev/null || true; fi
    wait "$pid" 2>/dev/null || true
  fi
  rm -f "$log"
}
trap cleanup EXIT
"$binary" >"$log" 2>&1 &
pid=$!
for _ in {1..8}; do
  sleep 1
  if ! kill -0 "$pid" 2>/dev/null; then
    echo 'ERROR: App process exited during the eight-second startup check.'
    tail -100 "$log"
    exit 1
  fi
done
printf 'PASS: Built app process stayed alive for eight seconds in a Mac GUI session.\n'
printf 'NOT TESTED: Window/UI readiness, Dock interactions, microphone, text insertion, mouse suppression or permissions.\n'
