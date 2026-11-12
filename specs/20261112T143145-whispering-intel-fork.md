# Whispering Intel fork

Base: upstream v7.10.0 (a7c37a8c18c396764a1db7d6af017fc6502aeb32).

## Plan

- [x] Fork EpicenterHQ/epicenter to archsinit/epicenter.
- [x] Clone v7.10.0 locally and create the whispering-intel branch.
- [x] Roger approved implementation, builds/tests, and incremental commits/pushes on 12 November 2026.
- [x] Implement a separate configurable Enter command and optional suppressible mouse bindings that coexist with keyboard shortcuts. Interactive acceptance remains pending.
- [x] Add an explicit off-by-default automatic LLM cleanup toggle. Defer chunked transcription and speed optimisation.
- [x] Install an idle-only 30-minute continuation scheduler for this chat; five offline tests passed and read-only live check confirmed busy state.
- [x] Replace the root README with a short, honest fork description; preserve the upstream license and attribution.
- [ ] Keep the app running when its main window closes, retain its Dock icon, reopen from the Dock, and quit only on explicit Quit.
- [ ] Add a non-focus-stealing floating recording indicator with a waveform driven by real microphone levels and a distinct processing animation. Hide after completion; show clear failures.
- [x] Limit fork release builds to Intel macOS. Use the fork's identity and ad-hoc signing; disable upstream updates.
- [ ] Validate types/builds and test recording, silence, transcription, errors, Dock reopening, close/hide, and explicit quit on Intel macOS. Builds/tests are pre-approved.
- [ ] Update README to describe completed changes only after implementation and validation.
- [x] Commit/push incremental changes and make whispering-intel the default branch. Commits/pushes are pre-approved.
- [ ] Verify the Intel-only binary architecture, fork identity and signature on Mac, and run a clearly limited app startup smoke test if the runner has a GUI login session.

## README draft (after implementation)

# Whispering for Intel Mac

A fork of [Whispering](https://github.com/EpicenterHQ/epicenter/tree/v7.10.0/apps/whispering), based on 7.10.0, with two changes:

- Stays in the Dock and keeps running when you close the window.
- A floating recording indicator with a live waveform and processing animation.

Built only for Intel macOS. We may add other platforms if there are requests.

[AGPL-3.0](LICENSE).

## Review

Implementation approved; progress and blockers are tracked in WORK-STATE.md. Scope expanded to shortcuts and explicit cleanup control; the final README must reflect this instead of claiming only two changes. The fork's default branch is whispering-intel. On 12 November 2026, CI run 43245643126 successfully built the app on GitHub's macos-15-intel runner and passed frontend types/Bun tests; native unit tests were still running at review time. The added bundle/startup validation step is not yet run. Interactive macOS behaviour remains unverified; this development server is Linux.
