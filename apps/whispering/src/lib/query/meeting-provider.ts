// Settings are snapshotted at explicit action time, not read by a service.
// Only this adapter receives credentials. Persistence stores a digest, never keys.
import { settings } from '$lib/stores/settings.svelte';
import * as providers from '$lib/services/transcription';
export function meetingProviderConsentKey() {
	const v = settings.value;
	return JSON.stringify([
		v['transcription.selectedTranscriptionService'],
		v['transcription.outputLanguage'],
		v['transcription.prompt'],
		v['transcription.temperature'],
		v['transcription.openai.model'],
		v['transcription.groq.model'],
		v['transcription.mistral.model'],
		v['transcription.deepgram.model'],
		v['transcription.elevenlabs.model'],
		v['transcription.whispercpp.modelPath'],
		v['transcription.parakeet.modelPath'],
		v['transcription.speaches.modelId'],
		v['transcription.speaches.baseUrl'],
		v['apiEndpoints.openai'],
		v['apiEndpoints.groq'],
	]);
}
export function meetingProviderLabel() {
	return settings.value['transcription.selectedTranscriptionService'];
}
export async function pinMeetingProvider() {
	const v = { ...settings.value };
	const service = v['transcription.selectedTranscriptionService'];
	const common = {
		outputLanguage: v['transcription.outputLanguage'],
		prompt: v['transcription.prompt'],
		temperature: v['transcription.temperature'],
	};
	let options: Record<string, unknown>;
	let transcribe;
	switch (service) {
		case 'OpenAI': {
			const o = {
				...common,
				apiKey: v['apiKeys.openai'],
				modelName: v['transcription.openai.model'],
				baseURL: v['apiEndpoints.openai'] || undefined,
			};
			options = o;
			transcribe = (b: Blob) => providers.openai.transcribe(b, o);
			break;
		}
		case 'Groq': {
			const o = {
				...common,
				apiKey: v['apiKeys.groq'],
				modelName: v['transcription.groq.model'],
				baseURL: v['apiEndpoints.groq'] || undefined,
			};
			options = o;
			transcribe = (b: Blob) => providers.groq.transcribe(b, o);
			break;
		}
		case 'ElevenLabs': {
			const o = {
				...common,
				apiKey: v['apiKeys.elevenlabs'],
				modelName: v['transcription.elevenlabs.model'],
			};
			options = o;
			transcribe = (b: Blob) => providers.elevenlabs.transcribe(b, o);
			break;
		}
		case 'Deepgram': {
			const o = {
				...common,
				apiKey: v['apiKeys.deepgram'],
				modelName: v['transcription.deepgram.model'],
			};
			options = o;
			transcribe = (b: Blob) => providers.deepgram.transcribe(b, o);
			break;
		}
		case 'Mistral': {
			const o = {
				...common,
				apiKey: v['apiKeys.mistral'],
				modelName: v['transcription.mistral.model'],
			};
			options = o;
			transcribe = (b: Blob) => providers.mistral.transcribe(b, o);
			break;
		}
		case 'whispercpp': {
			const o = {
				outputLanguage: common.outputLanguage,
				prompt: common.prompt,
				modelPath: v['transcription.whispercpp.modelPath'],
			};
			options = o;
			transcribe = (b: Blob) => providers.whispercpp.transcribe(b, o);
			break;
		}
		case 'parakeet': {
			const o = { modelPath: v['transcription.parakeet.modelPath'] };
			options = o;
			transcribe = (b: Blob) => providers.parakeet.transcribe(b, o);
			break;
		}
		case 'speaches': {
			const o = {
				...common,
				modelId: v['transcription.speaches.modelId'],
				baseUrl: v['transcription.speaches.baseUrl'],
			};
			options = o;
			transcribe = (b: Blob) => providers.speaches.transcribe(b, o);
			break;
		}
		default:
			throw Error('Select a supported transcription provider in settings.');
	}
	const safe = Object.fromEntries(
		Object.entries(options).filter(([key]) => key !== 'apiKey'),
	);
	const digest = await crypto.subtle.digest(
		'SHA-256',
		new TextEncoder().encode(JSON.stringify({ service, options: safe })),
	);
	const configurationId = Array.from(new Uint8Array(digest), (n) =>
		n.toString(16).padStart(2, '0'),
	).join('');
	return { configurationId, providerLabel: service, transcribe };
}
