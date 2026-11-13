# Irregular billows and lagging wisps

Roger says the current outward motion looks like small, uniform pond ripples and the cloud remains too round. He wants small vibrations mixed with much larger, uneven expansion/contraction in different directions, longer plumes in some regions, and gas that lags/trails when the head moves. His new cyan Gastly illustration is a silhouette/motion reference; retain the approved purple palette and existing face. Saved workspace reference: `/home/piweb/workspace/gastly-companion/silhouette-reference-cyan.png`. Roger further clarified that the result must still look natural: soft translucent billows and wisps, not cartoon outlines, spikes or angular fans.

## Plan

- [x] Add independently evolving low-frequency directional plume lengths plus mid/fine turbulence; deform the outer density envelope as well as internal texture. Avoid one global circular breathing pulse.
- [x] Keep near-core emission attached to the face/waveform, with older outer wisps following a delayed emitter center. Add only a small head lean alongside the existing genuine yaw/pitch, not a replacement flat-image wiggle.
- [x] Bound large plumes smoothly within the existing 180×120 transparent viewport, without flat clipping or native shadows. Recording envelope stays driven by actual audio rather than random voice-like peaks.
- [x] Preserve the recent crown-rim, template icon, no-bars waveform and transition fixes. Reduced motion freezes billowing/trails but still displays current audio.
- [x] Run pure/browser/pixel checks and inspect screenshots/motion before the next build. Do not drive Roger's desktop/mouse while he is actively testing.

## Current installed evidence

`dcaefc2` was installed after CI run 43381624764 passed 30 Bun + 16 Rust tests, types, signature and startup checks (actual report read). Actual Mac AX shows “Gastly with evolving smoke”, confirming the WebGL renderer/asset loaded in the packaged NSPanel host. The main window closes while the companion stays visible; status item remains present and activation policy is Accessory. Native screen capture remains unavailable.

A guarded native input test was interrupted/refused after its first pointer action because the isolated fixture was no longer foreground or did not cover the gesture; it aborted and cleaned up. **Do not claim packaged drag/focus or margin click-through passed.** Standalone exact NSPanel host tests passed earlier, but these are not equivalent. Roger is actively using/testing the desktop; pause synthetic input until coordinated. Placement across mixed-scale monitors also needs review rather than a claimed pass. No real microphone/provider requests or settings/bindings/OS permission changes were made by the tests.

## Review

Implemented and browser-reviewed, NOT installed on Mac yet. The installed build remains dcaefc2. Rejected an uncommitted angular-envelope experiment after inspecting its fan-like silhouette; it was never deployed. The revised density envelope instead blends eight soft overlapping ellipsoidal volumes with independent birth, growth, curl and fade, plus small local flutter. Cartesian internal noise avoids concentric radial rings. The shared core stays connected to the face while older outer wisps use an 850ms delayed source center. Bounds are softly faded, with no hard plume outlines.

31 focused Bun tests pass; app check has 0 errors/17 inherited warnings; diff whitespace check passes. Browser render tests verify 5px quiet / 41px voice gas envelopes; directional extents differ by 10–21 pixels at sampled times, with some directions growing while others contract; forced delayed-source test shifts the outer cloud centroid from x94.85 to x82.42 while the head remains at the same source position. Existing no-bar, head-turning, transparent-border, truthful-meter, transition and reduced-motion tests pass. Light/dark screenshots were inspected. Workspace preview `/home/piweb/workspace/gastly-companion/natural-billows-preview.gif` is an isolated browser idle capture (2× device scale); `natural-billows-light.png` shows light-background blending.

Visual fidelity remains Roger's judgment. No native deployment or additional desktop-driving input was done for this billow revision; do not claim Mac appearance/performance/focus passed. Preview first while Roger is actively testing; a native build may run through CI, but no promise of unattended installation.
