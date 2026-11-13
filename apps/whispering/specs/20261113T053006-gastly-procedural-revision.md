# Gastly revision: evolving smoke and compact draggable companion

Roger rejected the rotating-image smoke and processing animation. Latest approval: proceed with procedural smoke whose wisps continuously curl, deform, form and dissipate. The approved original remains the appearance reference, not an animated smoke texture. Mouse shortcuts are working by Roger's latest retest; leave bindings/event-tap code/permissions alone.

## Plan

- [x] Replace rotating smoke/vortex sprites with time-varying domain-warped volumetric noise; continuous transitions idle ↔ waveform ↔ processing. This is procedural rendering, not a physical fluid solver.
- [x] Project the approved face onto a ray-rendered sphere and rotate its surface toward the cursor (yaw/pitch), rather than sliding/rolling a flat sprite. Retain frontal likeness and add blinking. This is not an independently rigged character or newly generated multiview asset.
- [x] Eliminate the face-cutout gap by compositing smoke around/behind/in front of the sphere without a punched-out texture.
- [x] Reduce native and visual bounds from 360×240 to 180×120 (50% in each dimension). Disable native shadows and avoid clipped alpha edges/rectangular CSS decorations.
- [x] Remove all visible routine text. Preserve screen-reader status and a small graphical warning for error/unavailable/stale meter. Real waveform samples remain truthful.
- [x] Add a primary-button drag target over the character, without focusing the window; dynamically keep transparent surrounding areas click-through. Use a native drag command scoped to the overlay window; do not change extra-button bindings. Drag position lasts for this run initially; persistence/recovery policy is not newly specified.
- [x] Extend focused browser/native geometry tests; inspect actual light/dark screenshots, temporal noise changes, head turning, no visible text and drag dispatch. Run narrow type/controller checks.
- [ ] Build/validate Intel CI, then replace only the fork bundle at the stable path with backup; coordinate normal permission prompts with Roger. Test real window size/idle visibility/no Dock/dragging/focus where permissions allow; do not fabricate visual or hardware passes.

## Prior test evidence and scope

First prototype `8173f6b` installed with backup after CI run 43369514361 passed 25 Bun + 15 Rust tests and signature/eight-second startup validation (actual report read). Native probe confirmed visible 360×240 idle overlay, Accessory activation, no matching Dock entry, own-overlay accessible “Ready” and persistence after closing main window. Screen capture is not permitted for the native probe; no screen permissions changed. Roger supplied actual visual feedback. AX showed an Accessibility warning, but he subsequently confirmed mouse shortcuts work: do not diagnose them as broken based on that warning.

No provider/settings/bindings changes, real microphone capture, real API transcription, OS permission changes or upstream app changes are part of this revision.

## Review

Implemented locally. 25 focused Bun tests pass; app check has 0 errors/17 inherited warnings; diff whitespace check passes. Revised Playwright Chromium test passes and screenshots were visually read: 180×120 bounds, WebGL/no sprite smoke, actual white-feature centroid shifts for yaw/pitch, changing per-frame density without CSS rotation, waveform/silence, inaccessible/missing/stale input warnings, no visible routine labels, transparent border pixels, light/dark backgrounds, frozen reduced motion, primary-only drag dispatch and noninteractive margins, IPC disconnection/recovery and serialized polling. Added native ellipse hit-test coverage; awaits Mac execution. Native shadow disabled in source; actual OS dragging and native edge/focus/performance still unverified for this revision. No settings, bindings, permissions or upstream-app changes.

### Native execution follow-up

Revision `58ab5ab` CI run `43373671725` passed 25 Bun + 16 Rust tests, types, signature and startup checks; actual downloaded report and relevant log entries read. Installed at the stable fork path with prior bundle archived. Native probe confirmed 180×120 visible idle window and Accessory activation. Isolated native fixture confirmed transparent-margin click-through and a primary drag moving the overlay +60/-60 pixels. **It also caught an application-activation regression:** the fixture lost foreground status to the fork during drag. Therefore that build is not accepted as non-focus-stealing.

Follow-up source uses a real non-activating NSPanel, allocated normally with public AppKit APIs (no class swizzling or memory-layout changes), hosting the retained existing webview content while Tauri's hidden original window keeps IPC ownership. A transparent native drag surface does not need the panel to become key. AppKit functions execute on the main thread. Other platforms retain normal Tauri window handling. No dependencies, provider settings, shortcut/event-tap code or permissions changed.

The exact Objective-C host compiled on Roger's Intel Mac. An isolated standalone host test moved its panel +30/-30 pixels while the fixture retained foreground status; underlying fixture received no click. Preserved test tools: `scripts/integration/gastly-native-probe.swift` and `gastly-panel-host-test.swift`. Full packaged Tauri/WebKit integration for this follow-up still awaits CI/install/runtime tests. Native screen capture remains unavailable; no desktop snapshots/permission changes were made.

Known rendering compromises: procedural noise volume rather than a fluid solver, original front artwork projected onto a sphere with approximated sides, small graphical warning dots instead of visible text, no success animation or placement persistence. Graphics fallback uses independent gradient billows; not claimed to match normal WebGL quality.
