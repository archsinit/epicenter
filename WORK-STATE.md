# Whispering Intel work state

## Current status: direct testing on Roger's Intel Mac

Roger provided an existing SSH configuration and explicitly authorized direct Mac testing. Connection and GUI automation work. Read workspace `pi-web/whispering-mac-testing.md` for PRIVATE access details, exact installed app path, safe compound bundle-ID targeting, current settings and test progress. Do not publish the host/login details or raw UI dumps.

Installed and signature-verified exact test build `dd43322` on Roger's Intel macOS **26.7.1** without replacing or modifying his existing upstream Whispering app. Confirmed actual fork UI rendering, close-to-hide, persistent Dock icon, Dock-click reopening of the same process, explicit Quit of the fork only, and relaunch. UI settings confirmed automatic Enter off, separate Enter shortcut present/unassigned, cleanup toggle 0 -> 1 -> 0 restored off, and mouse bindings empty (draft Add/Remove tested without applying a binding).

The fork reports Microphone and Accessibility permissions are not granted. User must approve normal OS permissions; do not bypass controls or change security settings. No microphone recording/provider call or Enter injection was attempted; actual waveform, mouse suppression and transcription pipeline are not yet verified. No chat message sent, clipboard read/changed, or upstream app configuration changed.

The 30-minute scheduler is being resumed because useful direct-Mac testing is now possible. While permissions await user action, investigate upload-fixture/localhost-mock pipeline testing with the fork's copy/paste/Enter disabled, so no real provider credentials or physical capture are needed. See the private note's independent-next-steps section. Do not repeatedly call the model once no independent useful task remains.

Roger approved implementation, builds/tests and incremental commits/pushes on 12 November 2026; do not ask for routine approval again. Read root/app AGENTS.md and relevant rules.

## Repository and scope

- Fork: https://github.com/archsinit/epicenter
- Default branch: `whispering-intel`, based on upstream v7.10.0.
- Implemented: persistent Dock lifecycle (close hides, Dock/Finder reopens, explicit Quit exits); separate floating non-focusable indicator with real CPAL microphone levels and distinct processing/cleanup/error states; separate configurable Enter command; optional mouse bindings alongside keyboard bindings; explicit off-by-default automatic LLM cleanup toggle.
- Mouse defaults empty. Middle/extra buttons 2–31 can be assigned; primary/secondary clicks reserved. Active Quartz tap suppresses assigned down/up/drag, preserves original release across reassignment, and releases held actions on timeout. Keyboard and multiple mouse push-to-talk sources are coordinated.
- Actual waveform is CPAL-only. Navigator/FFmpeg show meter-unavailable instead of a fabricated waveform; VAD waveform integration is not included.
- Own app identity `com.archsinit.whispering-intel`, ad-hoc signing, Intel-only workflow, upstream updater disabled. Original unrelated workflows archived under `.github/upstream-workflows`.
- Deferred: chunked/10-second transcription, incremental pasting, speed work, security-hardening project.

## Verified evidence

**Current test build: `dd43322`**, [CI run 43259930929](https://github.com/archsinit/epicenter/actions/runs/43259930929).

Real Intel Mac host: macOS **15.7.9**, **x86_64**. Downloaded and read the actual validation report, not only Actions' green conclusion. Passed:

- Frozen dependency install and frontend types (zero errors, 17 inherited warnings).
- Seven Bun policy tests (cleanup consent and overlapping input sources).
- Twelve native Rust tests (three audio RMS, one meter sample, eight mouse policy).
- Three shell regressions: nonzero pipeline exit preserved through tee, successful pipeline report, invalid bundle fails with diagnostics retained.
- Actual Intel Tauri release build and app packaging.
- Intel-only Mach-O, correct fork identity and deep/strict macOS signature verification.
- **App process stayed alive for eight seconds in a Mac GUI login session.** No startup skip. Expected final `Terminated: 15` line is the smoke-test cleanup stopping its own child, not a startup crash.

This liveness check does NOT prove window/UI readiness or interactive features. Automated tests make no live provider API calls and do not capture a physical microphone.

### Current artifact

Use ONLY [artifact 13963969603](https://github.com/archsinit/epicenter/actions/runs/43259930929/artifacts/13963969603). It includes `Whispering-Intel-macOS.zip` and `Whispering-Intel-validation.txt`. Ad-hoc signed, not Developer ID signed or notarized. Follow normal macOS approval prompts; never disable Gatekeeper. Actions artifacts expire after 14 days; this is not a permanent release URL.

Local download: `/tmp/whispering-intel-artifact-dd43322/`.
Inner zip: 15,595,111 bytes; SHA-256 `cfdb321b582cc2e93405016560ca44dda9196ee8094ff6b89bd6e41a5f08ebb7`.
Linux archive inspection additionally confirmed zip integrity, app identity, x86_64 Mach-O and executable bit. This Linux check is separate from actual Mac signature/startup evidence.
Full run log: `/tmp/whispering-intel-mac-dd43322.log`.
Separate diagnostic report artifact: 13964457142.

### Earlier broken builds: do not offer

- 1812459 / run 43245643126: compilation and 19 tests passed, but no startup test existed. Artifact 13959012192 crashes on launch.
- e5724c4 / run 43253897031: architecture/signature passed, actual app startup ABORTED because updater initialization remained after its config was removed. CI falsely reported green because `script | tee` lacked pipefail. Artifact 13963395545 is broken too.
- `dd43322` fixed both problems by removing native updater registration, using explicit Bash/pipefail, adding pipeline regressions, and preserving failure diagnostics. Verified fresh Mac run now really passes.
- Earlier evidence remains under `/tmp/whispering-intel-artifact-e5724c4/Whispering-Intel-validation.txt`. File `/tmp/whispering-intel-mac-startup-success.log` is misleadingly named; it contains the historical failed startup report, not a success.

## Direct Mac tests and remaining acceptance

Checklist: `apps/whispering/docs/intel-testing.md`.

1. DONE: Installed exact corrected artifact and verified SHA/signature directly on Roger's Intel Mac. Normal Microphone/Accessibility permissions still need user approval.
2. DONE: Window close hides while process/Dock persist; actual Dock click reopens same PID; native menu Quit exits fork only; relaunch renders UI. Still test full-screen/Spaces/multiple monitors, typing focus and shortcut operation while hidden.
3. Configure intended microphone with default CPAL, dictate into a text field; confirm actual waveform responds to voice and settles on silence, processing animation appears, text arrives without automatic Enter.
4. Configure the independent Enter shortcut and optional mouse buttons. Confirm keyboard still works, bound side button does not trigger Back/Forward or drag behaviour, unassigned buttons remain normal, permission failures and push-to-talk release are safe.
5. Leave a transformation selected but cleanup OFF; verify no automatic cleanup run/provider call. Enable cleanup deliberately and verify selected transformation runs.
6. Record Mac model/macOS, mic, mouse/driver, tested commit and actual outcomes. Fix any reported failures with tests; do not mark unperformed checks passed.

Authorized direct Mac access is now available and initial real UI/Dock tests passed, in addition to CI. Continue tests yourself using the private workspace connection note. Remaining physical-input checks require the fork's normal user-approved Microphone/Accessibility permissions. Do not work around credentials, permissions or security restrictions.

## Local tooling and checks

- Bun 1.3.3: `/home/piweb/.local/bin/bun`; Rust/rustfmt: `/home/piweb/.cargo/bin`. Add both to PATH.
- App frontend check and production build passed locally. Eighteen pure policy/signal tests passed locally before Mac testing (Mac additionally tests the native meter integration).
- Root `bun check` was attempted but fails in unrelated `apps/demo-mcp` because its tsc executable is missing. Do not claim whole-monorepo checking passes. `/tmp/whispering-workspace-check.log`.
- Baseline v7.10.0 worktree `/tmp/whispering-intel-baseline` originally had 33 frontend type errors; narrow fixes made the fork's app check pass.
- Direct-Mac review currently changes documentation/status only; no production application code changed after the successful dd43322 Mac run. Keep all original upstream settings/processes untouched.

## Scheduler controls

Workspace `pi-web/whispering-work.cjs`, cron every 30 minutes, target this chat. Pause flag: `pi-web/whispering-work.paused`; it is being removed now that authorized direct Mac testing and independent fixture tests can proceed. `node pi-web/whispering-work.cjs --dry-run` reports Paused while the flag exists, without API/model calls.

The runner double-checks idle state and skips busy/unavailable/unknown states without intentional queueing/steering. Slot receipt prevents duplicate attempts; no session rollover. Host API lacks atomic idle-only admission, so a narrow race remains. Five offline scheduler tests previously passed. Never commit private session targets, receipts or logs.
