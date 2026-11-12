# Intel Mac acceptance tests

CI compiles the real macOS app and runs native unit tests on an Intel Mac runner. It also checks the executable architecture, fork identity and bundle signature. When the runner has a GUI login session, it launches the built executable and checks that it stays alive for eight seconds; otherwise it records an explicit skip. The artifact includes the validation report.

Startup liveness does not prove window/UI readiness, physical microphone capture, Accessibility permissions, Dock interactions or actual mouse-driver behaviour. Those require an interactive Intel Mac.

No live provider API calls are made by the automated tests. Use a dedicated low-budget key for the manual dictation tests.

## Verified build

Test commit `dd43322`, [CI run 43259930929](https://github.com/archsinit/epicenter/actions/runs/43259930929), passed on Intel macOS 15.7.9: frontend types, seven Bun tests, twelve native Rust tests, three CI failure-propagation checks, binary architecture/identity, deep-strict signature verification, and eight-second startup liveness in a GUI session. The cleanup signal printed after the startup pass is the test terminating its own app process, not a startup failure.

Use [this app artifact](https://github.com/archsinit/epicenter/actions/runs/43259930929/artifacts/13963969603), not earlier builds. Previous artifacts have a known updater-initialization crash. The new artifact includes the actual validation report; it expires after 14 days. All interactive checks below remain unperformed.

## Installation

Download the app artifact from this fork's Intel macOS workflow after a successful run. The initial build is ad-hoc signed, not Developer ID signed or notarized. Follow normal macOS approval prompts; do not disable Gatekeeper. The fork has its own app identity and stores data separately from upstream Whispering. Updates are manual; it does not install upstream releases.

## Dock and focus

- Launch the app. Its Dock icon should remain present.
- Close the main window. The app must stay running and respond to the recording shortcut.
- Click the Dock icon. The main window should reopen. Repeat after minimizing and after relaunching from Finder.
- Use the app's Quit menu or Command-Q. The process, indicator, global bindings and microphone capture should stop.
- In TextEdit, put the cursor inside a document and start/stop recording using the global shortcut. The indicator must not steal typing focus. Repeat on another display, another Space, and a full-screen app.

## Waveform and processing

- Use the default CPAL recording method and the intended microphone, not an unrelated default device.
- Speak softly/loudly; the waveform must respond to actual input, not animate independently.
- Stop speaking: waveform should settle and indicate quiet input.
- Deny microphone access, disconnect the input, and retry: errors should be visible and there must not be a fake active waveform.
- Stop a successful recording: display Transcribing, then hide when done.
- Enable cleanup with a valid transformation: display Cleaning up separately.
- Begin a new take while the previous take is processing: finishing the previous job must not hide the current take's indicator.
- Navigator/FFmpeg recording currently displays meter-unavailable rather than a fabricated waveform; the first version's actual waveform is CPAL-only.

## Enter and cleanup

- With fresh settings, both automatic Enter options and automatic LLM cleanup should be off; mouse bindings should be empty.
- Assign a global Enter shortcut. Dictate into a chat composer: text should be inserted, never automatically submitted. Review/edit it, then trigger Enter independently.
- Deny Accessibility permission: Enter should report failure rather than claim success.
- Select an existing transformation but leave the cleanup toggle off. Verify only transcription is requested, with no automatic transformation run.
- Enable cleanup and repeat. Verify the selected transformation runs. Disable it and repeat again.
- Manual transformations should still work while automatic cleanup is off.

## Mouse buttons

- Start unassigned: all mouse buttons keep their normal behaviour.
- Grant the app normal Accessibility/Input Monitoring permissions when configuring a binding; save and retry if needed.
- Assign a side button to Toggle recording; verify a keyboard shortcut still triggers the same action.
- In a browser with page history, verify the assigned side button starts/stops recording but does not navigate Back/Forward. Test button-down, button-up and dragging.
- Assign another button to Enter. Verify it submits only when explicitly pressed.
- Assign Push to talk: down starts, up stops. Test changing/removing the binding while held and permission revocation; it must not leave recording stuck on.
- Duplicate assignments must be rejected. Primary/secondary clicks are deliberately not assignable.
- Remove bindings and apply: normal mouse behaviour should return. Quit: all interception must stop.
- Test with actual vendor mouse software. If the vendor remaps the button into a keyboard event, configure it as a keyboard shortcut instead.

Record the macOS version, Mac model, input device, mouse/driver, tested commit and actual results. Do not mark unperformed tests passed.
