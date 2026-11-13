# Gas-shaped waveform, outward emission and transition refinement

Roger's latest feedback: preserve the idle-to-processing transformation he likes; improve recording-to-transcribing and especially processing-to-idle. The processing spin must stop before the face returns. Gas must visibly originate at the face/waveform core and spread outward, not behave as a disconnected background. The recording waveform must be made of gas itself, not separate white bars. The menu-bar icon must adapt to the actual light/dark menu bar.

## Plan

- [x] Reapply native template mode after each tray icon replacement. Verified dependency cause: tray-icon 0.21.2 macOS `set_icon` passes `false` to image creation, even when the initial builder was template-enabled. Extend narrow regression guard; no OS appearance changes.
- [x] Feed actual 24-sample amplitude history to the gas shader and shape its entire density envelope as a continuous purple waveform. Remove DOM bars. Missing/stale samples remain zero; never fabricate live voice activity.
- [x] Advect density outward from the shared core (head/recording form), including source-age variation and fading as material expands. Keep independently evolving wisps; no image rotation.
- [x] Separate head visibility from form morphing. Stop swirl before revealing the returning face; use integrated spin phase rather than multiplying absolute elapsed time by a shrinking blend (which caused unwinding during return).
- [x] Retain the last real recording history during the brief form morph, rather than clearing it immediately at a phase change and collapsing the ribbon. New recording resets history normally; current recording meter loss still settles to zero.
- [x] Remove the pale crown ring Roger identified in his screenshot: retouch only the outer upper edge of the derived head texture, preserve the approved reference and broad forehead lighting, and remove the added shader rim highlight. Verify in both fallback artwork and projected sphere.
- [x] Extend pure transition tests and actual rendered-pixel browser tests (voice vs silence volume shape, no bar DOM, reduced-motion real-level response). Inspect screenshots/motion.
- [ ] Batch all changes before the next Intel build; verify actual validation report, install only the fork at stable path with backup, then test native NSPanel/renderer readiness/drag/focus and tray behavior where permissions allow. Preserve working bindings, provider settings and upstream app.

## Existing native follow-up

Commit f34f6cc CI run 43377373844 completed successfully but is not yet installed. It contains the real NSPanel host/focus fix; the currently installed 58ab5ab still has the earlier drag activation issue. Standalone exact AppKit host tests have already verified non-activating drag, but the packaged Tauri/WebKit integration is still pending installation/runtime verification. Do not claim native focus is fixed until that test passes.

## Review

Local 30 focused Bun tests pass; app check reports 0 errors/17 inherited warnings; diff whitespace check passes. Browser tests compile/run the real GLSL and verify the gas itself changes from a 6px quiet envelope to 42px voice envelope in a fixed-time render, with no separate waveform DOM/bars. Tests also verify actual uploaded missing/stale samples become zero, historical shape survives the recording-to-processing morph, reduced motion still responds to fresh audio, head yaw/pitch and fully transparent borders. Pure transition tests hold returning head visibility at zero until swirl speed is zero while preserving gradual entering transitions. Screenshots and an enlarged head crop were inspected; the narrow pale crown arc is removed, while the broad central forehead highlight remains. Original approved-reference.png is unchanged.

Tray cause is confirmed from dependency source and covered by an ordering regression guard; actual menu-bar tint on Roger's display still requires the new build/user check. The native NSPanel packaged integration/drag/focus test is still pending for the combined build. No provider/settings/mouse bindings/OS permissions changes.
