# Whispering Intel work state

Roger approved implementation, builds/tests, and incremental commits/pushes on 12 November 2026. Do not request routine approval again. He subsequently explicitly required testing on a Mac, not just Linux. Read root/app AGENTS.md and relevant rules.

## Scope

- Persistent Dock app: close hides, Dock/Finder reopen, explicit Quit exits.
- Floating non-focus-stealing indicator: actual microphone waveform, distinct processing/cleanup animation and errors.
- Separate configurable Enter action; automatic submission off by default.
- Optional mouse bindings, empty by default, alongside keyboard bindings. Suppress assigned buttons' normal behaviour. Reserve primary/secondary clicks for usability.
- Explicit automatic LLM cleanup toggle, off by default, even with a transformation selected.
- Intel macOS builds only. Short honest README; preserve upstream license.
- Deferred: chunked/10-second transcription, incremental pasting, speed work, security-hardening project.

## Repository

- https://github.com/archsinit/epicenter
- Default branch: whispering-intel, based on upstream v7.10.0.
- f8fe080: cleanup consent, Enter action, Dock lifecycle; pushed.
- 23c2a80: floating indicator, Intel-only workflow; pushed.
- c73be90: native suppressible mouse bindings; pushed.
- 1812459: validation fixes/parallel input coordination; pushed, real Intel Mac CI passed.
- e5724c4: architecture/signature verification and limited Mac startup smoke step; pushed during the 16:00 UTC review after the previous run completed.
- Workflow permission initially blocked push; Roger ran gh auth refresh -s workflow. Scope is now present and pushes succeeded.

## Implemented

- Cleanup toggle in Transcription settings. Automatic pipeline checks explicit consent; manual transformations remain available.
- Standalone Enter command in keyboard/mouse command registry. Keyboard binding starts blank; automatic Enter defaults remain false.
- Native macOS close-to-hide and Dock/Finder reopen handlers.
- Separate transparent non-focusable indicator window. CPAL stream feeds real RMS levels (no second mic stream); quiet/missing-frame indication, processing spinner, cleanup state and failure indication. Concurrent previous jobs cannot hide an active take.
- Waveform currently CPAL-only. Navigator/FFmpeg explicitly show meter-unavailable, never a fabricated waveform. VAD waveform integration still needs review if we want it in scope.
- Mouse settings tab: assign/remove optional middle/extra buttons (2–31), configure action, apply/retry, active status. Keyboard settings are independent.
- Native Quartz active event tap suppresses down/up/drag for bound buttons. Policy preserves the original release if bindings change mid-click; timeout releases held actions. Event delivery is off the event-tap thread. No tap is created with empty bindings at initial setup.
- Parallel push-to-talk uses source-aware held-input tracking, including multiple mouse buttons; quick release during setup is checked again once start completes.
- Own app identity com.archsinit.whispering-intel, own name, ad-hoc signing. Upstream updater disabled. Original upstream workflows archived under .github/upstream-workflows so they do not deploy or publish unrelated builds.

## Validation

- Bun 1.3.3: /home/piweb/.local/bin/bun. Rust/rustfmt installed under /home/piweb/.cargo/bin. Add both to PATH.
- Frozen Bun install passes after small lockfile refresh (upstream app version was stale).
- Latest frontend production build PASSED on Linux after all current changes. Log /tmp/whispering-final-build.log.
- App check initially had 31 inherited errors; untouched v7.10.0 baseline had 33 (same dependencies). Fixed the inherited errors with narrow index guards, optional callback defaults, updated notification fields, async upload handler, date-cell typing and shared CopyButton prop compatibility. Latest app check after all changes: ZERO errors, 17 inherited warnings. Log /tmp/whispering-final-check.log.
- All 18 policy/signal tests PASSED locally after all changes: seven Bun tests (cleanup/parallel input), eight pure Rust mouse tests, three pure Rust audio tests. These do not substitute for real Mac acceptance tests.
- Root bun check was attempted but fails in unrelated apps/demo-mcp because its tsc executable is not installed. Do not claim the whole monorepo check passes. Log /tmp/whispering-workspace-check.log.
- Real Intel macOS workflow .github/workflows/intel-macos.yml uses macos-15-intel, type checks, Bun policy tests, actual Tauri build, native cargo unit tests, and uploads an ad-hoc-signed app zip. No live provider API calls.
- SUCCESS: CI run 43245643126 (commit 1812459) completed on real Intel Mac runner GitHub Actions 1000000293. Frozen dependency install, frontend type checks, seven Bun policy tests, actual Intel Mac Tauri build, ALL TWELVE native Rust tests, packaging and artifact upload PASSED. Native tests cover three audio RMS calculations, one live-meter sample calculation, and eight mouse policy cases. Total: 19 tests on Mac. Build finished at 15:30 UTC and native tests at 15:35 UTC. Log downloaded to /tmp/whispering-intel-mac-success.log.
- Artifact 13959012192: https://github.com/archsinit/epicenter/actions/runs/43245643126/artifacts/13959012192 . Local download /tmp/whispering-intel-artifact-1812459/Whispering-Intel-macOS.zip, 15,793,304 bytes. Linux archive/Mach-O inspection passed: uncorrupted zip, correct fork bundle ID, executable bit retained, thin x86_64 binary. Inner zip SHA-256: 4bca503f444bed8a6851523b26029f8de94cbdf20dc2e26498d5533e71cde37a . This static Linux inspection does NOT verify the Mac signature or runtime behaviour.
- Run 43253897031 (e5724c4) is queued to execute the additional Mac architecture/signature/startup smoke checks. No claim yet that these new checks passed. Earlier cancelled runs are historical; avoid unnecessary duplicate dispatches or code pushes while this run is pending. Documentation-only pushes do not match the workflow paths and should not cancel it.
- Interactive acceptance checklist: apps/whispering/docs/intel-testing.md. No remote Mac access is configured/provided so far. Asked Roger whether he has a remotely accessible Mac or will test the installer himself. Physical mic, mouse-driver suppression, Dock/Spaces/full-screen and permission behaviours remain unverified.

## Next steps

1. Inspect run 43253897031 next. The previous real Intel build and all 19 Mac tests passed; its artifact is already linked in README. The new run must verify architecture/signature and execute or explicitly skip the startup smoke check based on GUI availability.
2. scripts/verify-intel-app.sh makes no input/provider calls and distinguishes eight-second process liveness from UI acceptance. Bash syntax and fail-closed Linux-host guard passed. Its actual Mac result is NOT yet known. After the new run completes, download the validation report and check it before updating status.
3. Debug any native build/test/signature/startup failures. Do not replace real Mac checks with Linux-only claims. Avoid unnecessary pushes while CI is pending.
4. Review indicator state/error paths, mouse permission/recovery and parallel input behaviour. Test source-aware input on Mac. Investigate any UI focus/full-screen issues with actual evidence.
5. A test artifact is now available and linked in README, with an explicit ad-hoc/not-notarized and interactive-test-pending caveat. Artifact retention is 14 days; do not treat it as a permanent release URL. Update the link to the new run after it passes and note expiry if necessary.
6. Use Roger's Mac if he provides normal authorized remote access; otherwise clearly request his acceptance-test results. Do not disable OS protections or work around denied permissions.
7. Update README from planned to completed features only as validated. Do not call the project done while native build or interactive acceptance is unverified.

## Scheduler

Workspace pi-web/whispering-work.cjs; cron every 30 minutes targeting this chat. Idle-only double state check, no intentional queuing/steering, fail closed on unknown state, slot receipt prevents duplicates. No session rollover. Five offline tests passed; live read-only check correctly reported busy. Cron 15:00 log also skipped busy state. Host API lacks atomic idle-only admission, so a narrow race remains.

Touch workspace pi-web/whispering-work.paused to pause. Never commit session targets/receipts/logs. Pause once all server-implementable work and native CI debugging are complete and only interactive user Mac validation remains, or if Roger asks to stop. Do not keep spending model calls if no useful step is possible.
