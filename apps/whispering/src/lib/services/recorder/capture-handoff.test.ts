import { describe, expect, test } from 'bun:test';
import { Ok } from 'wellcrafted/result';
import { createNavigatorRecorderCore } from './navigator-core';
import { createOwnedRecordingProcessClient } from './owned-process';
import type { NavigatorRecordingParams } from './types';
import { asDeviceIdentifier } from '../types';

const params: NavigatorRecordingParams = {
	method: 'navigator',
	selectedDeviceId: null,
	recordingId: 'fixture',
	bitrateKbps: '64',
};
const callbacks = { sendStatus: () => {} };
const outcome = {
	outcome: 'success' as const,
	deviceId: asDeviceIdentifier('fixture'),
};
const stream = {} as MediaStream;
class Recorder extends EventTarget {
	state: RecordingState = 'inactive';
	mimeType = 'audio/webm';
	failStart = false;
	failStop = false;
	start() {
		if (this.failStart) throw new Error('fixture start failure');
		this.state = 'recording';
	}
	stop() {
		if (this.failStop) throw new Error('fixture stop failure');
		this.state = 'inactive';
		this.dispatchEvent(
			Object.assign(new Event('dataavailable'), {
				data: new Blob(['fixture audio']),
			}),
		);
		this.dispatchEvent(new Event('stop'));
	}
}
function fixture() {
	const flags = {
		live: false,
		failCleanup: false,
		failConstructor: false,
		cleanups: 0,
	};
	const recorder = new Recorder();
	const deps = {
		acquireStream: async () => {
			flags.live = true;
			return Ok({ stream, deviceOutcome: outcome });
		},
		enumerateDevices: async () => Ok([]),
		cleanupStream: () => {
			flags.cleanups++;
			if (flags.failCleanup) throw new Error('fixture track cleanup failure');
			flags.live = false;
		},
		createRecorder: () => {
			if (flags.failConstructor) throw new Error('fixture constructor failure');
			return recorder as unknown as MediaRecorder;
		},
		timesliceMs: 100,
	};
	return { flags, recorder, deps, service: createNavigatorRecorderCore(deps) };
}
describe('Navigator hardware handoff, synthetic stream/recorder', () => {
	test('failed recorder construction closes acquired tracks', async () => {
		const f = fixture();
		f.flags.failConstructor = true;
		expect(
			(await f.service.startRecording(params, callbacks)).error,
		).toBeTruthy();
		expect(f.flags.live).toBe(false);
		expect((await f.service.getRecorderState()).data).toBe('IDLE');
	});
	test('failed start closes acquired tracks', async () => {
		const f = fixture();
		f.recorder.failStart = true;
		expect(
			(await f.service.startRecording(params, callbacks)).error,
		).toBeTruthy();
		expect(f.flags.live).toBe(false);
	});
	test('failed constructor cleanup retains stream identity until Cancel really closes it', async () => {
		const f = fixture();
		f.flags.failConstructor = true;
		f.flags.failCleanup = true;
		await f.service.startRecording(params, callbacks);
		expect((await f.service.getRecorderState()).data).toBe('RECORDING');
		expect(f.flags.live).toBe(true);
		expect((await f.service.cancelRecording(callbacks)).error).toBeTruthy();
		f.flags.failCleanup = false;
		expect((await f.service.cancelRecording(callbacks)).error).toBeNull();
		expect(f.flags.live).toBe(false);
	});
	test('Stop returns audio only after track cleanup and preserves its MIME', async () => {
		const f = fixture();
		await f.service.startRecording(params, callbacks);
		const result = await f.service.stopRecording(callbacks);
		expect(result.error).toBeNull();
		expect(await result.data!.text()).toBe('fixture audio');
		expect(result.data!.type).toBe('audio/webm');
		expect(f.flags.live).toBe(false);
	});
	test('failed Stop confirmation still attempts track cleanup', async () => {
		const f = fixture();
		await f.service.startRecording(params, callbacks);
		f.recorder.failStop = true;
		expect((await f.service.stopRecording(callbacks)).error).toBeTruthy();
		expect(f.flags.live).toBe(false);
	});
	test('failed cleanup keeps stopped recording chunks available for retry', async () => {
		const f = fixture();
		await f.service.startRecording(params, callbacks);
		f.flags.failCleanup = true;
		expect((await f.service.stopRecording(callbacks)).error).toBeTruthy();
		expect((await f.service.getRecorderState()).data).toBe('RECORDING');
		f.flags.failCleanup = false;
		const result = await f.service.stopRecording(callbacks);
		expect(await result.data!.text()).toBe('fixture audio');
		expect(f.flags.live).toBe(false);
	});
	test('temporary cleanup failure also preserves already stopped audio for retry', async () => {
		const f = fixture();
		let pendingFailure = true;
		const service = createNavigatorRecorderCore({
			...f.deps,
			cleanupPendingStreams: () => {
				if (pendingFailure) throw new Error('temporary stream cleanup failed');
			},
		});
		await service.startRecording(params, callbacks);
		expect((await service.stopRecording(callbacks)).error).toBeTruthy();
		expect(f.flags.live).toBe(false);
		expect((await service.getRecorderState()).data).toBe('RECORDING');
		pendingFailure = false;
		const result = await service.stopRecording(callbacks);
		expect(await result.data!.text()).toBe('fixture audio');
		expect((await service.getRecorderState()).data).toBe('IDLE');
	});
	test('Cancel closes tracks even when recorder.stop throws', async () => {
		const f = fixture();
		await f.service.startRecording(params, callbacks);
		f.recorder.failStop = true;
		expect((await f.service.cancelRecording(callbacks)).error).toBeTruthy();
		expect(f.flags.live).toBe(false);
		expect((await f.service.cancelRecording(callbacks)).data?.status).toBe(
			'no-recording',
		);
	});
	test('pending acquisition cannot be mistaken for IDLE or cancelled successfully', async () => {
		const f = fixture();
		let resolve!: () => void;
		const gate = new Promise<void>((done) => {
			resolve = done;
		});
		f.deps.acquireStream = async () => {
			await gate;
			f.flags.live = true;
			return Ok({ stream, deviceOutcome: outcome });
		};
		const service = createNavigatorRecorderCore(f.deps);
		const starting = service.startRecording(params, callbacks);
		expect((await service.getRecorderState()).data).toBe('RECORDING');
		expect((await service.cancelRecording(callbacks)).error).toBeTruthy();
		resolve();
		await starting;
		await service.cancelRecording(callbacks);
		expect(f.flags.live).toBe(false);
	});
});

const closed = {
	captureId: 'ffmpeg-fixture',
	pid: 123,
	running: false,
	forced: false,
	exitCode: 0,
};
describe('owned-process acknowledgment boundary, fake IPC only', () => {
	test('Stop uses an opaque native ID, never a saved PID', async () => {
		const calls: unknown[] = [];
		const client = createOwnedRecordingProcessClient(
			async <T>(command: string, args?: Record<string, unknown>) => {
				calls.push([command, args]);
				return closed as T;
			},
		);
		expect((await client.stop(closed.captureId)).error).toBeNull();
		expect(calls).toEqual([
			['capture_ffmpeg_stop', { captureId: closed.captureId }],
		]);
	});
	test('running, mismatched or malformed acknowledgments cannot release capture', async () => {
		for (const info of [
			{ ...closed, running: true },
			{ ...closed, captureId: 'ffmpeg-other' },
			{ pid: 123 },
		]) {
			const client = createOwnedRecordingProcessClient(
				async <T>() => info as T,
			);
			expect((await client.stop(closed.captureId)).error).toBeTruthy();
		}
	});
	test('Stop waits for native exit acknowledgment, not a timer/file-size guess', async () => {
		let resolve!: (value: unknown) => void;
		const gate = new Promise<unknown>((done) => {
			resolve = done;
		});
		const client = createOwnedRecordingProcessClient(
			async <T>() => (await gate) as T,
		);
		let finished = false;
		const stopping = client.stop(closed.captureId).then((result) => {
			finished = true;
			return result;
		});
		await Promise.resolve();
		expect(finished).toBe(false);
		resolve(closed);
		expect((await stopping).error).toBeNull();
	});
	test('legacy restore never submits a saved PID to stop/kill', async () => {
		const calls: unknown[] = [];
		const client = createOwnedRecordingProcessClient(
			async <T>(command: string, args?: Record<string, unknown>) => {
				calls.push([command, args]);
				throw new Error('unverified session');
			},
		);
		expect((await client.restore()).error).toBeTruthy();
		expect(calls).toEqual([['capture_ffmpeg_restore', { captureId: null }]]);
	});
	test('a finalized-session receipt can restore closed state without a capture', async () => {
		const client = createOwnedRecordingProcessClient(
			async <T>() => closed as T,
		);
		expect((await client.restore(closed.captureId)).data?.running).toBe(false);
	});
});
