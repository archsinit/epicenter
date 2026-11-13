import { describe, expect, test } from 'bun:test';
import { createDeviceEnumerationCore, stopRecordingStream } from './device-enumeration';
import { createDeviceEnumeration, getRecordingStream, cleanupRecordingStream } from './device-stream';
import { asDeviceIdentifier } from './types';

function fixture() {
	let stopFailures = 0;
	let enumerationFails = false;
	let acquireFails = false;
	let stopped = 0;
	let acquired = 0;
	const track = { readyState: 'live', stop() {
		stopped++;
		if (stopFailures-- > 0) throw new Error('stop failed');
		track.readyState = 'ended';
	} };
	const core = createDeviceEnumerationCore({
		acquire: async () => {
			acquired++; if (acquireFails) throw new Error('acquire failed');
			return { getTracks: () => [track] } as unknown as MediaStream;
		},
		enumerate: async () => {
			if (enumerationFails) throw new Error('enumerate failed');
			return [];
		},
		cleanupStream: stopRecordingStream,
	});
	return { core, track, get stopped() { return stopped; }, get acquired() { return acquired; },
		failStop: () => { stopFailures++; },
		failEnumeration: () => { enumerationFails = true; },
		failAcquisition: () => { acquireFails = true; },
	};
}
describe('temporary microphone enumeration (fake streams)', () => {
	test('actual fallback helper closes temporary enumeration and returns the owned recording stream', async () => {
		const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
		const tracks: { readyState: string; stop(): void }[] = [];
		const calls: MediaStreamConstraints[] = [];
		Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {
			permissions: { query: async () => ({ state: 'granted' }) },
			mediaDevices: {
				getUserMedia: async (constraints: MediaStreamConstraints) => {
					calls.push(constraints);
					const track = { readyState: 'live', stop() { track.readyState = 'ended'; } };
					tracks.push(track);
					return { getTracks: () => [track] } as unknown as MediaStream;
				},
				enumerateDevices: async () => [{ kind: 'audioinput', deviceId: 'fixture-mic', label: 'Fixture' }],
			},
		} });
		try {
			const devices = createDeviceEnumeration();
			const acquired = await getRecordingStream({
				selectedDeviceId: null, sendStatus: () => {},
				enumerateForFallback: devices.enumerateDevices,
			});
			expect(acquired.error).toBeNull();
			expect(acquired.data?.deviceOutcome).toEqual({ outcome: 'fallback', reason: 'no-device-selected', deviceId: asDeviceIdentifier('fixture-mic') });
			expect(calls).toHaveLength(2);
			expect(tracks[0]!.readyState).toBe('ended');
			expect(tracks[1]!.readyState).toBe('live');
			expect(devices.held).toBe(false);
			cleanupRecordingStream(acquired.data!.stream);
			expect(tracks[1]!.readyState).toBe('ended');
		} finally {
			if (original) Object.defineProperty(globalThis, 'navigator', original);
			else Reflect.deleteProperty(globalThis, 'navigator');
		}
	});
	test('preferred-device helper preserves selection and does not enumerate or open a temporary stream', async () => {
		const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
		let constraints: MediaStreamConstraints | undefined;
		const track = { readyState: 'live', stop() { track.readyState = 'ended'; } };
		Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {
			permissions: { query: async () => ({ state: 'granted' }) },
			mediaDevices: {
				getUserMedia: async (value: MediaStreamConstraints) => {
					constraints = value;
					return { getTracks: () => [track] } as unknown as MediaStream;
				},
			},
		} });
		try {
			const acquired = await getRecordingStream({
				selectedDeviceId: asDeviceIdentifier('selected-mic'), sendStatus: () => {},
				enumerateForFallback: async () => { throw new Error('Unexpected enumeration'); },
			});
			expect(acquired.error).toBeNull();
			expect(acquired.data?.deviceOutcome).toEqual({ outcome: 'success', deviceId: asDeviceIdentifier('selected-mic') });
			expect(constraints?.audio).toMatchObject({ deviceId: { exact: 'selected-mic' } });
			cleanupRecordingStream(acquired.data!.stream);
			expect(track.readyState).toBe('ended');
		} finally {
			if (original) Object.defineProperty(globalThis, 'navigator', original);
			else Reflect.deleteProperty(globalThis, 'navigator');
		}
	});
	test('enumeration success and failure both close temporary tracks', async () => {
		for (const fail of [false, true]) {
			const f = fixture(); if (fail) f.failEnumeration();
			const result = await f.core.enumerate();
			expect(Boolean(result.error)).toBe(fail);
			expect(f.stopped).toBe(1); expect(f.track.readyState).toBe('ended');
			expect(f.core.held).toBe(false);
		}
	});
	test('failed acquisition has no invented stream to forget or close', async () => {
		const f = fixture(); f.failAcquisition();
		expect((await f.core.enumerate()).error?.message).toContain('acquire failed');
		expect(f.stopped).toBe(0); expect(f.core.held).toBe(false);
	});
	test('failed closure retains the exact stream for retry before opening another', async () => {
		const f = fixture(); f.failStop();
		expect((await f.core.enumerate()).error?.message).toContain('stop failed');
		expect(f.core.held).toBe(true); expect(f.acquired).toBe(1);
		f.core.cleanup(); expect(f.core.held).toBe(false);
		expect(f.stopped).toBe(2); expect(f.acquired).toBe(1);
	});
	test('one failed track never prevents stopping later tracks; false closure is rejected', () => {
		const calls: number[] = [];
		const first = { readyState: 'live', stop: () => { calls.push(1); throw new Error('first failed'); } };
		const second = { readyState: 'live', stop: () => { calls.push(2); second.readyState = 'ended'; } };
		const stream = { getTracks: () => [first, second] } as unknown as MediaStream;
		expect(() => stopRecordingStream(stream)).toThrow();
		expect(calls).toEqual([1, 2]);
		expect(() => stopRecordingStream({ getTracks: () => [{ readyState: 'live', stop: () => {} }] } as unknown as MediaStream)).toThrow('has not ended');
	});
	test('consumer pools are independent and reject overlapping enumeration', async () => {
		let finish!: (value: MediaDeviceInfo[]) => void;
		let opened!: () => void;
		const entered = new Promise<void>((resolve) => { opened = resolve; });
		const a = createDeviceEnumerationCore({
			acquire: async () => ({ getTracks: () => [] } as unknown as MediaStream),
			enumerate: () => { opened(); return new Promise((resolve) => { finish = resolve; }); },
			cleanupStream: stopRecordingStream,
		});
		const b = fixture(); b.failStop(); await b.core.enumerate();
		const listing = a.enumerate(); await entered;
		expect((await a.enumerate()).error).toBeTruthy();
		a.cleanup(); expect(b.core.held).toBe(true);
		finish([]); await listing; b.core.cleanup();
		expect(a.held).toBe(false); expect(b.core.held).toBe(false);
	});
});
