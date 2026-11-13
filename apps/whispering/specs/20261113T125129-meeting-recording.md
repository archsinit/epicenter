# Meeting recording with local screenshot references

Status: implementation in progress. Local storage/scheduling plus native macOS audio/screenshot capture primitives are implemented. Controller, progressive audio writing/mixing, UI and provider integration remain pending; the feature is not yet usable or installed.

## Purpose

Teams transcripts remain the preferred source where available. Add a separate meeting mode to Whispering Intel for other online meetings and in-person meetings. Do not change everyday dictation behaviour.

## Agreed requirements

- Online meetings: capture Mac system audio and the selected microphone together, including when headphones are used.
- In-person meetings: microphone-only recording.
- Explicit Start/Stop, visible recording status and elapsed time.
- Save audio progressively to local storage rather than holding an entire meeting in memory.
- Capture a screenshot automatically every five minutes while recording, plus a **Capture screenshot now** button for important moments.
- Manual screenshots do not reset the automatic five-minute schedule.
- Save screenshots locally with timestamps and offsets relative to the meeting audio so Roger can refer back to them later.
- Screenshots must never be automatically sent to a transcription provider or an LLM. Audio-only transcription is separate from screenshot storage.
- Transcribe after recording, with an explicit action. No automatic clipboard, paste, Enter or LLM cleanup inherited from dictation.
- No continuous screen video, live transcription, slide analysis or automatic meeting summaries in this version.

## Proposed implementation

### Native capture and durable storage

Add a macOS-only meeting capture module and narrowly scoped Tauri commands. Evaluate ScreenCaptureKit for system audio and selected-display/window screenshots, with CPAL for the microphone if necessary. Confirm API availability on the actual Intel build target before choosing the implementation; do not claim an API supports the CI deployment target without checking. Do not substitute microphone loopback for real system audio.

Use an app-owned local `meetings/<id>/` directory with a manifest, audio files and `screenshots/`. The manifest records start time, capture mode, source selection, audio paths, screenshot paths, actual elapsed capture offsets and automatic/manual origin. Use atomic manifest replacement; keep recoverable audio fragments and an unfinished-session marker if the app crashes. Never delete an unfinished session automatically.

For dual-source audio, align timestamps and handle different sample rates/channel layouts. Store recoverable source audio progressively; produce a combined transcription input after stopping. Exclude the app's own audio from system capture where supported to avoid notification feedback. Do not buffer the whole meeting in memory.

Screenshot scheduling belongs to native session lifetime, not the visibility of a Svelte page. First scheduled capture is at five minutes; further captures follow the original five-minute boundaries. Serialize manual and automatic captures, use unique filenames, and do not replay missed captures in a burst after sleep. Report capture failures visibly without silently stopping the audio recording. Stop and finalize must cancel the timer and account for any in-flight capture before returning.

### Service → query → UI

Add a meeting service for typed native calls, a query/controller layer for lifecycle and state, and a dedicated Meetings page. Keep business logic out of Svelte components and follow existing Result/error patterns.

UI: meeting title, microphone choice, microphone-only/system-plus-microphone mode, screenshot source selection, Start/Stop, elapsed time, screenshot count, **Capture screenshot now**, and a clearly indicated screenshot pause control for sensitive content. Show saved meetings, local audio playback, timestamped screenshot references and an action to reveal the local folder.

Prevent simultaneous meeting capture and dictation from competing for audio state. Closing/hiding the main window must not stop a meeting; explicit Quit should stop/finalize or ask for confirmation. Do not change the menu-bar-only policy or unrelated overlay work.

### Audio-only transcription boundary

Reuse the existing transcription providers only through a meeting-specific, explicit post-recording action. Audit provider duration/file-size limits and split oversized inputs if needed without changing normal dictation. Only supported audio files may enter the transcription request; never pass a meeting directory, manifest or screenshot attachment. Save the resulting transcript alongside the meeting. Preserve all local recording assets if transcription fails.

Do not silently change the user's provider, credentials, model or dictation settings, and do not download models without approval.

### Permissions and privacy

Use normal macOS microphone and screen/system-audio capture permission prompts. Handle permission denial and source disappearance explicitly. Recording requires a deliberate Start action; no capture at launch or outside an active meeting. Explain participant-consent expectations. Local storage is not a claim of encryption or protection from the user's own backup/sync tools.

## Implementation checklist

- [x] Record Roger's agreed requirements in the fork.
- [x] Inspect the existing Tauri recorder, native dependencies and app architecture instructions.
- [x] Review this plan before implementation (Roger: “Please start”).
- [x] Verify native API/deployment-target compatibility and settle capture backend: AVFoundation microphone capture; ScreenCaptureKit system audio (13+) and screenshot manager (14+). Meeting mode requires macOS 14+. Verified against the actual Mac SDK headers and by compiling/linking the bridge with a 10.15 deployment target and guarded APIs. ScreenCaptureKit is weak-linked; launching on an older OS is not yet runtime-tested.
- [ ] Implement durable meeting storage and explicit lifecycle with crash recovery.
  - [x] Local manifest/asset storage, atomic manifest replacement, private file permissions, progressive audio-file handles and orphan-file preservation.
  - [ ] Wire live capture lifecycle, list/recover unfinished recordings and integrate Stop/Quit finalization.
- [ ] Implement real system-plus-microphone and microphone-only recording.
  - [x] Native microphone and system-audio input ownership, source selection, mono 48kHz PCM, host-clock timestamps, callback queue draining and explicit permission entry points.
  - [x] Safe worker-only Rust wrapper, bounded nonblocking audio transport and explicit packet-loss reporting.
  - [ ] Connect inputs to progressive WAV writing/alignment and verify actual microphone/system audio with consent.
- [ ] Implement automatic five-minute screenshots, manual capture and screenshot pause.
  - [x] Native scheduling policy: fixed cadence, manual capture independence, pause, overlap suppression, stop/drain and missed-slot handling.
  - [x] Native selected-display PNG screenshot backend (macOS 14+), with explicit permission checks, missing-display errors and no video recording.
  - [ ] Connect policy and backend to the native capture worker; verify actual screenshots with consent.
- [ ] Add meeting service, query/controller and Meetings UI with local reference browsing.
- [ ] Add explicit audio-only transcription and save transcript without dictation delivery side effects.
- [ ] Add focused tests: original timer cadence after manual capture, duplicate/collision handling, pause/resume, stop/in-flight capture, errors, recovery, path safety and screenshot exclusion from provider requests.
- [ ] Run focused frontend/native tests and app type checking; validate Intel Mac build compatibility.
- [ ] With Roger's permission, verify actual microphone/system audio with headphones, screenshots while hidden, permission failures, long-recording storage and Stop/Quit cleanup on the Mac. Do not record ambient meetings or sensitive screens without coordination.
- [ ] Record observed results and remaining limitations before offering an install.

## Review

First increment: `src-tauri/src/meeting/{mod,schedule,storage}.rs`, registered in the app's native library. This adds storage and scheduling building blocks only; importing the module does not start capture or expose a UI. Storage separates screenshots from registered audio, validates actual WAV data before offering a transcription file, rejects unsafe paths/symlink redirection, and preserves unfinished/orphaned assets without auto-resuming or uploading them. Recovery UI and reconstruction of unfinished audio remain pending.

Validation: `bash scripts/test-meeting-core.sh` compiles the actual production modules in an isolated temporary Rust harness using cached dependencies; all **23 tests pass**. The same tests are also included by the existing Mac CI `cargo test --lib` command through module registration, but no new Mac CI run has been performed in this increment. App type check passes with **zero errors / 17 inherited warnings** (`/tmp/whispering-meeting-app-check.log`). No full native Tauri build, macOS API verification, physical microphone/system audio/screenshot capture, provider request or installation was performed. No new production dependency or lockfile change. Intel scheduler remains paused; unrelated Android/overlay changes are untouched.

Second increment: added `capture_macos.{h,m}`, worker-owned Rust wrapper `macos.rs`, and bounded audio transport `audio.rs`. The bridge uses AVFoundation for microphone-only capture without requiring screen access, ScreenCaptureKit for true system audio with current-process audio excluded, and SCScreenshotManager for selected-display PNGs. Input PCM timestamps are converted from each session's synchronization clock into the common monotonic host clock. No native API starts capture at initialization. Build integration keeps the app deployment target unchanged and weak-links the newer optional framework.

Validation: **28 Rust core tests pass**, and the actual Rust macOS wrapper type-checks in the portable harness. On the actual Intel Mac, SDK **26.5**, the committed-style native fixture compiles/links with a **10.15 deployment target**, unguarded availability/incompatible pointer/deprecation warnings treated as errors, and passes synthetic PCM forwarding, timestamps, wrong-format rejection, main-thread rejection and stop queue-draining checks. The same synthetic fixture also passes with AddressSanitizer (leak detection disabled). These tests do not enumerate devices, open a microphone, capture a screen or request permissions; they are NOT real-capture acceptance. App check remains zero errors/17 inherited warnings. No app installation, settings/permission change or provider call occurred. `scripts/test-meeting-native.sh` is now included in Intel CI; full app build/startup validation is separate from this direct SDK check.

Next increment: implement the native worker with progressive WAV writing/alignment, connect scheduling and storage, then wire service → query → UI and audio-only transcription. Do not present the current capture primitives as a working meeting recorder.
