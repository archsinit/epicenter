# Whispering Intel overlay design → implementation handoff

Living handoff for Roger's approved design references, implementation and test evidence. The latest installation summary below supersedes the older initial-handoff snapshots retained later in this file.

**Current installed runtime: 5e5a282** (recording travel + GPT-Transcribe), installed after CI 43397163550 passed 51 Bun +16 Rust tests, hidden native travel tests, types, Intel build, signature and startup checks (actual report/logs read). Installed executable SHA matches the artifact; prior bundle backed up at the stable fork installation path. Actual Mac AX confirms visible 180×120 companion, WebGL renderer ready, Ready status, native tray and preserved hidden main window; Accessory activation policy is 1. Provider/model selections, settings, bindings, permissions and upstream app were not changed. Native visual/performance and packaged drag/focus acceptance remain open; no synthetic input was used for installation.

## Recording travel revision (installed as 5e5a282)

Roger requested bottom-center recording travel and asked for a checkpoint before changing it. Checkpoint `a63f9ae` is pushed and retains the prior overlay motion while adding GPT-Transcribe without changing selection/defaults; request tests verify plural `languages` only for the new model using SDK 5.23.2 and a synthetic fetch. No real provider call was made.

Travel implementation captures the actual dragged home frame, glides the real NSPanel to its current display's horizontal center with the waveform core 36pt above the usable bottom, then returns to the saved home on recording exit. During return the ribbon expands to gas with no face or processing swirl; swirl begins only after arrival if a real job is still pending. New recordings interrupt return without overwriting home. Automatic travel is click-through and obeys system Reduce Motion. No Dock icon, input-binding, provider, permission or recording/pipeline changes. Local 51 Bun tests, app types, rendered browser sequencing and a hidden native AppKit host test pass; Intel CI and installation evidence are recorded above. See `apps/whispering/specs/20261113T084500-recording-travel.md`. Native visual/packaged focus acceptance remains unverified. Checkpoint `a63f9ae` remains the preferred source rollback if travel is rejected: it preserves GPT-Transcribe support with the prior overlay motion.

## For the design session to complete

### Approved visual direction from Roger's design conversation

Roger approved a four-form Gastly reference sheet, saying “Now what? I'm satisfied.” The final original PNG was uploaded in that conversation after Roger generated it on OpenAI's website, and has now been recovered losslessly from the user attachment. Approved asset: `apps/whispering/static/companion/approved-reference.png` (1536×1024 RGBA). Workspace preserved original: `/home/piweb/workspace/gastly-companion/approved-reference-original.png`. Alpha was checked: 531,579 fully transparent and 1,041,285 partially transparent pixels. See `apps/whispering/static/companion/README.md` and `scripts/gastly-assets.cjs` for derived textures.

- **Style:** halfway between cartoon and realistic; dimensional charcoal spherical head, expressive white eyes, small fangs, purple/lilac volumetric smoke. Preserve the approved character, not a replacement orb or generic ghost.
- **Proportions:** Roger requested the head and facial features 30% smaller in diameter than the earlier semi-realistic reference, with the gas cloud staying approximately the same overall size. The approved final sheet embodies that requested direction; do not reduce it another 30%.
- **Gas:** dense purple near the center, delicate irregular wisps fading smoothly to full transparency outward. No hard enclosing cloud outline. Verify actual PNG alpha: a dark-looking preview alone does not establish transparency. Test against light and dark backgrounds.
- **Four reference forms:** top left idle Gastly; top right horizontal purple audio waveform surrounded by gas; bottom left compact purple processing vortex; bottom right happy Gastly with small celebratory sparkles.
- **Transformation intent:** Roger wants the same character to transform between forms, not unrelated images swapping abruptly. Preserve the aura/palette through transitions. Exact technique, durations and performance budget remain to be prototyped; approval of a still image does not establish animation quality.
- **Earlier alternatives, not the final design:** `/home/piweb/workspace/gastly-companion/reference-sheet.svg`, `/home/piweb/workspace/gastly-companion/reference-sheet-ai.png` (cartoon), `/home/piweb/workspace/gastly-companion/reference-sheet-semi-realistic.png` (earlier CGI-style reference). Preserve these; Roger explicitly wants the ability to return to the cartoon version. They are not substitutes for the final uploaded sheet.

### Latest real-Mac feedback: supersedes conflicting prototype choices below

- First prototype commit `8173f6b` was built by Intel CI run `43369514361`: 25 Bun and 15 Rust tests passed, signature/startup checks passed; actual validation report was read. It was installed at the existing fork path with a backup of the previous app bundle. Settings/provider credentials were not changed.
- Native inspection confirmed a visible 360×240 idle overlay, Accessory activation policy, no matching Dock item, accessible “Ready” semantics, and persistence after closing the main window. Native screen-capture access was unavailable; no desktop screenshots or permission changes were made. Browser screenshots and a simulated-audio motion GIF were inspected separately.
- **Roger rejects the first animation quality.** He clarified that gas must swirl/evolve like real gas: wisps curling, stretching, breaking apart and fading while new wisps form, NOT one unchanged picture rotating together. Applies to idle, recording and processing. He approved moving on with this revision. The image remains the appearance reference, not the animation mechanism. Revised source uses domain-warped procedural volumetric noise and a projected spherical face; this is not a physical fluid simulation.
- **Size:** make the companion 50% smaller than the installed first prototype, including corresponding native bounds/placement changes. Keep padding sufficient to avoid clipping and do not add a large invisible input-blocking rectangle.
- **Edges:** remove black lines/rectangular edges everywhere. Native shadow is currently enabled and should be disabled; also inspect texture alpha/masks and any clipped animated bounds. Do not claim the exact source of every artifact is verified without observing the revision on Mac.
- **Face–gas gap:** remove the exposed cutout behind the moving head; smoke must connect naturally to/overlap the head without a transparent moat.
- **Head direction:** actual sideways/upward/downward turning is wanted, not merely translating/rolling a flat face. Preserve likeness; prototype a genuinely dimensional or multiview treatment. Do not describe flat CSS tilt as a completed facial rig.
- **Dragging APPROVED:** Roger explicitly wants to drag the overlay around. This supersedes the blanket click-through-only restriction for the character itself. Preserve non-focus-stealing operation and click-through outside the intended drag target. Primary-button drag must not affect configured extra-button shortcuts. Exact drag mechanism/persistence remains to be implemented/tested.
- **Normal state text:** Roger does not want visible “Listening”, “Processing” or other routine status labels. Keep accessible state semantics. Failure/meter-unavailable communication still needs a clear, unobtrusive treatment; do not silently represent unavailable input as voice or success.
- **Mouse shortcuts:** Roger initially reported a failure, then explicitly retested and said they work fine. Treat them as working by his latest report; do not change bindings, provider settings, OS permissions or event-tap code based on the earlier report. An earlier AX check reported an Accessibility warning, but its materialized process reference could be ambiguous with the identically named upstream process; do not treat it as reliable fork-specific evidence. Roger's direct retest is the current mouse-status evidence. No permissions or bindings were changed.

### Latest refinements approved by Roger

- Recording waveform must be the gas silhouette itself, driven by real microphone history, NOT separate white bars. Remove the bar overlay.
- Gas should originate at the shared face/waveform core and visibly spread outward, rather than float as a disconnected backdrop. Revised transport is source-relative radial advection with independently changing turbulence and outward fading.
- Keep the entering idle-to-processing morph; settle processing spin BEFORE revealing the returning face. Preserve the last real ribbon shape through recording-to-transcribing instead of clearing it immediately.
- Remove the pale crown arc Roger identified in his screenshot. Retouch only the derived idle-head outer upper edge, trim stray gas from its alpha mask, project the actual head radius, remove extra shader rim lighting and feather the sphere silhouette. Preserve original approved artwork and the broad forehead highlight.
- Fix menu-bar light/dark adaptation by restoring native template mode after each icon replacement. Verified dependency behavior: tray-icon's macOS `set_icon` creates a fresh non-template image even when startup used template mode. Do not hardcode a white icon or change OS appearance.
- These refinements are tracked in `apps/whispering/specs/20261113T063038-gas-waveform-and-emission.md`. Local 30-test suite and rendered browser waveform/transition checks pass; actual new-build installation/native validation still pending at this update.

### Natural cloud clarification and current installation

Roger's cyan Gastly illustration is a contour/motion reference only, saved at workspace `/home/piweb/workspace/gastly-companion/silhouette-reference-cyan.png`; keep the approved purple semi-realistic style. He wants small flutter AND larger independent billows, a noncircular outline with longer plumes in some directions, and wisps lagging the head. He explicitly emphasized natural translucent gas, not cartoon edges, spikes or triangular fans.

See `apps/whispering/specs/20261113T071100-irregular-billows-and-trails.md` for the local soft-volume revision and its browser preview. Local checks: 31 Bun tests, 0 type errors/17 inherited warnings, real rendered-pixel checks for uneven directional growth, trailing emitter, no bars and true meter levels. **The latest billow revision is now installed as 13c968a** (see current summary at top). The prior dcaefc2 build had a guarded desktop input test abort/refuse mid-test, so packaged drag/focus/click-through remains NOT a recorded pass. Do not drive Roger's desktop/mouse while he is actively testing. Native screen capture remains unavailable; no permission/settings/provider/binding changes were made.

### Latest feedback after installation

Roger says the billow appearance is substantially better. He requests the processing/transcribing cloud rotate in the opposite direction while retaining the existing spiral shape. The correction is installed as 02a721b; source, 32-test validation and installation evidence are tracked in `apps/whispering/specs/20261113T075523-processing-swirl-direction.md`.

Roger also reports he is using “GPT-Mini 04” rather than Whisper 1; likely `gpt-4o-mini-transcribe`, but the exact configured model was not read or independently verified. This is a quality/pricing question, not permission to change models/settings. Current official docs list approximate mini-transcribe pricing $0.003/min vs Whisper $0.006/min and describe mini accuracy improvements over original Whisper. No user-voice benchmark was performed. Both models are listed for retirement on 26 February 2027, with `gpt-transcribe`/`gpt-live-transcribe` as replacements; see official OpenAI pricing/model/deprecations docs. Do not assume paying more for Whisper improves his dictation. Roger asked why replacements are missing from the three-item dropdown: the list is hard-coded in `src/lib/services/transcription/cloud/openai.ts`. Official docs confirm gpt-transcribe supports /v1/audio/transcriptions at $0.0045/min ($0.27/hour); gpt-live-transcribe uses Realtime transcription, not that file endpoint, at $0.017/min ($1.02/hour). File-model request-option compatibility still needs checking before adding it. Neither catalog nor selected model was changed by the swirl installation.

### Revision implementation/testing

See `apps/whispering/specs/20261113T053006-gastly-procedural-revision.md`. Revised source is 180×120 logical pixels, no native shadow, no face-cutout smoke texture, no visible routine labels, primary-button-only character dragging with click-through margins, and a real yaw/pitch sphere projection for the approved face. Preserved smoke/vortex PNGs are historical assets only. Normal WebGL rendering uses no smoke images. Browser tests verify changing rendered density, directional movement of the white facial features, fully transparent outer pixels, frozen reduced motion, truthful unavailable/stale waveform, and mocked drag IPC. Native Mac build/drag/focus/appearance/performance validation must be checked in the revision spec, not assumed from these browser tests.

### Integration and details still to resolve

- **Implementation stack:** integrate into the existing Svelte/Tauri overlay. Earlier design-conversation suggestions of a standalone SwiftUI/AppKit/Metal app were exploratory, not an approved rewrite. Choose a renderer compatible with this app and Intel macOS after evaluating visual fidelity and performance.
- **Assets/prototype:** approved original and derived face/smoke/vortex textures are now saved (see above). First Svelte texture-motion prototype is `apps/whispering/src/lib/components/Companion.svelte`; development-only visual review route `/companion-preview` uses explicitly simulated audio. Production `/recording-indicator` uses real native levels. Native idle visibility, provisional dimensions and bounded cursor-relative head movement have been updated in source. This is not a shipped or visually validated implementation; independent pupil tracking and success-expression wiring remain unfinished.
- **Dimensions, padding, typography, exact colours, shadows:** not specified. The earlier 100–150 px reference prompt describes small-size readability, not an approved native window size. Prototype bounds with room for fading smoke and readable status semantics; update native bounds/placement together if changing size.
- **Placement:** not specified beyond floating on the Mac screen. Retain existing placement unless Roger approves a change.
- **Recording:** approved visual target is the purple waveform form. Drive it from real available microphone levels; silence settles toward zero. Meter-unavailable and stale-input presentations remain to be designed, without fabricated voice activity.
- **Processing:** vortex is the approved general visual target. Distinguish transcribing and cleaning with readable semantics; their exact visual distinction remains unresolved.
- **Preparing, errors and native-poll disconnection:** no final appearance approved. Propose legible states without silently inventing new character designs. Do not promise a preparing transition without wiring/testing it.
- **Completion:** happy form is approved artwork, but its trigger and display duration are not specified. Existing snapshot has no success phase; implementation needs an explicit, tested completion event/state if showing it, preserving newer-recording priority.
- **Idle visibility APPROVED:** Roger explicitly confirmed, “Gastly should be available all the time.” Keep idle Gastly visible whenever the app is running, including between transcription jobs. After completion or error expiry, return to idle Gastly rather than hiding the window. This supersedes the previous hide-at-idle behavior. Preserve click-through/non-focusable operation and recording priority; update native visibility logic and its tests accordingly.
- **Idle character animation APPROVED:** Roger wants the idle Pokémon itself to move and show facial changes, not only animated gas. He suggested looking toward the mouse as an example. Implement an expressive living idle; cursor-following gaze is a requested direction to prototype, not a finalized tracking specification. Candidate details to propose: subtle head bob/tilt, occasional blinks and small eye/pupil movements toward the pointer. Preserve the approved face's visual identity. Do not treat this as approval for click handling, focus stealing, mouse interception or new system permissions. Verify that cursor position can be obtained safely with the existing app/window architecture before promising tracking behavior.
- **Motion:** floating/bobbing idle, flowing gas, facial animation and transformations are desired. Exact timings, gaze limits/response and a reduced-motion presentation remain to be specified. Reduced motion should retain intelligible state changes without continuous turbulence, bobbing, pointer-following motion or morphing. A flattened face image alone cannot deliver independent eye movement; prepare layered facial assets or a compatible renderable face and validate likeness.
- **Interactions:** no new controls, dragging or focus behavior approved. Preserve click-through, non-focusable operation. Earlier assistant suggestions of draggable prototypes do not constitute Roger's approval.

Identify final approved choices versus optional explorations. Include exact asset paths or attach files; do not rely on another conversation being accessible. If code is supplied, identify whether it is a visual prototype or production-ready implementation.

## Product decisions to preserve

This is **Whispering Intel**, Roger's Intel macOS fork of upstream Whispering v7.10.0, not the separate Android project.

Roger dislikes the existing overlay's appearance and wants his new design implemented. Preserve the existing recording/pipeline behaviour unless he explicitly changes it.

Latest explicit app-lifecycle requirement: **menu-bar/tray only, NO Dock item**. This supersedes all earlier persistent-Dock requirements in older specs/docs. The menu-bar icon and the floating overlay are separate surfaces. Keep the native menu with Show Window, Settings and Quit; closing the main window hides it without quitting.

Other decisions: telemetry removed; automatic Enter OFF by default, with a separate manual Enter action; automatic LLM cleanup explicitly opt-in and OFF by default; optional mouse bindings alongside keyboard bindings. Roger reports OpenAI dictation and mouse buttons work well. Do not disturb his provider/key/settings or broaden testing to speech-recognition quality.

## Historical initial-handoff repository/build snapshot (superseded above)

- Working directory: `/home/piweb/workspace/projects/whispering-intel`.
- Repository: https://github.com/archsinit/epicenter ; branch: `whispering-intel`.
- Latest implementation commit when this file was written: `8982791` (`feat: run Intel fork from menu bar without telemetry`). Recheck HEAD/status before editing; concurrent work may have advanced it.
- That commit includes native menu-bar ownership/Accessory activation policy and telemetry removal, but **does not redesign the overlay**.
- Local validation: 22 focused Bun tests pass; app type check zero errors/17 inherited warnings.
- Intel Mac CI: https://github.com/archsinit/epicenter/actions/runs/43364967807 was **in progress** when checked for this handoff. Do not assume success; read its actual result/report. Two new native icon tests await that validation.
- Roger's installed Mac build is still `dd43322`; it does NOT yet contain menu-bar-only behaviour, telemetry removal or the new design. No replacement was installed during this work.
- Original upstream app must remain untouched. Both processes can be named `whispering`; target the fork by bundle ID `com.archsinit.whispering-intel`, never by process name alone.
- Never stage/overwrite/revert unrelated untracked `apps/whispering-android/` or mobile specs.

## Implementation entry points

Paths below are relative to the repository root:

| File | Responsibility |
| --- | --- |
| `apps/whispering/src/routes/recording-indicator/+page.svelte` | Overlay markup, styles, state labels, waveform history and native polling. Primary visual implementation target. |
| `apps/whispering/src-tauri/src/indicator.rs` | Native window dimensions/placement, transparency, click-through/focus behaviour, visibility and real microphone-level snapshot. |
| `apps/whispering/src/lib/query/indicator.ts` | Phase orchestration, overlapping-job tracking, recording priority, serialized native updates and error expiry. |
| `apps/whispering/src/lib/query/indicator.test.ts` | Nine focused controller tests with native IPC mocked. Preserve/extend these for behavioural changes. |
| `apps/whispering/src-tauri/src/audio_meter.rs` | Real audio RMS calculation. Not a visual redesign target. |
| `apps/whispering/src-tauri/src/menu_bar.rs` | New native menu-bar icon/menu and Dock-free lifecycle. Keep separate from the overlay. |
| `apps/whispering/src/lib/services/tray.ts` | Frontend updates native tray recording status; does not create tray icons. |

Current native overlay: 280 × 64 logical pixels; transparent, borderless, always on top, non-resizable, initially hidden, non-focusable and click-through; visible on all macOS workspaces. Positioned horizontally centred on the primary display with its top approximately 120 logical pixels above the display bottom. If the new design changes size, update BOTH the native bounds and positioning calculation, not just CSS. Allow space for shadows and the longest state labels; avoid clipping.

## Existing state/data contract

The UI polls native `get_indicator_state` for:

```ts
type Snapshot = {
  phase: 'idle' | 'preparing' | 'recording' | 'transcribing' | 'cleaning' | 'error';
  level: number | null;
  inputActive: boolean;
};
```

- `idle`: current implementation hides the native window. Roger has now explicitly approved permanent idle Gastly (see design decisions above); change this behavior so the companion remains visible while the app runs.
- `recording`: waveform derives from actual CPAL input samples. Current display keeps 24 levels, scaled with `min(1, sqrt(level) * 2)`.
- Silence should settle toward zero. Missing/stale samples and an unavailable meter must not be presented as active voice.
- `level === null`: meter unavailable, not silence. Navigator/FFmpeg do not supply a live waveform; do not invent one.
- `inputActive`: a real audio frame arrived in the last 500 ms. This is sample freshness, not voice detection; silence can still be active input.
- `transcribing` and `cleaning`: distinct processing states, with no fake audio waveform.
- `error` and native-poll disconnection: visible, understandable failure states.
- `preparing` exists in native/UI types, but the current query controller does not publish it. Do not promise this transition without wiring/testing it.
- Current polling is serialized: about 40 ms while active, 250 ms while idle/disconnected; clean up timers on unmount.
- New recording takes precedence over older processing jobs. Completion of an older job must not hide a newer recording. Preserve the controller's concurrency rules.

The overlay must not steal typing focus, intercept mouse clicks or submit text. Do not add controls to the click-through surface without agreeing the interaction and native-window changes first. Keep readable status semantics and reduced-motion support.

## Implementation workflow and acceptance

1. Read repository/app `AGENTS.md`, relevant rules, `HANDOFF.md`, `WORK-STATE.md`, this file and the supplied final design. Recheck current code/CI rather than relying on this snapshot.
2. Resolve missing design details with Roger; do not silently invent a different design. Save the implementation plan in `specs/`. Routine implementation/build/test/commit/push approval already exists for this fork.
3. Make narrow changes to the overlay. Do not revert the new menu-bar-only requirement or restore telemetry while integrating a prototype.
4. Run the focused checks below; extend narrow tests if behaviour changes.
5. Build on Intel macOS and inspect the actual validation report. Automated/controller/source tests do NOT prove visual appearance, animation, native focus or physical-device behaviour.
6. Coordinate desktop testing with Roger. Verify the approved appearance, real voice/silence response, meter-unavailable/error presentation, processing/cleanup transitions, return to visible idle Gastly after completion and no focus theft. Check bounds/shadows/labels at his display scale. Verify no Dock item and continued menu access after closing the main window.
7. Record actual passes, failures and remaining limits. Do not call the work fully tested without the real-Mac checks.

```sh
export PATH=/home/piweb/.local/bin:/home/piweb/.cargo/bin:$PATH
bun test scripts/no-telemetry.test.ts scripts/menu-bar.test.ts \
  apps/whispering/src/lib/query/indicator.test.ts \
  apps/whispering/src/lib/query/cleanup-policy.test.ts \
  apps/whispering/src/lib/services/hold-gate.test.ts
bun run --cwd apps/whispering check
```

Root monorepo `bun check` has a known unrelated demo-mcp missing-tsc failure; do not expand this task to fix it. Use `gh -R archsinit/epicenter ...` explicitly; this checkout otherwise defaults GitHub CLI to upstream.

## Mac safety and test limits

Authorized Mac access/precise targeting/restoration details are in workspace-private `pi-web/whispering-mac-testing.md` (outside this repository). Never commit private connection details, credentials, raw UI dumps or session targets.

Keep bundle ID and installation path stable; coordinate replacement of the installed ad-hoc-signed build because a changed binary may require normal user-approved permissions again. Do not create certificates, alter trust/TCC/Gatekeeper or change remote-access credentials. Do not reinstall an unchanged binary.

Use isolated documents/localhost fixtures for delivery tests; never send text to a real chat or expose/read provider keys. Preserve Roger's working OpenAI setup. Fixture instructions: `scripts/integration/README.md`. Restore temporary settings/bindings and remove only test-owned data.

The Intel continuation scheduler remains paused and targets an older conversation. Do not unpause/repoint it blindly. This file does not establish background execution.
