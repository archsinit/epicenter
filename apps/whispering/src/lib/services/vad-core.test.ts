import { describe, expect, test } from 'bun:test';
import { Ok } from 'wellcrafted/result';
import { createCaptureLease } from './capture-lease';
import { createVadRecorderCore, type VadCallbacks } from './vad-core';
import { asDeviceIdentifier } from './types';

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => { resolve = done; });
	return { promise, resolve };
}
const params = { selectedDeviceId: null, sendStatus: () => {} };
const outcome = { outcome: 'success' as const, deviceId: asDeviceIdentifier('fixture') };
function fixture() {
	const events: string[] = [];
	const states: string[] = ['IDLE'];
	const callbacks: VadCallbacks[] = [];
	const failures = { release: 0, stream: 0, pending: 0, destroy: 0 };
	let forbidden = false;
	let speech = 0;
	const hooks = {
		acquire: async () => {}, initialize: async () => {}, start: async () => {},
		destroy: async () => {}, enumerate: async () => {},
	};
	const lease = createCaptureLease({
		enabled: true,
		request: async <T>(command: string) => {
			events.push(command);
			if (command === 'capture_acquire_dictation' && forbidden) throw new Error('meeting active');
			if (command === 'capture_release_dictation' && failures.release-- > 0) throw new Error('release unavailable');
			return (command === 'capture_acquire_dictation' ? 'fixture-token' : undefined) as T;
		},
	});
	const core = createVadRecorderCore({
		lease,
		acquireStream: async () => {
			events.push('acquire-stream'); await hooks.acquire();
			return Ok({ stream: {} as MediaStream, deviceOutcome: outcome });
		},
		create: async (_, cb) => {
			events.push('initialize'); callbacks.push(cb); await hooks.initialize();
			return {
				start: async () => { events.push('start'); await hooks.start(); },
				destroy: async () => {
					events.push('destroy'); await hooks.destroy();
					if (failures.destroy-- > 0) throw new Error('destroy failed');
				},
			};
		},
		enumerateDevices: async () => { events.push('enumerate'); await hooks.enumerate(); return Ok([]); },
		cleanupStream: () => {
			events.push('close-stream');
			if (failures.stream-- > 0) throw new Error('track still live');
		},
		cleanupPendingStreams: () => {
			events.push('close-temporary');
			if (failures.pending-- > 0) throw new Error('temporary track still live');
		},
		onState: (state) => states.push(state),
	});
	return {
		core, lease, events, states, callbacks, hooks, failures,
		start: () => core.start(params, { onSpeechStart: () => {}, onSpeechEnd: () => { speech++; } }),
		get speech() { return speech; }, get state() { return states.at(-1); },
		forbid: () => { forbidden = true; },
	};
}

describe('VAD lifetime (synthetic, no model or devices)', () => {
	test('lease spans all speech chunks and releases only after destroy and tracks', async () => {
		const f = fixture(); expect((await f.start()).error).toBeNull();
		f.callbacks[0]!.onSpeechStart();
		f.callbacks[0]!.onSpeechEnd(new Float32Array(8));
		f.callbacks[0]!.onSpeechEnd(new Float32Array(8));
		expect(f.speech).toBe(2); expect(f.lease.held).toBe(true);
		expect(f.events).not.toContain('capture_release_dictation');
		expect((await f.core.stop()).error).toBeNull();
		expect(f.events.indexOf('capture_acquire_dictation')).toBeLessThan(f.events.indexOf('acquire-stream'));
		expect(f.events.indexOf('destroy')).toBeLessThan(f.events.indexOf('close-stream'));
		expect(f.events.indexOf('close-stream')).toBeLessThan(f.events.indexOf('capture_release_dictation'));
		expect(f.lease.held).toBe(false); expect(f.state).toBe('IDLE');
	});
	test('native refusal opens no device or model', async () => {
		const f = fixture(); f.forbid(); expect((await f.start()).error).toBeTruthy();
		expect(f.events).not.toContain('acquire-stream'); expect(f.events).not.toContain('initialize');
	});
	test('concurrent start is refused; Stop cancels pending acquisition without a late model', async () => {
		const f = fixture(); const entered = deferred<void>(); const acquired = deferred<void>();
		f.hooks.acquire = async () => { entered.resolve(); await acquired.promise; };
		const starting = f.start(); await entered.promise;
		expect(f.state).toBe('STARTING'); expect((await f.start()).error).toBeTruthy();
		const stopping = f.core.stop(); expect(f.lease.held).toBe(true); acquired.resolve();
		expect((await starting).error?.message).toContain('cancelled');
		expect((await stopping).error).toBeNull(); expect(f.events).not.toContain('initialize');
		expect(f.events).toContain('close-stream'); expect(f.state).toBe('IDLE');
	});
	test('Stop closes known tracks during model initialization, then drains the late handle', async () => {
		const f = fixture(); const entered = deferred<void>(); const initialized = deferred<void>();
		f.hooks.initialize = async () => { entered.resolve(); await initialized.promise; };
		const starting = f.start(); await entered.promise; const stopping = f.core.stop();
		expect(f.events).toContain('close-stream'); expect(f.lease.held).toBe(true);
		expect(f.state).toBe('STOPPING'); initialized.resolve();
		expect((await starting).error).toBeTruthy(); expect((await stopping).error).toBeNull();
		expect(f.events).not.toContain('start'); expect(f.events).toContain('destroy');
		expect(f.lease.held).toBe(false);
	});
	test('constructor and asynchronous Start errors clean acquired streams', async () => {
		for (const stage of ['initialize', 'start'] as const) {
			const f = fixture(); f.hooks[stage] = async () => { throw new Error(stage + ' failed'); };
			expect((await f.start()).error?.message).toContain(stage + ' failed');
			expect(f.events).toContain('close-stream'); expect(f.lease.held).toBe(false);
		}
	});
	test('failed destroy retains handle/lease, still closes tracks, and retries Stop', async () => {
		const f = fixture(); await f.start(); f.failures.destroy = 1;
		expect((await f.core.stop()).error).toBeTruthy(); expect(f.events).toContain('close-stream');
		expect(f.events).toContain('close-temporary'); expect(f.state).toBe('CLEANUP_REQUIRED');
		expect(f.lease.held).toBe(true); expect((await f.start()).error).toBeTruthy();
		expect((await f.core.stop()).error).toBeNull();
		expect(f.events.filter((e) => e === 'destroy')).toHaveLength(2); expect(f.lease.held).toBe(false);
	});
	test('stream and temporary-stream failures retain admission until Stop retry', async () => {
		for (const fail of ['stream', 'pending'] as const) {
			const f = fixture(); await f.start(); f.failures[fail] = 1;
			expect((await f.core.stop()).error).toBeTruthy(); expect(f.lease.held).toBe(true);
			expect((await f.core.stop()).error).toBeNull(); expect(f.lease.held).toBe(false);
		}
	});
	test('release failure retries acknowledgement without duplicate destroy or delivery', async () => {
		const f = fixture(); await f.start(); f.failures.release = 1;
		expect((await f.core.stop()).error).toBeTruthy(); expect(f.state).toBe('CLEANUP_REQUIRED');
		expect((await f.core.stop()).error).toBeNull();
		expect(f.events.filter((e) => e === 'destroy')).toHaveLength(1);
		expect(f.events.filter((e) => e === 'close-stream')).toHaveLength(1);
	});
	test('normal Stop flushes final speech; stale callbacks cannot revive another session', async () => {
		const f = fixture(); await f.start(); const old = f.callbacks[0]!;
		f.hooks.destroy = async () => old.onSpeechEnd(new Float32Array(4)); await f.core.stop();
		expect(f.speech).toBe(1); old.onSpeechStart(); old.onSpeechEnd(new Float32Array(4));
		expect(f.state).toBe('IDLE'); expect(f.speech).toBe(1);
		f.hooks.destroy = async () => {}; await f.start();
		old.onSpeechStart(); old.onSpeechEnd(new Float32Array(4));
		expect(f.state).toBe('LISTENING'); expect(f.speech).toBe(1); await f.core.stop();
	});
	test('asynchronous destroy holds admission and repeated Stop shares cleanup', async () => {
		const f = fixture(); await f.start(); const entered = deferred<void>(); const destroyed = deferred<void>();
		f.hooks.destroy = async () => { entered.resolve(); await destroyed.promise; };
		const first = f.core.stop(); await entered.promise; const second = f.core.stop();
		expect(f.lease.held).toBe(true); destroyed.resolve(); await Promise.all([first, second]);
		expect(f.events.filter((e) => e === 'destroy')).toHaveLength(1); expect(f.lease.held).toBe(false);
	});
	test('enumeration cleanup precedes release, even on error; unknown closure needs Stop retry', async () => {
		const f = fixture(); f.hooks.enumerate = async () => { throw new Error('enumerate failed'); };
		expect((await f.core.enumerateDevices()).error?.message).toContain('enumerate failed');
		expect(f.events.indexOf('capture_acquire_dictation')).toBeLessThan(f.events.indexOf('enumerate'));
		expect(f.events.indexOf('close-temporary')).toBeLessThan(f.events.indexOf('capture_release_dictation'));
		expect(f.lease.held).toBe(false); f.failures.pending = 1;
		expect((await f.core.enumerateDevices()).error).toBeTruthy(); expect(f.lease.held).toBe(true);
		expect((await f.core.stop()).error).toBeNull();
	});
});
