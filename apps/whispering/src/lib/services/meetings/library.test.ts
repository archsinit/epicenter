import { describe, expect, test } from 'bun:test';
import { createMeetingLibraryService, createLocalPreviewUrl, formatMeetingOffset } from './library';

const png = [137, 80, 78, 71, 13, 10, 26, 10, 0];
function wav() {
	const bytes = new Uint8Array(46); const view = new DataView(bytes.buffer);
	for (const [offset, text] of [[0, 'RIFF'], [8, 'WAVE'], [12, 'fmt '], [36, 'data']] as const)
		bytes.set([...text].map((c) => c.charCodeAt(0)), offset);
	view.setUint32(4, bytes.length - 8, true); view.setUint32(16, 16, true);
	view.setUint16(20, 1, true); view.setUint16(22, 1, true);
	view.setUint32(24, 48000, true); view.setUint32(28, 96000, true);
	view.setUint16(32, 2, true); view.setUint16(34, 16, true); view.setUint32(40, 2, true);
	return bytes;
}
function fixture() {
	const calls: { command: string; args?: Record<string, unknown> }[] = [];
	let response: unknown;
	const service = createMeetingLibraryService(async <T>(command: string, args?: Record<string, unknown>) => {
		calls.push({ command, args }); return response as T;
	});
	return { service, calls, set response(value: unknown) { response = value; } };
}
const entry = { id: 'meeting-1', title: 'Local', state: 'completed', startedAtUnixMs: 100, durationMs: 1000, audioChunks: 1, screenshots: 1, error: null };
function detail() {
	return {
		state: 'completed', unregisteredAssets: 0,
		manifest: {
			version: 1, id: 'meeting-1', title: 'Local', mode: 'microphone', microphoneId: 'fixture', displayId: 1,
			startedAtUnixMs: 100, durationMs: 1000, status: 'completed',
			audio: [{ path: 'audio/00000000.wav', track: 'mixed', offsetMs: 0, durationMs: 1000 }],
			screenshots: [{ path: 'screenshots/00000000.png', origin: 'manual', offsetMs: 500 }],
		},
	};
}
describe('meeting library client (synthetic IPC, no providers/devices)', () => {
	test('lists unfinished and damaged entries without write/capture/recovery requests', async () => {
		const f = fixture();
		f.response = { entries: [entry, { ...entry, id: 'bad name', state: 'unavailable', error: 'metadata damaged' }], truncated: true, nextCursor: 'meeting-1' };
		const result = await f.service.list();
		expect(result.data?.entries).toHaveLength(2); expect(result.data?.truncated).toBe(true);
		expect(f.calls).toEqual([{ command: 'meeting_list_saved', args: undefined }]);
	});
	test('paging uses an opaque advancing cursor and refuses malformed/non-advancing acknowledgements',async()=>{
        const f=fixture();
        f.response={entries:[],truncated:true,nextCursor:'z'};
        expect((await f.service.list('a')).error).toBeNull();
        expect(f.calls.at(-1)).toEqual({command:'meeting_list_saved',args:{after:'a'}});
        for(const nextCursor of ['a','0',null]){f.response={entries:[],truncated:true,nextCursor};expect((await f.service.list('a')).error).toBeTruthy();}
    });
	test('bad schemas, unsafe numeric counters and oversized lists are rejected', async () => {
		const f = fixture();
		for (const response of [null, { entries: [{ ...entry, screenshots: -1 }], truncated: false }, { entries: [{ ...entry, audioChunks: 1.5 }], truncated: false }, { entries: Array(501).fill(entry), truncated: true }]) {
			f.response = response; expect((await f.service.list()).error).toBeTruthy();
		}
	});
	test('detail accepts only matching local IDs and typed audio/image paths', async () => {
		const f = fixture(); f.response = detail();
		expect((await f.service.detail('meeting-1')).error).toBeNull();
		expect(f.calls.at(-1)).toEqual({ command: 'meeting_saved_detail', args: { id: 'meeting-1' } });
		const wrong = detail(); wrong.manifest.id = 'other'; f.response = wrong;
		expect((await f.service.detail('meeting-1')).error).toBeTruthy();
		const swapped = detail(); swapped.manifest.audio[0]!.path = 'screenshots/00000000.png'; f.response = swapped;
		expect((await f.service.detail('meeting-1')).error).toBeTruthy();
		const escaped = detail(); escaped.manifest.screenshots[0]!.path = 'https://not-local/image.png'; f.response = escaped;
		expect((await f.service.detail('meeting-1')).error).toBeTruthy();
	});
	test('invalid IDs/indexes cause no native request', async () => {
		const f = fixture();
		expect((await f.service.detail('../outside')).error).toBeTruthy();
		for (const index of [-1, 0.5, Number.NaN, Number.MAX_SAFE_INTEGER + 1]) {
			expect((await f.service.readMixedAudio('meeting-1', index)).error).toBeTruthy();
			expect((await f.service.readScreenshot('meeting-1', index)).error).toBeTruthy();
		}
		expect(f.calls).toHaveLength(0);
	});
	test('Finder reveal is explicit, passes only an ID and accepts only a matching acknowledgement', async () => {
        const f = fixture();
        expect(f.calls).toHaveLength(0);
        f.response = { id: 'meeting-1' };
        expect((await f.service.revealFolder('meeting-1')).data).toEqual({ id: 'meeting-1' });
        expect(f.calls).toEqual([{ command: 'meeting_reveal_folder', args: { id: 'meeting-1' } }]);
        for (const response of [null, undefined, true, '/local/folder', { id: 'other' }, { id: 'meeting-1', path: '/private' }]) {
            f.response = response;
            expect((await f.service.revealFolder('meeting-1')).error).toBeTruthy();
        }
    });
    test('invalid reveal selections never dispatch, and native errors do not retry or fall back', async () => {
        const f = fixture();
        for (const id of ['', '../outside', 'file:///tmp', '/tmp', 'bad name', 'x'.repeat(97)]) {
            expect((await f.service.revealFolder(id)).error).toBeTruthy();
        }
        expect(f.calls).toHaveLength(0);
        let calls = 0;
        const service = createMeetingLibraryService(async () => { calls++; throw new Error('synthetic Finder permission denial'); });
        const result = await service.revealFolder('meeting-1');
        expect(result.error?.message).toContain('synthetic Finder permission denial');
        expect(calls).toBe(1);
    });
	test('audio and screenshots use distinct native commands and distinct local result types', async () => {
		const f = fixture();
		f.response = wav().buffer;
		const audio = await f.service.readMixedAudio('meeting-1', 0);
		expect(audio.data?.kind).toBe('audio'); expect(audio.data?.blob.type).toBe('audio/wav');
		f.response = png;
		const screenshot = await f.service.readScreenshot('meeting-1', 0);
		expect(screenshot.data?.kind).toBe('local-screenshot'); expect(screenshot.data?.blob.type).toBe('image/png');
		expect(screenshot.data).toMatchObject({ localOnly: true });
		expect(f.calls.map((c) => c.command)).toEqual(['meeting_read_mixed_audio', 'meeting_read_screenshot']);
		expect(f.calls.every((c) => Object.keys(c.args!).join(',') === 'id,index')).toBe(true);
	});
	test('PNG cannot be returned as audio; WAV cannot be returned as a screenshot', async () => {
		const f = fixture(); f.response = png;
		expect((await f.service.readMixedAudio('meeting-1', 0)).error).toBeTruthy();
		f.response = Array.from(wav());
		expect((await f.service.readScreenshot('meeting-1', 0)).error).toBeTruthy();
		f.response = 'https://not-local/asset.png';
		expect((await f.service.readScreenshot('meeting-1', 0)).error).toBeTruthy();
	});
	test('unfinished WAV headers, invalid binary bytes and oversized previews fail closed', async () => {
		const f = fixture(); const bytes = wav(); new DataView(bytes.buffer).setUint32(4, 0, true); f.response = bytes.buffer;
		expect((await f.service.readMixedAudio('meeting-1', 0)).error).toBeTruthy();
		for (const raw of [[137, 256, -1], new ArrayBuffer(17 * 1024 * 1024)]) {
			f.response = raw; expect((await f.service.readMixedAudio('meeting-1', 0)).error).toBeTruthy();
		}
	});
	test('local blob URL is revoked exactly once without using any manifest path', async () => {
		const f = fixture(); f.response = png; const screenshot = await f.service.readScreenshot('meeting-1', 0);
		const revoked: string[] = []; let created = 0;
		const resource = createLocalPreviewUrl(screenshot.data!, {
			createObjectURL: (blob) => { expect(blob).toBe(screenshot.data!.blob); created++; return 'blob:local-fixture'; },
			revokeObjectURL: (url) => { revoked.push(url); },
		});
		expect(resource.url).toBe('blob:local-fixture'); resource.close(); resource.close();
		expect(created).toBe(1); expect(revoked).toEqual(['blob:local-fixture']);
	});
	test('offset labels preserve five-minute boundaries and hours without treating milliseconds as seconds', () => {
		expect(formatMeetingOffset(0)).toBe('00:00:00'); expect(formatMeetingOffset(299999)).toBe('00:04:59');
		expect(formatMeetingOffset(300000)).toBe('00:05:00'); expect(formatMeetingOffset(3661000)).toBe('01:01:01');
		expect(formatMeetingOffset(-1)).toBe('Unknown');
	});
});
