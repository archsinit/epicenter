# Add GPT-Transcribe without changing existing selections

Roger requested `gpt-transcribe` be added to the app's OpenAI options. Add it alongside the existing three; keep saved models, keys, endpoints, defaults, recording/delivery/cleanup behavior and overlay unchanged.

## Verified API contract

Official speech-to-text guide and Audio Transcriptions reference checked 13 November 2026:
- `/v1/audio/transcriptions` supports `gpt-transcribe`, returning JSON with `text` and optional detected `languages`.
- For this model, `languages: string[]` replaces singular `language`; never send both. Auto detection sends neither.
- Existing `prompt` is supported; the API reference lists optional temperature. Keep existing prompt/temperature behavior.
- 25MB upload limit and supported file metadata remain applicable.
- Price: $0.0045/minute, $0.27/hour. Do not add the streaming-only `gpt-live-transcribe` to this file-upload selector.
- Installed SDK 5.23.2 predates the model in its narrow AudioModel type, but request.model accepts a string and its multipart serializer forwards extra fields. Verify plural languages serialization with the real installed SDK; avoid an unnecessary dependency upgrade.

Sources: https://developers.openai.com/api/docs/guides/speech-to-text ; https://developers.openai.com/api/reference/resources/audio/subresources/transcriptions/methods/create ; https://developers.openai.com/api/docs/models/gpt-transcribe

## Plan

- [x] Extract pure OpenAI catalog/request builder for isolated testing, retaining the service's existing exports/consumer paths.
- [x] Append gpt-transcribe with current price/description. Preserve existing order (registry uses first item as its fallback), settings default and saved selections.
- [x] Apply the new model's plural language field while keeping legacy/custom-model request behavior unchanged.
- [x] Test real SDK multipart serialization against an injected local synthetic fetch: model, file metadata, prompt, temperature, plural/singular hints, auto language, and JSON text responses. No real keys, microphone or paid API requests.
- [x] Run focused regression/type checks; add tests to Intel CI, build and inspect the validation report.
- [x] Install the new build at the stable fork path with backup, keeping current model selection/settings unchanged; verify app/overlay readiness. Leave new model selection/real-provider quality check to Roger unless he authorizes it.

## Review

Implemented and pushed as checkpoint `a63f9ae` before any recording-travel changes. The 17 new pure catalog/request and real-SDK synthetic-fetch tests pass without changing dependencies. Existing models/order/defaults and service exports remain intact; only the new model sends plural language hints. No paid request or real-provider quality test was performed.

Installed as part of combined runtime `5e5a282` after Intel CI 43397163550 passed 51 Bun +16 Rust tests, hidden native host, types, signature and startup checks. Actual validation report/logs read; installed executable matches the artifact and prior bundle is backed up. Actual AX verifies companion/UI readiness and tray; selected provider/model, settings and keys were not changed. A browser-only main settings UI probe was inconclusive because the native event bridge was not mocked; do not represent it as a visual dropdown test. Pre-existing HANDOFF.md/WORK-STATE.md edits and unrelated Android files remain untouched. Roger can select the new model when ready.
