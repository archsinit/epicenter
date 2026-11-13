# Reverse processing swirl

Roger reports the new cloud looks substantially better, but the processing/transcribing swirl rotates opposite the direction appropriate for its existing spiral shape. Reverse temporal rotation only: preserve the spiral geometry, outward billow emission, recorded waveform, current size and head-return gating.

## Plan

- [x] Reverse the integrated processing phase, with continuous wraparound and exactly unchanged phase when stopped/reduced motion.
- [x] Add a phase-direction/wrap regression test and run focused checks.
- [ ] Commit/build normally; do not claim the installed Mac app is changed until a new validated artifact is installed. Current installed runtime is 13c968a.

No transcription model/provider/settings/bindings changes. Model quality/pricing discussion is informational only, based on current official documentation, not a new benchmark or authorization to change models.

## Review

Integrated phase now decreases rather than increases, leaving the GLSL spiral geometry and billow emission unchanged. A regression test verifies reversed advance, continuous wrap and exact phase hold on zero elapsed time; existing return-to-idle/reduced-motion tests pass. Focused suite: 32 Bun tests pass; app check: 0 errors/17 inherited warnings; diff whitespace check passes. New native artifact/install still pending; currently installed runtime remains 13c968a.
