import type OpenAI from 'openai';

export const OPENAI_TRANSCRIPTION_MODELS = [
	{
		name: 'whisper-1',
		description:
			"OpenAI's flagship speech-to-text model with multilingual support. Reliable and accurate transcription for a wide variety of use cases.",
		cost: '$0.36/hour',
	},
	{
		name: 'gpt-4o-transcribe',
		description:
			'GPT-4o powered transcription with enhanced understanding and context. Best for complex audio requiring deep comprehension.',
		cost: '$0.36/hour',
	},
	{
		name: 'gpt-4o-mini-transcribe',
		description:
			'Cost-effective GPT-4o mini transcription model. Good balance of performance and cost for standard transcription needs.',
		cost: '$0.18/hour',
	},
	{
		name: 'gpt-transcribe',
		description:
			"OpenAI's current recorded-audio transcription model with context and language hints.",
		cost: '$0.27/hour',
	},
] as const satisfies {
	// The installed SDK predates this model; its transport already accepts string IDs.
	name: OpenAI.Audio.AudioModel | 'gpt-transcribe';
	description: string;
	cost: string;
}[];

export type OpenAIModel = (typeof OPENAI_TRANSCRIPTION_MODELS)[number];

type RequestOptions = {
	modelName: string;
	outputLanguage: string;
	prompt: string;
	temperature: string;
};

type TranscriptionRequest = OpenAI.Audio.TranscriptionCreateParamsNonStreaming<'json' | undefined> & {
	// Supported API field missing from SDK 5.23.2's generated request types.
	languages?: string[];
};

export function buildOpenaiTranscriptionRequest(file: File, options: RequestOptions): TranscriptionRequest {
	const languageHint = options.outputLanguage === 'auto'
		? {}
		: options.modelName === 'gpt-transcribe'
			? { languages: [options.outputLanguage] }
			: { language: options.outputLanguage };
	return {
		file,
		model: options.modelName,
		...languageHint,
		prompt: options.prompt || undefined,
		temperature: options.temperature ? Number.parseFloat(options.temperature) : undefined,
	};
}
