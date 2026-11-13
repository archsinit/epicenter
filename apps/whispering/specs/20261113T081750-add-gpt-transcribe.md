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

- [ ] Extract pure OpenAI catalog/request builder for isolated testing, retaining the service's existing exports/consumer paths.
- [ ] Append gpt-transcribe with current price/description. Preserve existing order (registry uses first item as its fallback), settings default and saved selections.
- [ ] Apply the new model's plural language field while keeping legacy/custom-model request behavior unchanged.
- [ ] Test real SDK multipart serialization against an injected local synthetic fetch: model, file metadata, prompt, temperature, plural/singular hints, auto language, and JSON text responses. No real keys, microphone or paid API requests.
- [ ] Run focused regression/type checks; add tests to Intel CI, build and inspect the validation report.
- [ ] Install the new build at the stable fork path with backup, keeping current model selection/settings unchanged; verify app/overlay readiness. Leave new model selection/real-provider quality check to Roger unless he authorizes it.

## Review

In progress. Current installed runtime is 02a721b (reversed swirl). Pre-existing HANDOFF.md/WORK-STATE.md edits and unrelated Android files will be left untouched. Routine implementation/tests/builds/commits approval is recorded in HANDOFF.md.
