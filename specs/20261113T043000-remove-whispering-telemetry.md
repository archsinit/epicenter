# Remove telemetry from the Intel fork

Roger requested removal of telemetry on 13 November 2026. Implementation/tests are approved under the existing fork work approval.

## Plan

- [x] Remove frontend event collection, analytics services/query and opt-in settings/UI.
- [x] Remove native Aptabase initialization/lifecycle reporting, capability and SDK dependencies.
- [x] Update lockfiles and inaccurate app documentation; add a focused source regression guard.
- [x] Run focused tests and app type checking. Record build/device limits; do not reinstall the Mac app without coordinating the changed build/signature.

Keep provider requests, local diagnostic logs, Dock fixes and overlay design separate. Leave the Android work untouched. Existing stored analytics preferences must not re-enable collection.

## Review

Removed all existing frontend event calls and recording-duration collection, the query/service modules, analytics toggle/page/navigation, native plugin/lifecycle events, capability and JS/Rust SDK dependencies. Cargo metadata regenerated the lockfile without dependency upgrades; offline regeneration initially failed because the cached Git dependency lacked origin/HEAD, then normal metadata resolution succeeded. Added three source/dependency/UI guards to the existing focused CI command. These are source guards, not a runtime network audit.

Validation: 19 focused Bun tests passed (three new guards plus 16 existing fork tests); app check zero errors/17 inherited warnings; git diff --check passed. Provider request code and local diagnostic logs remain unchanged. No new native Mac build, runtime network audit or install was performed. Roger's working Mac install is still dd43322 and does NOT yet contain this removal. Dock visibility bug and overlay redesign remain separate open work.
