import { test, expect } from 'bun:test';
import { Ok, Err } from 'wellcrafted/result';
import { createMeetingTranscription } from './transcription';
function wav() {
	const b = new Uint8Array(140),
		v = new DataView(b.buffer);
	for (const [o, s] of [
		[0, 'RIFF'],
		[8, 'WAVE'],
		[12, 'fmt '],
		[36, 'data'],
	] as const)
		b.set(
			[...s].map((c) => c.charCodeAt(0)),
			o,
		);
	v.setUint32(4, 132, true);
	v.setUint32(16, 16, true);
	v.setUint16(20, 1, true);
	v.setUint16(22, 1, true);
	v.setUint32(24, 48000, true);
	v.setUint32(28, 96000, true);
	v.setUint16(32, 2, true);
	v.setUint16(34, 16, true);
	v.setUint32(40, 96, true);
	return b.buffer;
}
function fixture() {
	let doc = {
		version: 1,
		id: 'transcript-1',
		meetingId: 'source',
		configurationId: 'a'.repeat(64),
		providerLabel: 'OpenAI',
		audio: [
			{ index: 0, offsetMs: 0, durationMs: 1 },
			{ index: 2, offsetMs: 60000, durationMs: 1 },
		],
		segments: [] as { index: number; text: string }[],
	};
	let failSave = false;
	let badAudio = false;
	const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
	const service = createMeetingTranscription(
		async <T>(cmd: string, args?: Record<string, unknown>) => {
			calls.push({ cmd, args });
			if (cmd === 'meeting_read_mixed_audio')
				return (
					badAudio ? new Uint8Array([137, 80, 78, 71]).buffer : wav()
				) as T;
			if (cmd === 'meeting_transcript_save') {
				if (failSave) throw Error('disk denied');
				if (!doc.segments.some((s) => s.index === args!.index))
					doc.segments.push({
						index: args!.index as number,
						text: args!.text as string,
					});
			}
			return structuredClone(doc) as T;
		},
	);
	return {
		service,
		calls,
		get doc() {
			return doc;
		},
		set failSave(v: boolean) {
			failSave = v;
		},
		set badAudio(v: boolean) {
			badAudio = v;
		},
	};
}
const params = {
	id: 'source',
	configurationId: 'a'.repeat(64),
	providerLabel: 'OpenAI',
	resume: false,
	confirmed: true,
};
test('explicit batch sends Blob audio only, sequentially checkpoints; no images/manifest/provider settings payloads', async () => {
	const f = fixture();
	let requests = 0;
	expect(f.calls).toEqual([]);
	const r = await f.service.run(params, async (blob) => {
		expect(blob instanceof Blob).toBe(true);
		expect(blob.type).toBe('audio/wav');
		requests++;
		return Ok('text ' + requests);
	});
	expect(r.error).toBeNull();
	expect(r.data?.segments).toHaveLength(2);
	expect(requests).toBe(2);
	expect(f.calls.map((c) => c.cmd)).toEqual([
		'meeting_transcript_begin',
		'meeting_read_mixed_audio',
		'meeting_transcript_save',
		'meeting_read_mixed_audio',
		'meeting_transcript_save',
	]);
	expect(f.service.progress().active).toBe(false);
});
test('unconfirmed/invalid IDs do not dispatch or call provider', async () => {
	const f = fixture();
	const provider = async () => {
		throw Error('must not call');
	};
	expect(
		(await f.service.run({ ...params, confirmed: false }, provider)).error,
	).toBeTruthy();
	expect(
		(await f.service.run({ ...params, id: '../outside' }, provider)).error,
	).toBeTruthy();
	expect(f.calls).toHaveLength(0);
});
test('provider failure preserves prior text and explicit resume skips saved chunks', async () => {
	const f = fixture();
	let calls = 0;
	const r = await f.service.run(params, async () => {
		calls++;
		return calls === 1 ? Ok('saved first') : Err(Error('provider denied'));
	});
	expect(r.error).toBeTruthy();
	expect(f.doc.segments).toHaveLength(1);
	let resumed = 0;
	expect(
		(
			await f.service.run({ ...params, resume: true }, async () => {
				resumed++;
				return Ok('remaining');
			})
		).error,
	).toBeNull();
	expect(resumed).toBe(1);
	expect(f.doc.segments).toHaveLength(2);
});
test('local save failure retains response; retry-save never calls provider again or starts new batch', async () => {
	const f = fixture();
	f.failSave = true;
	let calls = 0;
	expect(
		(
			await f.service.run(params, async () => {
				calls++;
				return Ok('retained response');
			})
		).error,
	).toBeTruthy();
	expect(f.service.progress().pendingText).toBe('retained response');
	const n = f.calls.length;
	expect(
		(await f.service.run(params, async () => Ok('must not call'))).error,
	).toBeTruthy();
	expect(f.calls.length).toBe(n);
	f.failSave = false;
	expect((await f.service.retryPendingSave()).error).toBeNull();
	expect(calls).toBe(1);
	expect(f.doc.segments[0]?.text).toBe('retained response');
	expect(f.service.progress().pendingText).toBeNull();
});
test('pause retains current request result but does not request another chunk', async () => {
	const f = fixture();
	let calls = 0;
	const r = await f.service.run(params, async () => {
		calls++;
		f.service.pause();
		return Ok('one');
	});
	expect(r.error).toBeNull();
	expect(calls).toBe(1);
	expect(f.doc.segments).toHaveLength(1);
});
test('adapter permission/credential descriptions propagate without fallback or retry', async () => {
	const f = fixture();
	let calls = 0;
	const result = await f.service.run(params, async () => {
		calls++;
		return Err({ description: 'Permission denied by selected provider' });
	});
	expect(result.error?.message).toContain('Permission denied');
	expect(calls).toBe(1);
	expect(f.doc.segments).toHaveLength(0);
});
test('PNG substitution never reaches provider', async () => {
	const f = fixture();
	f.badAudio = true;
	let calls = 0;
	expect(
		(
			await f.service.run(params, async () => {
				calls++;
				return Ok('bad');
			})
		).error,
	).toBeTruthy();
	expect(calls).toBe(0);
});
