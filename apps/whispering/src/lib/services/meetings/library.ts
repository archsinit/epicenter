import { type } from 'arktype';
import { createTaggedError, extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, tryAsync, trySync } from 'wellcrafted/result';

const State = type("'active' | 'completed' | 'interrupted' | 'unfinished' | 'unavailable'");
const Audio = type({ path: 'string', track: "'mixed' | 'microphone' | 'system'", offsetMs: 'number', durationMs: 'number' });
const Screenshot = type({ path: 'string', origin: "'automatic' | 'manual'", offsetMs: 'number' });
const Manifest = type({
	version: '1', id: 'string', title: 'string',
	mode: "'microphone' | 'system-and-microphone'", microphoneId: 'string', displayId: 'number | null',
	startedAtUnixMs: 'number', durationMs: 'number | null', status: "'recording' | 'completed' | 'interrupted'",
	audio: Audio.array(), screenshots: Screenshot.array(),
});
const Library = type({
	entries: type({ id: 'string', title: 'string', state: State, startedAtUnixMs: 'number | null', durationMs: 'number | null', audioChunks: 'number', screenshots: 'number', error: 'string | null' }).array(),
	truncated: 'boolean',
	'nextCursor?': 'string | null',
});
const Detail = type({ manifest: Manifest, state: State, unregisteredAssets: 'number' });
export type MeetingLibrary = typeof Library.infer;
export type MeetingDetail = typeof Detail.infer;
export type MeetingPreview =
	| { kind: 'audio'; blob: Blob }
	| { kind: 'local-screenshot'; localOnly: true; blob: Blob };
const { MeetingLibraryServiceErr } = createTaggedError('MeetingLibraryServiceError');
type Request = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export function isLocalMeetingId(id: string) { return /^[a-zA-Z0-9-]{1,96}$/.test(id); }
const natural = (n: number) => Number.isSafeInteger(n) && n >= 0;
const nullableNatural = (n: number | null) => n === null || natural(n);
const invalid = (message = 'Invalid local meeting-library response; files were retained.') => MeetingLibraryServiceErr({ message });
const PNG = [137, 80, 78, 71, 13, 10, 26, 10];

/** Local-only browsing and explicit folder reveal. No provider, capture,
 * settings, clipboard or LLM import.
 * Screenshot objects are explicitly local-only and never returned as audio.
 */
export function createMeetingLibraryService(request: Request) {
	async function read(command: string, args?: Record<string, unknown>) {
		return tryAsync({
			try: () => request<unknown>(command, args),
			catch: (error) => MeetingLibraryServiceErr({ message: extractErrorMessage(error) }),
		});
	}
	async function asset(id: string, index: number, kind: 'audio' | 'local-screenshot') {
		if (!isLocalMeetingId(id) || !natural(index)) return invalid('Invalid meeting/asset selection.');
		const maximum = (kind === 'audio' ? 16 : 64) * 1024 * 1024;
		const result = await read(kind === 'audio' ? 'meeting_read_mixed_audio' : 'meeting_read_screenshot', { id, index });
		if (result.error) return Err(result.error);
		return trySync({
			try: () => {
				const raw = result.data;
				let bytes: Uint8Array;
				if (raw instanceof ArrayBuffer && raw.byteLength <= maximum) bytes = new Uint8Array(raw);
				else if (Array.isArray(raw) && raw.length <= maximum && raw.every((b) => Number.isInteger(b) && b >= 0 && b <= 255)) bytes = Uint8Array.from(raw);
				else throw new Error('Invalid or oversized local preview response.');
				if (kind === 'audio') {
					if (bytes.length < 44 || String.fromCharCode(...bytes.subarray(0, 4)) !== 'RIFF' || String.fromCharCode(...bytes.subarray(8, 12)) !== 'WAVE'
						|| new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(4, true) + 8 !== bytes.length)
						throw new Error('Local audio preview must be a finalized WAV, never a screenshot.');
					return { kind: 'audio', blob: new Blob([Uint8Array.from(bytes)], { type: 'audio/wav' }) } satisfies MeetingPreview;
				}
				if (!PNG.every((b, i) => bytes[i] === b)) throw new Error('Local screenshot preview must be PNG data.');
				return { kind: 'local-screenshot', localOnly: true, blob: new Blob([Uint8Array.from(bytes)], { type: 'image/png' }) } satisfies MeetingPreview;
			},
			catch: (error) => MeetingLibraryServiceErr({ message: extractErrorMessage(error) }),
		});
	}
	return {
		async list(after?: string) {
			if(after !== undefined && (after.length>1024 || after.includes('\0')))return invalid('Invalid paging cursor.');
			const result = await read('meeting_list_saved', after===undefined?undefined:{after});
			if (result.error) return Err(result.error);
			const library = Library(result.data);
			if (library instanceof type.errors || library.entries.length > 500 || library.entries.some((e) => !natural(e.audioChunks) || !natural(e.screenshots) || !nullableNatural(e.startedAtUnixMs) || !nullableNatural(e.durationMs))) return invalid();
			if ((library.truncated && !library.nextCursor) || (library.nextCursor != null && (library.nextCursor.length>1024 || library.nextCursor.includes('\0') || (after!==undefined && library.nextCursor<=after)))) return invalid('Invalid/non-advancing page cursor.');
			return Ok(library);
		},
		async detail(id: string) {
			if (!isLocalMeetingId(id)) return invalid('Invalid meeting selection.');
			const result = await read('meeting_saved_detail', { id });
			if (result.error) return Err(result.error);
			const detail = Detail(result.data);
			if (detail instanceof type.errors) return invalid();
			const m = detail.manifest;
			if (m.id !== id || !natural(detail.unregisteredAssets) || !natural(m.startedAtUnixMs) || !nullableNatural(m.durationMs) || !nullableNatural(m.displayId)
				|| m.audio.some((a) => !/^audio\/[0-9]+\.wav$/.test(a.path) || !natural(a.offsetMs) || !natural(a.durationMs) || a.durationMs === 0)
				|| m.screenshots.some((a) => !/^screenshots\/[0-9]+\.png$/.test(a.path) || !natural(a.offsetMs))) return invalid();
			return Ok(detail);
		},
		/** Explicit local OS action. The native side owns all path resolution. */
        async revealFolder(id: string) {
            if (!isLocalMeetingId(id)) return invalid('Invalid meeting selection.');
            const result = await read('meeting_reveal_folder', { id });
            if (result.error) return Err(result.error);
            const ack = type({ id: 'string' })(result.data);
            if (ack instanceof type.errors || ack.id !== id || Object.keys(ack).length !== 1)
                return invalid('Finder request acknowledgement did not match this meeting.');
            return Ok({ id });
        },
		readMixedAudio: (id: string, index: number) => asset(id, index, 'audio'),
		readScreenshot: (id: string, index: number) => asset(id, index, 'local-screenshot'),
	};
}

export function formatMeetingOffset(milliseconds: number) {
	if (!natural(milliseconds)) return 'Unknown';
	const seconds = Math.floor(milliseconds / 1000);
	return [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60].map((n) => String(n).padStart(2, '0')).join(':');
}

/** Blob URLs only, with explicit disposal on selection change/unmount. Never
 * use a manifest path or remote URL as an image/audio source.
 */
export function createLocalPreviewUrl(preview: MeetingPreview, urls: Pick<typeof URL, 'createObjectURL' | 'revokeObjectURL'> = URL) {
	const url = urls.createObjectURL(preview.blob);
	let closed = false;
	return { url, close() { if (!closed) { closed = true; urls.revokeObjectURL(url); } } };
}
