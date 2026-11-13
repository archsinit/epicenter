import { describe, expect, test } from 'bun:test';
import { Ok } from 'wellcrafted/result';
import { createRecorderSession } from './session';
import { createCaptureLease } from '../capture-lease';
import { createLeasedRecorderService } from './leased';
import { RecorderServiceErr, type RecorderService, type StartRecordingParams } from './types';
import { asDeviceIdentifier } from '../types';

const callbacks = { sendStatus: () => {} };
const params: StartRecordingParams = { method: 'navigator', recordingId: 'original-id', selectedDeviceId: null, bitrateKbps: '64' };
function fixture() {
	let active = false;
	let stops = 0; let cancels = 0; let failStop = false; let failCancel = false;
	let failRelease = false;
	const blob = new Blob(['retained fixture audio']);
	const lease = createCaptureLease({
		enabled: true,
		request: async <T>(command: string) => {
			if (command === 'capture_release_dictation' && failRelease) throw new Error('release failed');
			return (command === 'capture_acquire_dictation' ? 'fixture' : undefined) as T;
		},
	});
	const base: RecorderService = {
		getRecorderState: async () => Ok(active ? 'RECORDING' : 'IDLE'),
		enumerateDevices: async () => Ok([]),
		startRecording: async () => { active = true; return Ok({ outcome: 'success', deviceId: asDeviceIdentifier('fixture') }); },
		stopRecording: async () => {
			stops++;
			if (failStop) return RecorderServiceErr({ message: 'Stop cleanup failed' });
			active = false; return Ok(blob);
		},
		cancelRecording: async () => {
			cancels++;
			if (failCancel) return RecorderServiceErr({ message: 'Cancel cleanup failed' });
			active = false; return Ok({ status: 'cancelled' });
		},
	};
	return {
		service: createLeasedRecorderService(base, lease), base, blob, lease,
		get stops() { return stops; }, get cancels() { return cancels; },
		set failStop(value: boolean) { failStop = value; },
		set failCancel(value: boolean) { failCancel = value; },
		set failRelease(value: boolean) { failRelease = value; },
	};
}
describe('dictation session ownership (synthetic)', () => {
	test('backend selection changes cannot redirect Stop and Stop errors retain the original ID', async () => {
		const a = fixture(); const b = fixture(); const session = createRecorderSession();
		await session.start(a.service, params, callbacks); a.failStop = true;
		expect((await session.stop(b.service, callbacks)).error).toBeTruthy();
		expect(b.stops).toBe(0);
		expect((await session.start(b.service, { ...params, recordingId: 'new-id' }, callbacks)).error).toBeTruthy();
		a.failStop = false;
		const result = await session.stop(b.service, callbacks);
		expect(result.data?.recordingId).toBe('original-id');
		expect(result.data?.blob).toBe(a.blob); expect(b.stops).toBe(0);
		await session.start(b.service, { ...params, recordingId: 'new-id' }, callbacks);
		expect((await session.stop(a.service, callbacks)).data?.recordingId).toBe('new-id');
	});
	test('failed Cancel retains the pinned backend even when preferences select another', async () => {
		const a = fixture(); const b = fixture(); const session = createRecorderSession();
		await session.start(a.service, params, callbacks); a.failCancel = true;
		expect((await session.cancel(b.service, callbacks)).error).toBeTruthy();
		expect(b.cancels).toBe(0); a.failCancel = false;
		expect((await session.cancel(b.service, callbacks)).error).toBeNull();
		expect(a.cancels).toBe(2); expect(b.cancels).toBe(0);
	});
	test('delivered Blob survives release failure; Cancel retries only the lock and cannot redeliver audio', async () => {
		const a = fixture(); const b = fixture(); const session = createRecorderSession();
		await session.start(a.service, params, callbacks); a.failRelease = true;
		expect((await session.stop(b.service, callbacks)).data?.blob).toBe(a.blob);
		expect((await session.getRecorderState(b.service)).data).toBe('RECORDING');
		expect((await session.stop(b.service, callbacks)).error?.message).toContain('already delivered');
		expect(a.stops).toBe(1); expect(a.lease.held).toBe(true);
		a.failRelease = false;
		expect((await session.cancel(b.service, callbacks)).error).toBeNull();
		expect(a.cancels).toBe(0); expect(b.cancels).toBe(0);
		expect(a.lease.held).toBe(false);
	});
	test('pre-existing recordings are not relabelled or automatically cancelled by Start', async () => {
		const a = fixture(); const session = createRecorderSession();
		await a.base.startRecording(params, callbacks);
		expect((await session.start(a.service, { ...params, recordingId: 'wrong-new-id' }, callbacks)).error).toBeTruthy();
		expect(a.cancels).toBe(0);
	});
});
