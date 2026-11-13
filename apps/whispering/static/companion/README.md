# Approved Gastly reference and preserved prototype assets

`approved-reference.png` is Roger's original uploaded 1536×1024 reference sheet, recovered losslessly from the design conversation attachment. It contains genuine alpha transparency, including partially transparent vapor. Preserve it and the earlier design alternatives; it remains the appearance target.

## Current renderer

`src/lib/components/companion-renderer.ts` generates gas from evolving, domain-warped 3D noise sampled through a small volume. There is NO rotating smoke or vortex image in the normal renderer. Noise advects/deforms at multiple scales, with radius-dependent processing swirl and coordinated waveform transitions. It is a procedural approximation, not a physical fluid solver or a guarantee of photographic fidelity. Performance is capped at 24 draws/second and a 1.5 device pixel ratio; hidden documents skip rendering and reduced motion freezes the field.

The approved `idle-head.png` artwork is front-projected onto a ray-rendered sphere, allowing its facial features/material to rotate with bounded yaw/pitch. Smoke is composited in front of and behind it, with no face-shaped hole in a smoke sprite. It is not an independently rigged face or a complete multi-view character model. Side/back material is approximated; validate likeness at oblique angles. A low-cost independent-billow fallback is used if WebGL is unavailable.

The companion/native window are 180×120 logical pixels (half the original prototype in both dimensions). Normal status text is screen-reader-only; small colored warnings communicate failures or missing/stale metering. Native shadow is disabled. The character has a primary-button-only drag target; native hover geometry leaves transparent margins click-through. Dragging must not take keyboard focus or change extra-button shortcuts. Placement persistence is not implemented yet.

## Preserved asset preparation

These derived PNGs were extracted without new image generation:

- `idle-head.png`: approved idle face, used as the sphere's front projection/fallback.
- `happy-head.png`: prepared but not connected to success events yet.
- `idle-smoke.png` and `vortex.png`: retained historical first-prototype textures, NOT used by the revised renderer.

Regenerate extractions from repository root with `node scripts/gastly-assets.cjs`, with `sharp` resolvable in that Node environment. Sharp is offline preparation tooling, not an app dependency. Source pixel coordinates/masks are in the script; the approved source is never overwritten. This workspace used `NODE_PATH=/opt/pi-web/node_modules`.

Development preview: run the app's `dev:web` script and open `/companion-preview`. Audio there is explicitly simulated. `scripts/test-gastly-browser.cjs` tests the preview and production route with mocked IPC; install Playwright/Chromium in a separate test environment. The production `/recording-indicator` uses actual native samples. Browser tests do not prove native drag/focus, GPU performance or physical microphone behavior on Intel macOS.
