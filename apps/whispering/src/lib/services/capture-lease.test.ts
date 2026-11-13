import { describe, expect, test } from 'bun:test';
import { Ok } from 'wellcrafted/result';
import { createCaptureLease } from './capture-lease';
import { createLazyRecorderService, createLeasedRecorderService } from './recorder/leased';
import {
	RecorderServiceErr,
	type RecorderService,
	type StartRecordingParams,
} from './recorder/types';
import { asDeviceIdentifier } from './types';

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => {
		resolve = done;
	});
	return { promise, resolve };
}
const params: StartRecordingParams = {
	method: 'navigator',
	recordingId: 'fixture',
	selectedDeviceId: null,
	bitrateKbps: '64',
};
const callbacks = { sendStatus: () => {} };
const outcome = {
	outcome: 'success' as const,
	deviceId: asDeviceIdentifier('fixture'),
};
function fixture() {
	const calls: string[] = [];
	let releaseFailures = 0;
	const lease = createCaptureLease({
		enabled: true,
		request: async <T>(command: string) => {
			calls.push(command);
			if (command === 'capture_release_dictation' && releaseFailures-- > 0)
				throw new Error('fixture release unavailable');
			return (
				command === 'capture_acquire_dictation' ? 'lease-fixture' : undefined
			) as T;
		},
	});
	const blob = new Blob(['synthetic audio'], { type: 'audio/wav' });
	const counts = { starts: 0, cancels: 0 };
	let active = false;
	const base: RecorderService = {
		enumerateDevices: async () => Ok([]),
		getRecorderState: async () => Ok(active ? 'RECORDING' : 'IDLE'),
		startRecording: async () => {
			counts.starts++;
			active = true;
			return Ok(outcome);
		},
		stopRecording: async () => {
			active = false;
			return Ok(blob);
		},
		cancelRecording: async () => {
			counts.cancels++;
			active = false;
			return Ok({ status: 'cancelled' });
		},
	};
	return {
		calls,
		lease,
		base,
		blob,
		counts,
		failRelease: () => {
			releaseFailures++;
		},
		setActive: () => {
			active = true;
		},
	};
}

describe('native capture lease', () => {
	test('pending admission cannot be duplicated or released early', async () => {
		const admitted = deferred<string>();
		const calls: string[] = [];
		const lease = createCaptureLease({
			enabled: true,
			request: async <T>(command: string) => {
				calls.push(command);
				return (
					command === 'capture_acquire_dictation'
						? await admitted.promise
						: undefined
				) as T;
			},
		});
		const starting = lease.acquire();
		expect(lease.held).toBe(true);
		expect((await lease.acquire()).error).toBeTruthy();
		expect((await lease.release()).error).toBeTruthy();
		expect(calls).toEqual(['capture_acquire_dictation']);
		admitted.resolve('fixture-token');
		expect((await starting).error).toBeNull();
		await lease.release();
		expect(lease.held).toBe(false);
	});
	test('release is acknowledged and retryable, never silently expired', async () => {
		const f = fixture();
		await f.lease.acquire();
		f.failRelease();
		expect((await f.lease.release()).error).toBeTruthy();
		expect(f.lease.held).toBe(true);
		expect((await f.lease.release()).error).toBeNull();
		expect(f.lease.held).toBe(false);
	});
	test('web mode makes no native calls', async () => {
		const lease = createCaptureLease({
			enabled: false,
			request: async () => {
				throw new Error('Unexpected native call');
			},
		});
		await lease.acquire();
		await lease.release();
		expect(lease.held).toBe(false);
	});
});

describe('recorder admission adapter (synthetic backend only)', () => {
	test('live-client resolution is lazy and shared, avoiding service-index initialization cycles', async () => {
		const f = fixture(); let resolutions = 0;
		const service = createLazyRecorderService(() => { resolutions++; return f.base; });
		expect(resolutions).toBe(0);
		await service.getRecorderState(); await service.enumerateDevices();
		expect(resolutions).toBe(1);
	});
	test('temporary enumeration is leased, cleaned and released without nested admission', async () => {
		const f = fixture(); let closes = 0;
		f.base.enumerateDevices = async () => {
			expect(f.lease.held).toBe(true);
			return Ok([]);
		};
		const service = createLeasedRecorderService(f.base, f.lease, () => { closes++; });
		expect((await service.enumerateDevices()).error).toBeNull();
		expect(closes).toBe(1);
		expect(f.calls).toEqual(['capture_acquire_dictation', 'capture_release_dictation']);
		await service.startRecording(params, callbacks);
		expect((await service.enumerateDevices()).error).toBeTruthy();
		await service.cancelRecording(callbacks);
	});
	test('failed enumeration cleanup holds admission and Cancel retries the backend before release', async () => {
		const f = fixture();
		const service = createLeasedRecorderService(f.base, f.lease, () => { throw new Error('temporary stream still open'); });
		expect((await service.enumerateDevices()).error).toBeTruthy();
		expect(f.lease.held).toBe(true);
		expect((await service.startRecording(params, callbacks)).error).toBeTruthy();
		await service.cancelRecording(callbacks);
		expect(f.counts.cancels).toBe(1); expect(f.lease.held).toBe(false);
	});

	test('native refusal never opens a device', async () => {
		const f = fixture();
		const lease = createCaptureLease({
			enabled: true,
			request: async () => {
				throw new Error('A meeting is active');
			},
		});
		const recorder = createLeasedRecorderService(f.base, lease);
		expect(
			(await recorder.startRecording(params, callbacks)).error,
		).toBeTruthy();
		expect(f.counts.starts).toBe(0);
	});
	test('lease covers device initialization and stopping, not just RECORDING state', async () => {
		const f = fixture();
		const started = deferred<void>();
		const stopped = deferred<void>();
		const originalStart = f.base.startRecording;
		const originalStop = f.base.stopRecording;
		f.base.startRecording = async (...args) => {
			await started.promise;
			return originalStart(...args);
		};
		f.base.stopRecording = async (...args) => {
			await stopped.promise;
			return originalStop(...args);
		};
		const recorder = createLeasedRecorderService(f.base, f.lease);
		const starting = recorder.startRecording(params, callbacks);
		await Promise.resolve();
		await Promise.resolve();
		expect(
			(await recorder.startRecording(params, callbacks)).error,
		).toBeTruthy();
		started.resolve();
		await starting;
		const stopping = recorder.stopRecording(callbacks);
		expect(
			(await recorder.startRecording(params, callbacks)).error,
		).toBeTruthy();
		expect(f.lease.held).toBe(true);
		stopped.resolve();
		await stopping;
		expect(f.lease.held).toBe(false);
	});
	test('saved audio survives release failure; retrying Cancel only releases the lock', async () => {
		const f = fixture();
		const recorder = createLeasedRecorderService(f.base, f.lease);
		await recorder.startRecording(params, callbacks);
		f.failRelease();
		const stopped = await recorder.stopRecording(callbacks);
		expect(stopped.data).toBe(f.blob);
		expect(stopped.error).toBeNull();
		expect(f.lease.held).toBe(true);
		await recorder.cancelRecording(callbacks);
		expect(f.lease.held).toBe(false);
		expect(f.counts.cancels).toBe(0);
	});
	test('failed startup releases only after the backend confirms cleanup', async () => {
		const f = fixture();
		f.base.startRecording = async () =>
			RecorderServiceErr({ message: 'fixture start failure' });
		const recorder = createLeasedRecorderService(f.base, f.lease);
		expect(
			(await recorder.startRecording(params, callbacks)).error,
		).toBeTruthy();
		expect(f.counts.cancels).toBe(1);
		expect(f.lease.held).toBe(false);
	});
	test('failed or unknown cleanup retains native admission', async () => {
		const f = fixture();
		f.base.startRecording = async () =>
			RecorderServiceErr({ message: 'fixture start failure' });
		f.base.cancelRecording = async () =>
			RecorderServiceErr({ message: 'fixture cleanup unknown' });
		const recorder = createLeasedRecorderService(f.base, f.lease);
		await recorder.startRecording(params, callbacks);
		expect(f.lease.held).toBe(true);
		expect(f.calls).toEqual(['capture_acquire_dictation']);
	});
	test('pre-existing recordings are not automatically cancelled on a rejected Start', async () => {
		const f = fixture();
		f.setActive();
		const recorder = createLeasedRecorderService(f.base, f.lease);
		expect(
			(await recorder.startRecording(params, callbacks)).error,
		).toBeTruthy();
		expect(f.counts.starts).toBe(0);
		expect(f.counts.cancels).toBe(0);
		await recorder.cancelRecording(callbacks);
		expect(f.lease.held).toBe(false);
	});
	test('an unexpected backend exception is reported without releasing unknown ownership', async () => {
		const f = fixture();
		f.base.getRecorderState = async () => {
			throw new Error('fixture state unavailable');
		};
		const recorder = createLeasedRecorderService(f.base, f.lease);
		expect(
			(await recorder.startRecording(params, callbacks)).error,
		).toBeTruthy();
		expect(f.lease.held).toBe(true);
		expect(f.counts.starts).toBe(0);
	});
});
