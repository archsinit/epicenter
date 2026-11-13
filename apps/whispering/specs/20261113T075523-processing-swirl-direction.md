# Reverse processing swirl

Roger reports the new cloud looks substantially better, but the processing/transcribing swirl rotates opposite the direction appropriate for its existing spiral shape. Reverse temporal rotation only: preserve the spiral geometry, outward billow emission, recorded waveform, current size and head-return gating.

## Plan

- [x] Reverse the integrated processing phase, with continuous wraparound and exactly unchanged phase when stopped/reduced motion.
- [x] Add a phase-direction/wrap regression test and run focused checks.
- [x] Commit/build normally; do not claim the installed Mac app is changed until a new validated artifact is installed. Current installed runtime is 13c968a.

No transcription model/provider/settings/bindings changes. Model quality/pricing discussion is informational only, based on current official documentation, not a new benchmark or authorization to change models.

## Review

Integrated phase now decreases rather than increases, leaving the GLSL spiral geometry and billow emission unchanged. A regression test verifies reversed advance, continuous wrap and exact phase hold on zero elapsed time; existing return-to-idle/reduced-motion tests pass. Focused suite: 32 Bun tests pass; app check: 0 errors/17 inherited warnings; diff whitespace check passes. Installed runtime is now 02a721b at Roger's request. CI 43389968995 passed 32 Bun +16 Rust tests, types, Intel build, signature and startup checks; actual report and relevant log entries read. The installed executable SHA matches the artifact, deep/strict signature verification passed on his Mac, and the prior bundle was backed up at the stable fork installation path. Actual AX confirms running app, visible companion, WebGL renderer ready and tray present. No model/provider/settings/binding/permission changes or recording/global-input tests were made. Visual direction is implemented/installed, but no native video capture or broad physical-input acceptance is claimed.
