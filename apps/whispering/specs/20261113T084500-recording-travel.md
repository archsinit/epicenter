# Recording travel and saved home position

## Request / checkpoint
Roger requested the recording waveform glide to bottom-center on Gastly's display, then spread into gas and travel to its original position before beginning the processing swirl. Checkpoint `a63f9ae` was committed and pushed before any travel changes; it includes the GPT-Transcribe catalog/request addition and preserves the existing overlay motion. Installed runtime was `02a721b` when work began; the installation below supersedes it.

## Plan
- Move only the real non-activating AppKit panel; retain the 180×120 transparent viewport, approved artwork, waveform data, native drag surface, menu-bar-only policy and existing shortcuts/settings.
- Capture the actual native panel frame at recording entry (including user dragging). Animate to bottom-center of that display's usable area, positioning the waveform core above the Dock.
- On recording exit, return to the saved frame. Expose a visual-only `returning` snapshot flag: gas expands without a face or processing swirl until arrival. The real pipeline phase is unchanged; never fake processing if the job already finished.
- Preserve the original home across repeated recording states and a new recording that interrupts return. Ignore pointer interception during automatic travel; restore normal character-only dragging afterwards. Respect system Reduce Motion without changing the preference.
- Add focused motion/browser sequencing checks and a hidden native host test (no input injection, no visible test window, no provider calls). Run app type checks and the established Intel CI; inspect validation before installation.

## Validation / status
Implemented native 550ms ease-in/out travel and visual-only cloud return gating. Local 51 Bun tests pass; app types: 0 errors/17 inherited warnings. Rendered browser/IPC sequencing checks pass (isolated rerun after a reduced-motion PNG comparison failed during concurrent type-check load). Hidden real-AppKit host tests passed on Mac: dock geometry within backing-pixel rounding, exact saved frame restoration, duplicate phases, interrupted return/new recording, new home after an idle move, pointer mode, non-key/non-main and invisible/non-activating operation. Public AppKit/QuartzCore code compiles with warnings treated as errors. Intel CI 43397163550 passed the same 51 Bun tests plus 16 Rust tests and the hidden native host test, app type checks, signature verification and eight-second GUI-session startup. Actual report and relevant logs read.

Installed `5e5a282` at the existing fork path, with the prior bundle archived as `Whispering-Intel-before-5e5a282.zip`. Transfer ZIP SHA verified; deep/strict signature passed; installed executable SHA matches the validated artifact. Idle confirmed before normal bundle-qualified Quit. Post-launch actual AX confirms visible 180×120 companion, Ready status, WebGL-ready graphic, tray present, and main window hidden as before. Activation policy is Accessory (1). No recording/provider test or synthetic global input was performed; model/provider choices, settings, bindings, permissions and upstream app were untouched.

Native visual travel and packaged drag/focus acceptance require honest separation from hidden host/browser checks. No new OS permissions, provider selection changes, or upstream app changes are authorized by this work.
