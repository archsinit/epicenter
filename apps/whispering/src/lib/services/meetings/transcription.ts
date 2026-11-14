import { type } from 'arktype';
import { createTaggedError, extractErrorMessage } from 'wellcrafted/error';
import { Ok, Err, tryAsync, type Result } from 'wellcrafted/result';
import { createMeetingLibraryService, isLocalMeetingId } from './library';
const Audio = type({
	index: 'number',
	offsetMs: 'number',
	durationMs: 'number',
});
const Document = type({
	version: '1',
	id: 'string',
	meetingId: 'string',
	configurationId: 'string',
	providerLabel: 'string',
	audio: Audio.array(),
	segments: type({ index: 'number', text: 'string' }).array(),
});
export type MeetingTranscript = typeof Document.infer;
const { MeetingTranscriptionErr } = createTaggedError(
	'MeetingTranscriptionError',
);
type Request = <T>(
	command: string,
	args?: Record<string, unknown>,
) => Promise<T>;
export type AudioTranscriber = (blob: Blob) => Promise<Result<string, unknown>>;
export type TranscriptionProgress = {
	active: boolean;
	meetingId: string | null;
	completed: number;
	total: number;
	pendingText: string | null;
};
const natural = (n: number) => Number.isSafeInteger(n) && n >= 0;
function decode(raw: unknown, id: string, token?: string) {
	const d = Document(raw);
	if (
		d instanceof type.errors ||
		d.meetingId !== id ||
		!isLocalMeetingId(d.id) ||
		(token && d.id !== token) ||
		!/^[a-f0-9]{64}$/.test(d.configurationId) ||
		d.audio.length === 0 ||
		d.audio.length > 10000 ||
		d.audio.some(
			(a) =>
				!natural(a.index) ||
				!natural(a.offsetMs) ||
				!natural(a.durationMs) ||
				a.durationMs === 0 ||
				a.durationMs > 60000,
		) ||
		new Set(d.audio.map((a) => a.index)).size !== d.audio.length ||
		new Set(d.segments.map((a) => a.index)).size !== d.segments.length ||
		d.segments.some(
			(s) =>
				!d.audio.some((a) => a.index === s.index) || s.text.length > 1048576,
		)
	)
		return MeetingTranscriptionErr({
			message:
				'Invalid local transcript acknowledgement; retained audio is unchanged.',
		});
	return Ok(d);
}
/** Explicit audio-only batch, never ordinary dictation's delivery/cleanup query.
 * Native mixed indices only. No screenshot/directory/manifest enters transcribe(). */
export function createMeetingTranscription(request: Request) {
	const library = createMeetingLibraryService(request);
	let progress: TranscriptionProgress = {
		active: false,
		meetingId: null,
		completed: 0,
		total: 0,
		pendingText: null,
	};
	let stopped = false;
	let pending: {
		meetingId: string;
		token: string;
		index: number;
		text: string;
	} | null = null;
	async function call(cmd: string, args: Record<string, unknown>) {
		return tryAsync({
			try: () => request<unknown>(cmd, args),
			catch: (error) =>
				MeetingTranscriptionErr({ message: extractErrorMessage(error) }),
		});
	}
	async function save(id: string, token: string, index: number, text: string) {
		const r = await call('meeting_transcript_save', { id, token, index, text });
		if (r.error) return Err(r.error);
		return decode(r.data, id, token);
	}
	return {
		progress() {
			return { ...progress };
		},
		async retryPendingSave() {
			if (progress.active || !pending)
				return MeetingTranscriptionErr({
					message: 'No inactive pending local save.',
				});
			const result = await save(
				pending.meetingId,
				pending.token,
				pending.index,
				pending.text,
			);
			if (!result.error) {
				pending = null;
				progress.pendingText = null;
				progress.completed = result.data.segments.length;
			}
			return result;
		},
		pause() {
			stopped = true;
		}, // Does not pretend to abort an in-flight provider request.
		async latest(id: string) {
			if (!isLocalMeetingId(id))
				return MeetingTranscriptionErr({ message: 'Invalid meeting ID.' });
			const r = await call('meeting_transcript_latest', { id });
			if (r.error) return Err(r.error);
			return r.data === null ? Ok(null) : decode(r.data, id);
		},
		async run(
			p: {
				id: string;
				configurationId: string;
				providerLabel: string;
				resume: boolean;
				confirmed: boolean;
			},
			transcribe: AudioTranscriber,
		) {
			if (
				progress.active ||
				p.confirmed !== true ||
				!isLocalMeetingId(p.id) ||
				!/^[a-f0-9]{64}$/.test(p.configurationId)
			)
				return MeetingTranscriptionErr({
					message: 'Confirm one audio-only transcription operation at a time.',
				});
			if (pending)
				return MeetingTranscriptionErr({
					message:
						'Retry the pending local save before starting/resuming provider requests.',
				});
			progress = {
				active: true,
				meetingId: p.id,
				completed: 0,
				total: 0,
				pendingText: null,
			};
			stopped = false;
			return tryAsync({
				try: async () => {
					const start = await call('meeting_transcript_begin', {
						id: p.id,
						configurationId: p.configurationId,
						providerLabel: p.providerLabel,
						resume: p.resume,
					});
					if (start.error) throw Error(start.error.message);
					const parsed = decode(start.data, p.id);
					if (parsed.error) throw Error(parsed.error.message);
					let doc = parsed.data;
					if (doc.configurationId !== p.configurationId)
						throw Error(
							'Local transcript configuration does not match the pinned provider.',
						);
					progress.total = doc.audio.length;
					progress.completed = doc.segments.length;
					for (const chunk of doc.audio) {
						if (stopped) break;
						if (doc.segments.some((s) => s.index === chunk.index)) continue;
						const audio = await library.readMixedAudio(p.id, chunk.index);
						if (audio.error) throw Error(audio.error.message);
						if (audio.data.kind !== 'audio')
							throw Error('Only mixed audio can enter transcription.');
						const bytes = await audio.data.blob.arrayBuffer();
						const h = new DataView(bytes);
						if (
							bytes.byteLength < 46 ||
							bytes.byteLength > 5760044 ||
							h.getUint32(16, true) !== 16 ||
							h.getUint16(20, true) !== 1 ||
							h.getUint16(22, true) !== 1 ||
							h.getUint32(24, true) !== 48000 ||
							h.getUint32(28, true) !== 96000 ||
							h.getUint16(32, true) !== 2 ||
							h.getUint16(34, true) !== 16 ||
							(bytes.byteLength - 44) % 2 !== 0 ||
							new TextDecoder().decode(new Uint8Array(bytes, 36, 4)) !==
								'data' ||
							h.getUint32(40, true) + 44 !== bytes.byteLength ||
							Math.ceil((bytes.byteLength - 44) / 96) !== chunk.durationMs
						)
							throw Error(
								'Meeting audio must be finalized native mono 48kHz PCM16, at most one minute per request.',
							);
						if (stopped) break;
						const result = await transcribe(audio.data.blob);
						if (result.error) {
							const failure = result.error;
							const message =
								typeof failure === 'object' &&
								failure !== null &&
								'description' in failure &&
								typeof failure.description === 'string'
									? failure.description
									: extractErrorMessage(failure);
							throw Error(message);
						}
						if (typeof result.data !== 'string' || result.data.length > 1048576)
							throw Error('Invalid/oversized provider transcript.');
						pending = {
							meetingId: p.id,
							token: doc.id,
							index: chunk.index,
							text: result.data,
						};
						progress.pendingText = result.data;
						const saved = await save(p.id, doc.id, chunk.index, result.data);
						if (saved.error) throw Error(saved.error.message);
						doc = saved.data;
						pending = null;
						progress.pendingText = null;
						progress.completed = doc.segments.length;
					}
					return doc;
				},
				catch: (error) =>
					MeetingTranscriptionErr({ message: extractErrorMessage(error) }),
			}).finally(() => {
				progress.active = false;
			});
		},
	};
}
