# Approved Gastly companion reference and prototype assets

`approved-reference.png` is Roger's original uploaded 1536×1024 reference sheet, recovered losslessly from the design conversation attachment. It contains genuine alpha transparency (including partially transparent vapor). It is the approved visual target; earlier cartoon and CGI explorations are not replacements.

The other PNGs are derived locally, without new image generation:

- `idle-head.png`: isolated original idle face, used by the first prototype.
- `idle-smoke.png`: original idle aura, with head removed and a soft outer mask to isolate it from the neighboring forms. A face-sized hole remains behind the separate head; this is a prototype layering compromise.
- `vortex.png`: original processing form with a soft isolation mask.
- `happy-head.png`: original happy face, prepared but not connected to success events yet.

Regenerate from repository root with `node scripts/gastly-assets.cjs`, with `sharp` installed/resolvable in that Node environment. Sharp is an offline preparation tool, not an added app dependency. Source pixel coordinates and masks are in the script; source reference is never overwritten. This workspace used `NODE_PATH=/opt/pi-web/node_modules`.

The prototype uses layered texture transforms for smoke, coordinated fades/scales for state changes, and a separately generated real-level waveform. It is not a physical smoke simulation or a completed facial rig. Pointer direction moves/tilts the entire head; independent pupil tracking needs additional layer preparation. Verify visual fidelity, seams, blink placement, alpha blending and animation/performance on Intel macOS before shipping.

Development preview: run the app's `dev:web` script and open `/companion-preview`. Audio there is explicitly simulated. The preview controls are development-only and do not invoke native recording or providers. Production `/recording-indicator` uses actual native samples and remains click-through.
