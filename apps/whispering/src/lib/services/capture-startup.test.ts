import { describe, expect, test } from 'bun:test';
import { Ok } from 'wellcrafted/result';
import { createCaptureStartup } from './capture-startup';
import { RecorderServiceErr } from './recorder/types';

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => {
		resolve = done;
	});
	return { promise, resolve };
}
describe('capture startup (synthetic; no devices or IPC)', () => {
	test('concurrent admissions share restoration and await native acknowledgement', async () => {
		const restored = deferred<ReturnType<typeof Ok<void>>>();
		const confirmed = deferred<void>();
		const events: string[] = [];
		const barrier = createCaptureStartup({
			restore: () => {
				events.push('inspect');
				return restored.promise;
			},
			confirm: () => {
				events.push('confirm');
				return confirmed.promise;
			},
		});
		const first = barrier.wait();
		const second = barrier.wait();
		expect(first).toBe(second);
		let admitted = false;
		void first.then(() => {
			admitted = true;
		});
		expect(events).toEqual(['inspect']);
		restored.resolve(Ok(undefined));
		await Promise.resolve();
		await Promise.resolve();
		expect(events).toEqual(['inspect', 'confirm']);
		expect(admitted).toBe(false);
		confirmed.resolve();
		expect((await first).error).toBeNull();
		expect(await barrier.wait()).toEqual(Ok(undefined));
		expect(events).toEqual(['inspect', 'confirm']);
	});
	test('unverified saved sessions never acknowledge or retry via another backend', async () => {
		let inspections = 0;
		let confirmations = 0;
		const barrier = createCaptureStartup({
			restore: async () => {
				inspections++;
				return RecorderServiceErr({ message: 'unverified' });
			},
			confirm: async () => {
				confirmations++;
			},
		});
		expect((await barrier.wait()).error?.message).toBe('unverified');
		expect((await barrier.wait()).error?.message).toBe('unverified');
		expect(inspections).toBe(1);
		expect(confirmations).toBe(0);
	});
	test('thrown restoration errors also fail closed', async () => {
		let confirmed = false;
		const barrier = createCaptureStartup({
			restore: async () => {
				throw new Error('storage unavailable');
			},
			confirm: async () => {
				confirmed = true;
			},
		});
		expect((await barrier.wait()).error?.message).toContain('storage unavailable');
		expect(confirmed).toBe(false);
	});
	test('failed native acknowledgement is sticky, not successful admission', async () => {
		let attempts = 0;
		const barrier = createCaptureStartup({
			restore: async () => Ok(undefined),
			confirm: async () => {
				attempts++;
				throw new Error('native fence unavailable');
			},
		});
		expect((await barrier.wait()).error?.message).toContain('native fence unavailable');
		expect((await barrier.wait()).error).toBeTruthy();
		expect(attempts).toBe(1);
	});
});
