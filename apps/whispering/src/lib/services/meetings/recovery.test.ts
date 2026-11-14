import { test, expect } from 'bun:test';
import { createMeetingRecovery } from './recovery';
const preview = {
	sourceId: 'source',
	durationMs: 1,
	sourceChunks: 1,
	unregisteredChunks: 0,
	ignoredMixedChunks: 0,
	emptyChunks: 0,
	normalizedHeaders: 0,
	discardedPartialSampleBytes: 0,
	durationMismatches: 0,
	additionalBytesEstimate: 140,
	microphoneChunks: 1,
	systemChunks: 0,
};
const ready = {
	token: 'review-1',
	sourceId: 'source',
	phase: 'ready',
	preview,
	derivedId: null,
	error: null,
};
test('construction/status do not restore or execute; review sends only ID after restoration', async () => {
	const calls: string[] = [];
	const s = createMeetingRecovery(
		async <T>(cmd: string, args?: Record<string, unknown>) => {
			calls.push(cmd);
			if (args) expect(Object.keys(args)).toEqual(['id']);
			return (cmd === 'meeting_recovery_status' ? null : ready) as T;
		},
		async () => {
			calls.push('restore');
		},
	);
	expect(calls).toEqual([]);
	expect((await s.status()).data).toBeNull();
	expect((await s.prepare('source')).data?.token).toBe('review-1');
	expect(calls).toEqual([
		'meeting_recovery_status',
		'restore',
		'meeting_prepare_recovery',
	]);
});
test('missing consent/invalid identifiers dispatch nothing', async () => {
	let calls = 0;
	const s = createMeetingRecovery(async <T>() => {
		calls++;
		return ready as T;
	});
	expect((await s.prepare('../outside')).error).toBeTruthy();
	expect((await s.execute('review-1', false)).error).toBeTruthy();
	expect((await s.cancel('file:///tmp')).error).toBeTruthy();
	expect(calls).toBe(0);
});
test('restoration error never dispatches prepare or execute', async () => {
	let calls = 0;
	const s = createMeetingRecovery(
		async <T>() => {
			calls++;
			return ready as T;
		},
		async () => {
			throw Error('unverified recorder');
		},
	);
	expect((await s.prepare('source')).error).toBeTruthy();
	expect((await s.execute('review-1', true)).error).toBeTruthy();
	expect(calls).toBe(0);
});
test('wrong identity, impossible phase, unsafe counters are rejected', async () => {
	let response: unknown;
	const s = createMeetingRecovery(async <T>() => response as T);
	for (const raw of [
		{ ...ready, sourceId: 'other' },
		{ ...ready, token: '../bad' },
		{ ...ready, phase: 'completed' },
		{ ...ready, preview: { ...preview, durationMs: -1 } },
		{ ...ready, preview: null },
	]) {
		response = raw;
		expect((await s.prepare('source')).error).toBeTruthy();
	}
	response = { ...ready, token: 'review-other' };
	expect((await s.execute('review-1', true)).error).toBeTruthy();
});
test('transport lost execute acknowledgement never retries; status can report native completion', async () => {
	let calls = 0;
	const s = createMeetingRecovery(
		async <T>(cmd: string, args?: Record<string, unknown>) => {
			calls++;
			if (cmd === 'meeting_execute_recovery') {
				expect(args).toEqual({ token: 'review-1', confirmed: true });
				throw Error('lost ack');
			}
			return { ...ready, phase: 'completed', derivedId: 'recovered-1' } as T;
		},
	);
	expect((await s.execute('review-1', true)).error).toBeTruthy();
	expect(calls).toBe(1);
	expect((await s.status()).data?.derivedId).toBe('recovered-1');
	expect(calls).toBe(2);
});
