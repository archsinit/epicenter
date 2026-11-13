#!/usr/bin/env bash
# Test production Rust meeting modules without Tauri, live capture, provider
# calls, or heavyweight transcription dependencies. All builds are offline.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/whispering-meeting-tests.XXXXXXXX")"
trap 'rm -rf "$work"' EXIT
python3 - "$root" "$work" <<'PY'
import json
import pathlib
import sys
root, work = map(pathlib.Path, sys.argv[1:])
module = root / 'apps/whispering/src-tauri/src/meeting'
dependencies = '''[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
hound = "3.5"
'''
(work / 'Cargo.toml').write_text('''[package]
name = "whispering-meeting-core-tests"
version = "0.0.0"
edition = "2021"
[workspace]
[lib]
path = ''' + json.dumps(str(module / 'mod.rs')) + '\n' + dependencies)
# Type-check the real macOS Rust FFI wrapper even when this script runs on Linux.
# cargo check does not link or execute platform APIs.
bridge = work / 'bridge'
bridge.mkdir()
(bridge / 'Cargo.toml').write_text('''[package]
name = "whispering-meeting-bridge-check"
version = "0.0.0"
edition = "2021"
[workspace]
[lib]
path = "lib.rs"
''' + dependencies)
(bridge / 'lib.rs').write_text(''.join(
    '#[path = ' + json.dumps(str(module / (name + '.rs'))) + ']\npub mod ' + name + ';\n'
    for name in ['audio', 'macos', 'schedule', 'storage', 'timeline', 'worker', 'backend_macos', 'sources', 'control', 'controller', 'request', 'capture_process']))
PY
if [[ "$(uname -s)" == Darwin ]]; then
    xcrun clang -fobjc-arc -O2 -mmacosx-version-min=10.15 \
        -Werror=unguarded-availability-new -Werror=incompatible-pointer-types \
        -c "$root/apps/whispering/src-tauri/src/meeting/capture_macos.m" -o "$work/capture.o"
    xcrun ar crs "$work/libwhispering_meeting_capture.a" "$work/capture.o"
    # Keep paths containing spaces intact by using a generated Cargo config.
    python3 - "$work" <<'PY'
import json
import pathlib
import sys
work = pathlib.Path(sys.argv[1])
(work / 'link.toml').write_text('[build]\nrustflags = ' + json.dumps([
    '-L', 'native=' + str(work), '-l', 'static=whispering_meeting_capture',
    '-l', 'framework=AppKit', '-l', 'framework=AVFoundation',
    '-l', 'framework=CoreMedia', '-l', 'framework=CoreGraphics',
    '-C', 'link-arg=-Wl,-weak_framework,ScreenCaptureKit',
]))
PY
    cargo --config "$work/link.toml" test --offline --manifest-path "$work/Cargo.toml" "$@"
else
    cargo test --offline --manifest-path "$work/Cargo.toml" "$@"
    CARGO_TARGET_DIR="$work/target" cargo check --tests --offline --manifest-path "$work/bridge/Cargo.toml"
fi
