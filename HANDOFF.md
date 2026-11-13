# Whispering Intel session handoff

## Start here

Continue the Intel Mac fork, not the separate Android project. Read `AGENTS.md`, `WORK-STATE.md` and `specs/20261112T143145-whispering-intel-fork.md`. For authorized Mac access and safe UI targeting, read workspace-local `pi-web/whispering-mac-testing.md`; never commit that private note or credentials.

Roger has approved implementation, builds, tests and incremental commits/pushes. Do not repeatedly ask for routine approval. He specifically wants testing limited to OUR changes, not broad upstream functionality or model quality. Do not declare the project fully tested while the remaining real-Mac checks are unperformed.

## Latest conversation

IMPORTANT requirement correction: Roger meant the TOP menu bar/tray, not the Dock. He now explicitly wants NO Dock item and menu-bar-only access. This supersedes persistent-Dock requirements and the earlier missing-Dock report. Implemented native menu-bar creation before switching macOS to Accessory policy; Show Window/Settings/Quit target main window, frontend only updates native recording status. Source also removes telemetry. Existing installed dd43322 remains unchanged. Local 22 focused tests and app check (zero errors/17 inherited warnings) pass; two new native icon tests await Mac CI. New build and actual Mac absence-of-Dock/menu actions/close-to-hide/shortcut checks remain pending. See specs/20261113T050000-menu-bar-only.md. Overlay appearance is still a separate design discussion.

Dock retested at Roger's request on 13 November using existing dd43322: Regular activation policy and left-hand Dock item present; actual close hid the main window while preserving PID/icon; actual Dock click reopened and focused the same PID. Native Quit exited; normal relaunch restored main window and Dock item. No missing-icon failure reproduced, no Dock code or macOS preference changes made. Earlier user report remains open pending clarification: running versus permanently pinned icon, intermittent/Space-specific disappearance. See WORK-STATE.md for evidence. Telemetry removal remains source-only; unchanged installed build was not replaced.

Latest update (13 November): Roger reports actual everyday OpenAI usage works well, including mouse buttons. He reports the Dock item is NOT visible, even after closing, so reopen Dock persistence as a reported bug despite the earlier observed pass. He dislikes the overlay appearance and wants to design improvements together; no new design has been agreed. Prioritize these over repeating broad acceptance tests.

He also requested telemetry removal. Implemented in the working tree: frontend event calls/services/query/analytics settings UI removed; native Aptabase plugin/lifecycle reporting/capability and SDK dependencies removed; JS/Rust lockfiles refreshed. Three focused source guards added to Intel CI; 19 local tests pass, app type check zero errors/17 inherited warnings, diff whitespace check passes. See specs/20261113T043000-remove-whispering-telemetry.md. No new native Mac build/install or runtime network audit yet. His installed dd43322 build still has the old code; do not claim removal is deployed or overwrite his working OpenAI configuration.

The following earlier handoff context is historical where superseded by the update above.

Roger is now at his computer and wants to test the remaining changes for real. The agent opened the installed fork and asked him to enable Microphone and Accessibility for **Whispering Intel**, not upstream Whispering. He has NOT confirmed those permissions were enabled. Check the current state rather than assuming either approval or denial.

He is considering this model split:
- Everyday dictation: OpenAI `whisper-1` API.
- Meeting recordings: local Whisper Large v3 Turbo, accepting slower processing on Intel.
- Teams transcripts remain an option for meetings already transcribed by Teams.

This is a discussed preference, NOT verified app configuration or a benchmark. No local model download, provider-key configuration or real API call was performed in this conversation. Do not claim Turbo speed/accuracy was measured on his Mac. Do not add model-quality benchmarking to the fork-change test scope.

## Repository and installed build

- Local folder: `/home/piweb/workspace/projects/whispering-intel`.
- GitHub: https://github.com/archsinit/epicenter
- Branch/default: `whispering-intel`, based on upstream v7.10.0.
- Production app source last changed at `dd43322`; additional focused tests are in `dc87847`. Later commits update documentation/test infrastructure, not the installed app.
- Installed on Roger's Intel macOS 26.7.1 under `$HOME/Applications/Whispering-Intel-Test-dd43322/Whispering Intel.app`.
- Bundle ID: `com.archsinit.whispering-intel`.
- Keep its bundle ID/install path/signature stable; do not reinstall an unchanged ad-hoc build merely to run tests.
- Existing upstream `/Applications/Whispering.app` must remain untouched. Both processes are named `whispering`; target the fork using the full bundle-ID predicate, not process name. See private note for the exact tested commands.
- Separate untracked `apps/whispering-android/` and mobile specs belong to another task. Do not stage, overwrite, delete or revert them.

## Implemented changes

1. Persistent Dock app: closing hides; Dock/Finder reopens; explicit Quit exits.
2. Floating non-focus-stealing indicator: real CPAL microphone waveform, processing/cleanup states and error indication. Navigator/FFmpeg report meter unavailable rather than fake levels; live waveform is CPAL-only.
3. Separate configurable Enter action. Automatic Enter remains off by default; Roger always reviews text before submitting.
4. Optional assignable middle/extra mouse buttons, blank by default, alongside keyboard shortcuts. Bound buttons suppress normal down/up/drag behaviour. Primary/secondary clicks reserved. Overlapping keyboard/mouse push-to-talk sources are coordinated.
5. Explicit automatic LLM cleanup toggle, off by default, even with a transformation selected. Manual transformations still work.
6. Intel-only builds, own app identity, manual updates; upstream updater removed.

Deferred: 10-second/chunked transcription, incremental pasting and speed optimisation. README should remain short and factual, identifying the fork, actual changes and Intel-only scope.

## Verified, with evidence

- Intel Mac CI run https://github.com/archsinit/epicenter/actions/runs/43316942968 passed **28 unit tests** (16 Bun, 12 Rust), three pipeline checks, app types/build, deep-strict signature verification, eight-second startup liveness in a GUI session and packaging. Actual report was downloaded/read; startup was not skipped. This does not prove visible animation, focus or physical inputs.
- Directly on Roger's Mac: actual UI renders, window close hides while process/Dock persist, actual Dock icon reopens the same PID, Quit exits only the fork, and relaunch works.
- Actual UI: automatic Enter off; separate Enter shortcut exists/unassigned; mouse defaults empty; cleanup switch round-trip 0 -> 1 -> 0, restored off.
- Actual Mac app + localhost canned provider: a selected fixture with cleanup OFF sent transcription only; cleanup ON sent transcription plus one completion request and saved the expected cleaned result. All settings, dummy keys, fixture records and test server were cleaned up. No real provider key/call, ambient recording, clipboard change or chat submission.
- Current app artifact: https://github.com/archsinit/epicenter/actions/runs/43259930929/artifacts/13963969603 . Already installed; no need to download again. Actions artifact retention is 14 days.
- Earlier artifacts from 1812459/e5724c4 had an updater startup crash. Do not offer them. Fixed in dd43322; startup CI failure propagation was fixed too.

## Next real-Mac test sequence: only the fork changes

1. Confirm user-approved Microphone/Accessibility permissions. Do not manipulate TCC, trust, Gatekeeper or permission settings behind the user. Request Input Monitoring normally only if the actual mouse test requires it.
2. Confirm CPAL recording method and intended mic. Ask Roger to speak/pause while checking that the actual waveform follows voice/silence. No provider key is needed to check recording levels; cancel the take if transcription is not configured.
3. Check processing-indicator visibility, phase changes, disappearance and preservation of focus. Use the existing loopback fixture server if needed, with copy/paste/Enter disabled and then restored. Do not send text to a real chat.
4. Assign an Enter shortcut and test it in an isolated test document, not a chat composer. Confirm dictation does not auto-submit.
5. Let Roger choose mouse bindings, or agree a temporary test binding; verify keyboard and mouse coexist and that Back/Forward/drag does not also fire. Restore temporary bindings afterward; normal default is blank.
6. Fix failures with narrow regression tests. Record actual passes/failures and remaining limits; do not replace observed GUI/device evidence with mock results.

Cleanup toggle and Dock lifecycle already have actual-app evidence. Do not repeat broad transcription tests or upstream model benchmarking unnecessarily.

## Signing and security

Roger wants one persistent signature to avoid permission resets. No valid signing identity was installed when checked. Upstream is signed with its maintainer's Developer ID; we cannot reuse it without their private key. The fork is ad-hoc signed, not notarized.

Explained Apple Developer Program is US$99/year, optional for personal builds. A free local self-signed identity was mentioned as an investigation option, NOT implemented and NOT proven to preserve permissions. Roger has not approved certificate/keychain/trust changes. Do not silently create/install certificates or promise stable TCC permission retention.

RustDesk was opened at Roger's request. He subsequently connected using his phone/SSH but video was too slow. No password was supplied to this agent. Do not change, reset or remove his remote-access credentials during project work. He is now at the computer, so coordinate any desktop-driving steps with him.

## Scheduler and local tooling

The Intel 30-minute continuation scheduler is **paused** using workspace `pi-web/whispering-work.paused`. Leave it paused during this session handoff. It targets the OLD chat; do not blindly unpause it in a new session. If Roger wants recurring continuation in the new session, verify/repoint its local target first; never commit session IDs, targets or logs. Android work is separate.

Bun: `/home/piweb/.local/bin/bun`; Rust: `/home/piweb/.cargo/bin`. Add these to PATH. Focused tests:

```
bun test apps/whispering/src/lib/query/indicator.test.ts apps/whispering/src/lib/query/cleanup-policy.test.ts apps/whispering/src/lib/services/hold-gate.test.ts
bun run --cwd apps/whispering check
```

App checks pass with zero errors/17 inherited warnings. Root monorepo `bun check` has an unrelated demo-mcp missing-tsc failure; do not claim it passes or expand the task to fix unrelated packages.

Private Mac test instructions and restoration details: workspace `pi-web/whispering-mac-testing.md`. The mock provider/fixture instructions are in `scripts/integration/README.md`. No production code, Mac configuration, tests or builds were changed/run just to create this handoff.
