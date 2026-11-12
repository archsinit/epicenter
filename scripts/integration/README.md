# Local pipeline fixture

This tests the app's request routing and cleanup consent without a microphone, real provider credentials or paid model calls. Responses are canned; it does not test speech-recognition accuracy.

The Python standard-library server binds only to `127.0.0.1`. It never proxies requests. It accepts only the dummy key `sk-whispering-local-fixture-only` and records route/byte-count metadata, not audio, keys or prompts. The fixture transformation uses the Custom completion provider with an explicit loopback endpoint, so cleanup cannot fall back to a cloud provider.

## Commands

```
python3 -m unittest discover -s scripts/integration -p 'test_*.py' -v
python3 scripts/integration/mock_provider.py --port 18763 --delay 1
```

`GET http://127.0.0.1:18763/health` identifies the fixture server. `GET /events` returns the requests received since startup.

## App test procedure

Use a dedicated test app profile. Do not overwrite existing credentials. Record all original settings and restore them afterward.

1. Turn OFF both transcript and transformed-text copy/paste and automatic Enter before uploading. Never test with a real chat composer focused.
2. Generate a small silent WAV file, for example 0.25 seconds of 16 kHz mono PCM. Do not capture ambient microphone audio.
3. Set the transcription service to OpenAI, Base URL to `http://127.0.0.1:18763/v1`, and key to the dummy key above. Set Custom API Key to the same dummy key only for the cleanup fixture.
4. Load `cleanup-fixture.md` into the test profile's supported file-based transformation store, without overwriting any existing transformation. Select it in the app. Its schema was validated against the app's Transformation validator before the initial Mac test.
5. Cleanup OFF: upload the WAV via the real app's file chooser. Expect `/v1/audio/transcriptions` only, no `/v1/chat/completions`, and the raw fixture result.
6. Cleanup ON: upload again. Expect one transcription request followed by one chat-completion request; the fixture transformation run must finish with the cleaned fixture result.
7. Restore cleanup OFF, selection, service, empty test credentials/endpoint and original delivery settings. Remove only the fixtures created by this test. Stop only the mock-server process started by this test.

On macOS, AX text-field value assignment may not dispatch Svelte input events; normal typing into the specifically focused test field does. Never put real keys in the fixture fields. Password AX values may be masked. Use normal UI input, not direct WebKit/private settings database editing or permission bypasses.

## Observed on the actual Intel Mac

Tested app `dd43322` on Intel macOS 26.7.1:

- No selected transformation, cleanup OFF: one transcription request, raw fixture output visible.
- Fixture selected, cleanup OFF: one more transcription request, no cleanup request.
- Fixture selected, cleanup ON: one more transcription plus exactly one cleanup request. The saved fixture run was completed and contained the expected cleaned output.
- Totals: three transcription requests (8,443 bytes each) and one cleanup request (186 bytes).
- Five mock-server tests passed on both Linux and the actual Mac.
- Restored original app preferences, cleared dummy keys/endpoint, removed three test recordings, one fixture run and the fixture transformation, and stopped the mock server. Existing upstream Whispering was untouched. Clipboard contents were neither read nor changed; no chat message was sent.

This does not verify physical microphone levels, keyboard/Enter injection, real mouse-driver suppression, or a real provider's transcription/cleanup quality. Those remain separate acceptance checks.
