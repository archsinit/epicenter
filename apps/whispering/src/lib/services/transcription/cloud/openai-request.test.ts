import { expect, test } from 'bun:test';
import OpenAI from 'openai';
import { buildOpenaiTranscriptionRequest, OPENAI_TRANSCRIPTION_MODELS } from './openai-request';

const legacy = ['whisper-1', 'gpt-4o-transcribe', 'gpt-4o-mini-transcribe'] as const;
const file = new File([new Uint8Array([82, 73, 70, 70])], 'recording.wav', { type: 'audio/wav' });
const options = {
	modelName: 'gpt-transcribe', outputLanguage: 'fr',
	prompt: 'Synthetic fixture context MOCK-42.', temperature: '0.2',
};

test('catalog adds GPT-Transcribe without reordering or replacing existing models', () => {
	expect(OPENAI_TRANSCRIPTION_MODELS.map(m => m.name)).toEqual([...legacy, 'gpt-transcribe']);
	expect(OPENAI_TRANSCRIPTION_MODELS.at(-1)?.cost).toBe('$0.27/hour');
	expect(OPENAI_TRANSCRIPTION_MODELS[0].name).toBe('whisper-1');
	expect(OPENAI_TRANSCRIPTION_MODELS[2].name).toBe('gpt-4o-mini-transcribe');
});

test('GPT-Transcribe uses plural languages and never sends singular language', () => {
	const request = buildOpenaiTranscriptionRequest(file, options);
	expect(request.model).toBe('gpt-transcribe');
	expect(request.languages).toEqual(['fr']);
	expect(request).not.toHaveProperty('language');
	expect(request.prompt).toBe(options.prompt);
	expect(request.temperature).toBe(0.2);
});

test.each([...legacy, 'gpt-transcribe'])('%s auto detection omits both hint fields', modelName => {
	const request = buildOpenaiTranscriptionRequest(file, { ...options, modelName, outputLanguage: 'auto' });
	expect(request).not.toHaveProperty('language');
	expect(request).not.toHaveProperty('languages');
});

test.each([...legacy, 'custom-proxy-model'])('%s keeps its existing singular language contract', modelName => {
	const request = buildOpenaiTranscriptionRequest(file, { ...options, modelName, outputLanguage: 'en' });
	expect(request.language).toBe('en');
	expect(request).not.toHaveProperty('languages');
	expect(request.model).toBe(modelName);
});

test('request body excludes service credentials and endpoint settings', () => {
	const serviceOptions = { ...options, apiKey: 'sk-synthetic-only', baseURL: 'https://fixture.invalid' };
	const request = buildOpenaiTranscriptionRequest(file, serviceOptions);
	expect(request).not.toHaveProperty('apiKey');
	expect(request).not.toHaveProperty('baseURL');
});

test('empty optional values remain omitted and zero temperature is preserved', () => {
	const empty = buildOpenaiTranscriptionRequest(file, { ...options, prompt: '', temperature: '' });
	expect(empty.prompt).toBeUndefined();
	expect(empty.temperature).toBeUndefined();
	expect(buildOpenaiTranscriptionRequest(file, { ...options, temperature: '0.0' }).temperature).toBe(0);
});

async function serialize(overrides: Partial<typeof options> = {}) {
	let form: FormData | undefined;
	// This is the actual installed SDK + multipart serializer. The injected fetch
	// never performs network I/O and uses no user credentials or recordings.
	const client = new OpenAI({
		apiKey: 'sk-synthetic-fixture-not-a-real-key',
		baseURL: 'https://openai-fixture.invalid/v1',
		dangerouslyAllowBrowser: true,
		maxRetries: 0,
		fetch: async (input, init) => {
			const request = new Request(input, init);
			expect(request.url).toBe('https://openai-fixture.invalid/v1/audio/transcriptions');
			expect(request.method).toBe('POST');
			expect(request.headers.get('content-type')).toContain('multipart/form-data; boundary=');
			form = await request.formData();
			return Response.json({ text: ' fixture transcript ', languages: [{ code: 'fr' }] });
		},
	});
	const result = await client.audio.transcriptions.create(buildOpenaiTranscriptionRequest(file, { ...options, ...overrides }));
	if (!form) throw new Error('Synthetic fetch did not receive a multipart body');
	return { form, result };
}

test('installed SDK forwards GPT-Transcribe plural hints and preserves file/options/JSON text', async () => {
	const { form, result } = await serialize();
	expect(form.get('model')).toBe('gpt-transcribe');
	expect(form.getAll('languages[]')).toEqual(['fr']);
	expect(form.has('language')).toBe(false);
	expect(form.get('prompt')).toBe(options.prompt);
	expect(form.get('temperature')).toBe('0.2');
	const upload = form.get('file');
	if (!upload || typeof upload === 'string') throw new Error('Missing synthetic audio upload');
	expect(upload.name).toBe('recording.wav');
	// Bun's multipart parser normalizes WAV to the equivalent audio/x-wav alias.
	expect(['audio/wav', 'audio/x-wav']).toContain(upload.type);
	expect(upload.size).toBe(file.size);
	expect(result.text.trim()).toBe('fixture transcript');
	expect('languages' in result).toBe(true);
});

test('installed SDK auto-language request contains no language field or undefined value', async () => {
	const { form } = await serialize({ outputLanguage: 'auto', prompt: '', temperature: '' });
	expect(form.has('language')).toBe(false);
	expect(form.has('languages')).toBe(false);
	expect(form.has('languages[]')).toBe(false);
	expect(form.has('prompt')).toBe(false);
	expect(form.has('temperature')).toBe(false);
});

test.each([...legacy])('installed SDK preserves the existing %s request shape', async modelName => {
	const { form, result } = await serialize({ modelName, outputLanguage: 'en', temperature: '0.0' });
	expect(form.get('model')).toBe(modelName);
	expect(form.get('language')).toBe('en');
	expect(form.has('languages[]')).toBe(false);
	expect(form.get('temperature')).toBe('0');
	expect(result.text.trim()).toBe('fixture transcript');
});
