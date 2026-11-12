import { afterAll, afterEach, expect, mock, spyOn, test } from 'bun:test';

type State = { phase: string; metered: boolean };
const calls: State[] = [];
const outcomes: Array<() => Promise<void>> = [];
const runtime: { __TAURI_INTERNALS__: object | undefined } = {
	__TAURI_INTERNALS__: {},
};
const originalWindow = Object.getOwnPropertyDescriptor(globalThis, 'window');
Object.defineProperty(globalThis, 'window', {
	configurable: true,
	value: runtime,
});

mock.module('@tauri-apps/api/core', () => ({
	invoke(command: string, state: State) {
		expect(command).toBe('set_indicator_state');
		calls.push({ ...state });
		return outcomes.shift()?.() ?? Promise.resolve();
	},
}));
const { indicator } = await import('./indicator');
const settle = () => Bun.sleep(0);
const phases = () => calls.map((state) => state.phase);

afterEach(async () => {
	runtime.__TAURI_INTERNALS__ = {};
	outcomes.length = 0;
	indicator.recording(false, false);
	indicator.finish('a');
	indicator.finish('b');
	await settle();
	calls.length = 0;
});

afterAll(() => {
	if (originalWindow)
		Object.defineProperty(globalThis, 'window', originalWindow);
	else Reflect.deleteProperty(globalThis, 'window');
});

test('transcription, cleanup and completion publish distinct phases', async () => {
	indicator.begin('a');
	indicator.cleaning('a');
	indicator.finish('a');
	await settle();
	expect(phases()).toEqual(['transcribing', 'cleaning', 'idle']);
});

test('finishing an earlier job cannot hide a new recording', async () => {
	indicator.begin('a');
	indicator.recording(true, true);
	indicator.finish('a');
	await settle();
	expect(phases()).toEqual(['transcribing', 'recording', 'recording']);
	expect(calls.at(-1)).toEqual({ phase: 'recording', metered: true });
});

test('finishing one job keeps another pending job visible', async () => {
	indicator.begin('a');
	indicator.begin('b');
	indicator.finish('a');
	await settle();
	expect(phases()).toEqual(['transcribing', 'transcribing', 'transcribing']);
	indicator.finish('b');
	await settle();
	expect(calls.at(-1)?.phase).toBe('idle');
});

test('a new take takes priority over cleanup, which resumes after recording', async () => {
	indicator.begin('a');
	indicator.recording(true, true);
	indicator.cleaning('a');
	indicator.recording(false, true);
	await settle();
	expect(phases()).toEqual([
		'transcribing',
		'recording',
		'recording',
		'cleaning',
	]);
});

test('a recorder without a meter never advertises live microphone levels', async () => {
	indicator.recording(true, false);
	await settle();
	expect(calls).toEqual([{ phase: 'recording', metered: false }]);
});

test('native updates remain serialized while an earlier update is pending', async () => {
	let release = () => {};
	outcomes.push(
		() =>
			new Promise<void>((resolve) => {
				release = resolve;
			}),
	);
	indicator.begin('a');
	indicator.cleaning('a');
	indicator.finish('a');
	try {
		await settle();
		expect(phases()).toEqual(['transcribing']);
	} finally {
		release();
	}
	await settle();
	expect(phases()).toEqual(['transcribing', 'cleaning', 'idle']);
});

test('a native update failure does not block subsequent updates', async () => {
	const warning = spyOn(console, 'warn').mockImplementation(() => {});
	outcomes.push(() => Promise.reject(new Error('fixture IPC failure')));
	try {
		indicator.begin('a');
		indicator.cleaning('a');
		indicator.finish('a');
		await settle();
		expect(phases()).toEqual(['transcribing', 'cleaning', 'idle']);
		expect(warning).toHaveBeenCalledTimes(1);
	} finally {
		warning.mockRestore();
	}
});

test('failure remains visible briefly after a job ends, then returns to idle', async () => {
	const realSetTimeout = globalThis.setTimeout;
	let expire: () => void = () => {
		throw new Error('No indicator expiry scheduled');
	};
	const replacement = Object.assign(
		(callback: TimerHandler) => {
			if (typeof callback !== 'function')
				throw new Error('Expected a timer callback');
			expire = () => callback();
			return realSetTimeout(() => {}, 0);
		},
		{ __promisify__: realSetTimeout.__promisify__ },
	);
	// DOM and Node disagree on timer handles; this Bun-only stub preserves the real handle.
	const timer = spyOn(globalThis, 'setTimeout').mockImplementation(
		replacement as unknown as typeof setTimeout,
	);
	try {
		indicator.begin('a');
		indicator.fail();
		indicator.finish('a');
		await settle();
		expect(phases()).toEqual(['transcribing', 'transcribing', 'error']);
		expect(timer).toHaveBeenCalledWith(expect.any(Function), 5000);
	} finally {
		expire();
		timer.mockRestore();
	}
	await settle();
	expect(calls.at(-1)?.phase).toBe('idle');
});

test('the browser surface does not attempt native window calls', async () => {
	runtime.__TAURI_INTERNALS__ = undefined;
	indicator.begin('a');
	indicator.cleaning('a');
	indicator.finish('a');
	await settle();
	expect(calls).toEqual([]);
});
