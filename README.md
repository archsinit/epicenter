# Whispering Intel

My personal Intel macOS fork of [Whispering](https://github.com/EpicenterHQ/epicenter/tree/main/apps/whispering), based on v7.10.0.

All credit for the original app goes to Braden Wong and the Whispering contributors. This is just a fork with a few changes for how I like to use it, not a separate product or an official upstream release.

## What changed

- Intel Mac builds with a separate app identity, so the original app can stay installed.
- Menu-bar-only operation, without a Dock icon. Closing the main window keeps the app running.
- A draggable Gastly companion with animated smoke, cursor-aware expressions and a microphone-driven gas waveform. It moves to bottom-center while recording, then returns home before the processing swirl starts. Respects Reduce Motion.
- Optional extra mouse-button shortcuts and a separate manual Enter shortcut. Automatic Enter is off by default.
- Explicitly opt-in LLM cleanup, off by default.
- Usage telemetry removed.
- `gpt-transcribe` added to the OpenAI model choices without changing existing selections.

Builds are available from the [Intel macOS workflow](https://github.com/archsinit/epicenter/actions/workflows/intel-macos.yml). Personal-use builds are ad-hoc signed, not notarized.

The original [AGPL-3.0 license](LICENSE) and copyright notices are retained. Gastly and Pokémon belong to their respective rights holders; no affiliation is implied.
