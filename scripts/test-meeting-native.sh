#!/usr/bin/env bash
# macOS SDK + synthetic sample buffers only. No screen/microphone capture,
# permission requests, UI automation, provider calls, or app installation.
set -euo pipefail
if [[ "$(uname -s)" != Darwin ]]; then
    printf '%s\n' 'Meeting native fixture requires a Mac SDK.' >&2
    exit 1
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/whispering-meeting-native.XXXXXXXX")"
trap 'rm -rf "$work"' EXIT
xcrun clang -fobjc-arc -O2 -mmacosx-version-min=10.15 \
    -Werror=unguarded-availability-new -Werror=incompatible-pointer-types \
    -Werror=deprecated-declarations \
    "$root/apps/whispering/src-tauri/src/meeting/capture_macos.test.m" \
    -framework AppKit -framework AVFoundation -framework CoreMedia \
    -framework CoreGraphics -weak_framework ScreenCaptureKit \
    -o "$work/capture-tests"
"$work/capture-tests"
