# Whispering Intel fork

Base: upstream v7.10.0 (a7c37a8c18c396764a1db7d6af017fc6502aeb32).

Scope clarification from Roger: validate only the fork changes, not broad upstream behaviour or real-provider speech-recognition quality.

## Plan

- [x] Fork EpicenterHQ/epicenter to archsinit/epicenter.
- [x] Clone v7.10.0 locally and create the whispering-intel branch.
- [x] Roger approved implementation, builds/tests, and incremental commits/pushes on 12 November 2026.
- [x] Implement a separate configurable Enter command and optional suppressible mouse bindings that coexist with keyboard shortcuts. Interactive acceptance remains pending.
- [x] Add an explicit off-by-default automatic LLM cleanup toggle. Verified actual Mac app request routing with selected fixture: OFF sends only transcription; ON sends transcription plus one cleanup call to localhost. Defer chunked transcription and speed optimisation.
- [x] Install an idle-only 30-minute continuation scheduler for this chat; five offline tests passed and read-only live check confirmed busy state.
- [x] Replace the root README with a short, honest fork description; preserve the upstream license and attribution.
- [x] Keep the app running when its main window closes, retain its Dock icon, reopen from the Dock, and quit only on explicit Quit. Verified directly on Roger's Intel macOS 26.7.1 using the fork's actual window, Dock icon and Quit menu; upstream app left running.
- [ ] Add a non-focus-stealing floating recording indicator with a waveform driven by real microphone levels and a distinct processing animation. Hide after completion; show clear failures.
- [x] Limit fork release builds to Intel macOS. Use the fork's identity and ad-hoc signing; disable upstream updates.
- [ ] Complete Intel Mac acceptance: physical recording/silence, input permissions, actual mouse suppression and Enter injection remain pending. Dock lifecycle and localhost fixture transcription/cleanup routing have passed; builds/tests are pre-approved.
- [x] Add focused indicator-controller regression coverage: nine local tests passed, combined fork policy suite 16 passed; native IPC is mocked. The policy step also passed on Intel Mac CI run 43316942968; the app build passed, while later native tests/startup/report steps remain pending.
- [ ] Verify processing indicator visibility/phase/focus during a delayed localhost fixture upload when the desktop is available; no microphone permission or real API key required. Do not interrupt Roger's active remote-access session.
- [ ] Agree a persistent signing strategy; no signing identity is currently installed and no certificate/keychain changes have been approved.
- [ ] Update README to describe completed changes only after implementation and validation.
- [x] Commit/push incremental changes and make whispering-intel the default branch. Commits/pushes are pre-approved.
- [x] Verify the Intel-only binary architecture, fork identity and signature on Mac, and pass a clearly limited app startup smoke test. Fixed updater initialization and masked CI failures; run 43259930929 passed on Intel macOS 15.7.9, including eight-second startup liveness.

## README draft (after implementation)

# Whispering for Intel Mac

A fork of [Whispering](https://github.com/EpicenterHQ/epicenter/tree/v7.10.0/apps/whispering), based on 7.10.0, with two changes:

- Stays in the Dock and keeps running when you close the window.
- A floating recording indicator with a live waveform and processing animation.

Built only for Intel macOS. We may add other platforms if there are requests.

[AGPL-3.0](LICENSE).

## Review

Implementation approved; progress and blockers are tracked in WORK-STATE.md. Scope expanded to shortcuts and explicit cleanup control; the final README must reflect this instead of claiming only two changes. The fork's default branch is whispering-intel. On 12 November 2026, CI run 43245643126 successfully built the app on GitHub's macos-15-intel runner, passed frontend types and all 19 tests (seven Bun and twelve native Rust), and uploaded the test app. The downloaded archive passed integrity, fork identity, executable-mode and x86_64 Mach-O inspection on Linux. Run 43253897031 passed architecture/identity/signature checks on macOS 15.7.9 x86_64, but its actual startup report found an updater-initialization panic. The job's green status was false: piping verification into tee masked its failure. Commit dd43322 removed native updater initialization and enforced Bash/pipefail, with pipeline regression checks and always-preserved diagnostics. Run 43259930929 then passed all 19 unit tests, three pipeline checks, Mac binary/signature checks and eight-second GUI-session startup liveness on macOS 15.7.9 x86_64. The full report was downloaded and read; no startup skip occurred. README links only the corrected test build. Roger then authorized his existing SSH host configuration. Direct tests on his Intel macOS 26.7.1 confirmed actual UI rendering, close-to-hide, persistent Dock icon, actual Dock-click reopening of the same PID, explicit Quit and relaunch, automatic Enter off, separate Enter action present/unassigned, cleanup toggle round-trip restored off, and empty mouse bindings. Fork Microphone/Accessibility permissions remain ungranted; no security changes or provider calls were made. Scheduled continuation resumes for further safe direct-Mac/fixture testing. The actual Mac upload pipeline was then exercised with a synthetic silent WAV and a loopback-only canned provider. Three transcription requests and one cleanup request were observed: selected transformation with cleanup OFF bypassed completion, and cleanup ON produced the expected completed cleanup run. Five fixture-server tests passed on Linux and the Mac. Temporary settings, credentials, fixture records and server were cleaned up; no physical recording, real provider calls, chat messages or clipboard changes occurred. Full physical-input/real-provider acceptance remains pending; private connection and targeting notes stay outside the public repository.
