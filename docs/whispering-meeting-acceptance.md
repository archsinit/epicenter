# Whispering Intel meeting recording: acceptance handoff

Implementation code checkpoint: `772d07f9ee20e9ea0cec1300c75ddeae604a806a`.

[Actual Intel validation run 43531539995](https://github.com/archsinit/epicenter/actions/runs/43531539995) passed: 160 native Rust tests, 170 focused Bun tests, synthetic capture bridge, app type check (zero errors/17 inherited warnings), release build, Intel-only fork identity/signature and eight-second GUI-session process survival on macOS 15.7.9 x86_64. Downloaded report/logs: /tmp/whispering-meeting-772d07f/ on the development workspace.

Linux checks: 142 portable Rust tests, 170 focused Bun tests, type check and production/static frontend build pass. This is not evidence of rendered UI readiness, physical capture, permissions, ASR quality or model performance.

Implementation includes native mic/system capture and progressive aligned raw/mixed WAVs, five-minute screenshots plus independent manual/pause controls, capture ownership/restoration/recovery fences, native menu-bar status, Stop/Quit coordination, paginated local library, user-started playback, copy-on-recovery UI and explicit audio-only transcription with durable per-chunk local text/resume. Existing dictation/settings/keys/models and Android/overlay work were not changed.

## Before any acceptance activity

Coordinate with Roger first. Do not install/open the candidate unattended, start live mic/screen capture, grant/request OS permissions, interact with native recovery/Quit dialogs, send real provider audio or download models automatically. No candidate was installed during development.

Use a non-sensitive test screen and deliberately chosen test audio, not an ambient meeting. Agree on participant consent and what may be recorded. Preserve current settings, model paths and credentials; do not reset TCC, bypass a denial, change providers or clear unknown-process guards to make a test pass. Opening/hiding a window is not a full Quit/relaunch.

## Coordinated checks

1. **Ordinary dictation regression:** confirm selected recorder/provider/model/key/settings unchanged. Exercise normal hotkey/VAD/Stop behavior and existing menu-bar-only/overlay operation.
2. **Microphone-only:** explicit source load and normal microphone permission action, source selection and consent; Start, native elapsed/status, Stop, local playback and retained raw/mixed files. No screen permission should be needed without screenshots.
3. **Online/headphones:** selected microphone plus real system audio, with headphones. Use distinguishable test sounds/voices to verify both tracks, alignment, rates/volume and clipping rather than assuming a nonempty WAV proves correct capture.
4. **Screenshots:** keep recording beyond 5:00 and 10:00 while the main window is hidden. Take a manual capture between boundaries and verify it does not shift cadence. Pause/resume, missing display/denied access and Stop during a pending capture; inspect local timestamps/offsets and no late screenshots after Stop.
5. **Ownership and status:** attempts to overlap dictation/VAD and meeting recording are refused, not silently cancelled/reconfigured. Hiding/unmounting does not stop capture. Native menu-bar mark remains accurate through Stop/mixing/unknown cleanup.
6. **Stop/Quit:** verify source closure/finalized audio and worker exit. Exercise normal Quit and incomplete-finalization confirmation. A timeout is not proof of closure; Keep-open while cleanup is pending keeps admission fenced until full Quit/relaunch. Do not force a real stuck driver just to test this; synthetic stall tests already cover control flow.
7. **Recovery:** use a deliberately interrupted, non-sensitive test session only after verifying its prior recorder stopped. Review counts/space, cancel, re-review, explicitly confirm a copy. Check distinct derived ID/provenance, interrupted label, source offsets/playback and original audio/screenshots byte preservation. Reload/lost acknowledgement must not duplicate the same recovery token. Unknown legacy FFmpeg metadata must not signal a saved PID or silently clear quarantine.
8. **Audio-only transcription:** explicitly authorize an already configured local model/provider. Cloud/self-hosted use requires Roger's separate agreement to sending the test audio and any charges. Verify per-minute text/checkpoints, pause/resume, credential/provider failure, local-save error reporting and settings pinning. No automatic clipboard/paste/Enter/LLM cleanup; no screenshot/directory/manifest body in provider requests. Missing models fail without downloading.
9. **Library/playback/storage:** use first/next/previous pages, corrupt/missing metadata entries and Finder reveal. Continuous playback starts only on user action, stops on selection/unmount and handles browser play restrictions. Inspect raw plus mixed storage growth (about 1 GB/hour dual source, 0.7 GB/hour mic-only, excluding screenshots), disk errors and long-duration behavior.

## Declared limits, not hidden successes

- Meeting capture requires macOS 14+. Older-OS ordinary-dictation runtime compatibility remains a separate unperformed device check.
- Recovery supports native mono 48kHz PCM16 minute files, valid source metadata/sidecars, at most 24 hours/10,000 audio-folder entries. Ambiguous/missing/non-native/corrupt metadata fails closed for manual review; originals are retained.
- Transcription accepts finalized native mixed chunks only, <=60 seconds/5,760,044 bytes per request. Custom endpoint/proxy/rate limits may be smaller and fail normally; no automatic endpoint/credential/compression workaround.
- Provider adapter options are pinned; keys are not persisted. Local models must already exist. See [boundary and adapter audit](../apps/whispering/specs/20261114T000000-meeting-transcription-boundary.md).
- Text checkpoints are private JSON alongside the meeting (1 MiB/chunk, 16 MiB/document). Unsaved provider output is held in memory for explicit save retry; durability is unconfirmed until acknowledged. Closing/restarting during an unacknowledged request can require retranscribing a minute and incur another charge. No exactly-once provider billing claim.
- Ready recovery review is metadata-only and holds no capture lease. New review/full restart invalidates old tokens. Running recovery remains owned through completion/Quit; no blind retry after a lost acknowledgement.
- Local storage is not encryption and follows the user's own backup/sync policies.

Automatic half-hour development should remain paused while only these coordinated acceptance checks remain. Resume it for any concrete defect found; do not reinterpret the pause as permission for unattended capture or installation.
