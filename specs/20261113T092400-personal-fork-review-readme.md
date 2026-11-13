# Personal fork review and README cleanup

## Plan
- [x] Review the recent GPT-Transcribe and recording-travel changes against the installed version and checkpoint; avoid unrelated Android work and existing handoff edits.
- [x] Apply narrowly scoped formatting cleanup without changing runtime behavior.
- [x] Replace the stale root README with a short, accurate personal-fork description, changes, upstream credit and unchanged license.
- [x] Run focused tests/type checks and review the final diff; commit and push to `whispering-intel`.

Roger explicitly requested this review, cleanup, commit and push. Installed runtime `5e5a282` is already committed; no app restart/reinstallation is needed for documentation/formatting alone.

## Review
Reviewed recording-travel state transitions, the AppKit/Rust snapshot contract, return-before-spin gating, interrupted takes and captured home, pointer-mode restoration, Reduce Motion, OpenAI language serialization and unchanged model defaults. No blocking issues identified in this focused review; it is not a whole-monorepo audit. Native visible-animation/packaged drag/focus acceptance and monitor hot-plug behavior remain manual validation items, not claimed passes.

Applied Biome formatting/import organization only to the three recent OpenAI implementation/test files. No recording, animation, provider, permission, dependency or settings behavior changes. Replaced stale root README build-status text with a short personal-fork introduction, changes, upstream credit, build-workflow link and signing limitation. Original AGPL license and copyright notices remain unchanged; third-party character rights are acknowledged separately. Confirmed upstream repository identity and that the fork's default branch is `whispering-intel`.

Validation: 51 focused Bun tests pass (478 assertions), app types 0 errors/17 inherited warnings, targeted Biome checks and git whitespace checks pass. Prior runtime `5e5a282` already passed Intel CI 43397163550; no new native build or reinstall is necessary for this formatting/documentation cleanup. Existing unrelated HANDOFF.md/WORK-STATE.md edits and untracked Android work are intentionally not staged. Rollback checkpoint `a63f9ae` and approved artwork remain intact.
