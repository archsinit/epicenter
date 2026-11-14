# Meeting transcription boundary and adapter audit

This is separate from ordinary dictation delivery. Only an explicit Start/Resume action sends audio. The query snapshots the selected adapter, model, language, ASR prompt, temperature, endpoint and credentials once per operation. Settings are not changed. SHA-256 of non-key configuration identifies compatible local resume; API keys and raw configuration are never persisted. No compression, clipboard, paste, Enter, LLM cleanup, summary or screenshot processing is inherited.

## Audio and provider budgets

The existing native worker already splits mixed audio on a 60-second grid. Meeting transcription accepts only finalized canonical mono 48,000 Hz PCM16 WAVs with 44-byte headers, at most 5,760,044 bytes and one minute, matching native manifest duration. Requests are sequential, not whole-meeting buffers. Images/raw tracks/arbitrary paths/unregistered/active/unfinished audio are refused by separate native and client boundaries.

Audited existing adapter source (not a claim of freshly checked commercial pricing/quotas):
- OpenAI, Groq, Mistral: adapter size guards are 25 MB.
- Deepgram: adapter size guard is 500 MB.
- ElevenLabs: adapter size guard is 1,000 MB.
- speaches: self-hosted limits vary; adapter documents typical 25 MB compatibility. Custom endpoints/proxies can impose smaller limits.
- whisper.cpp and Parakeet: existing configured local model/path checks; native audio conversion handles WAV input. Their transcription entry points do not download models.

The one-minute native input is below every bundled cloud adapter's size guard. It bounds local memory too. The normal adapters still enforce their credentials/model/rate/endpoint restrictions and may apply SDK-internal transport retries. The meeting orchestrator does not retry failed batches automatically, switch providers/endpoints, guess credentials, compress on failure or bypass denials. 429/size/model/endpoint errors pause the batch with saved text/audio retained; custom lower limits require configuration or future explicit smaller-chunk support, not a silent workaround.

## Durability and failure semantics

Private transcripts/<native-id>.json in each meeting contains audio indices/offsets/durations, provider label, configuration digest and per-chunk text. Every successful chunk is atomically synced before another provider request. Resume is explicit and configuration-matched, skipping saved chunks. First durable result per chunk wins, making local-save retries idempotent. Text remains escaped/plain in the UI.

Provider results awaiting a failed local save remain visible in memory; Retry local save only never calls a provider. That memory is NOT durable until filesystem save succeeds. Audio and all prior saved segments remain on errors. Pause drains/saves the current request then stops; it does not claim to abort an in-flight network/native ASR request. Keep the app running while transcribing. Closing/restarting during an unacknowledged request may require retranscribing that minute and can incur another provider charge. No exactly-once cloud billing claim is made.

Local transcript bounds: 1 MiB UTF-8 per chunk, 16 MiB per JSON document, 10,000 history files per directory scan. Failures retain files; no automatic deletion. Native directory/manifest/asset validation and index-only mixed audio access remain in force. Local files are not encrypted and follow the user's backup/sync policies.

Synthetic provider spies and filesystem tests establish the code boundary; real provider quality, model performance, native permission/capture behavior and UI/media rendering remain coordinated acceptance, never unattended tests.
