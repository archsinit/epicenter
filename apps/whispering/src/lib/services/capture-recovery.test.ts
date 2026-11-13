import { describe, expect, test } from 'bun:test';
import { Ok } from 'wellcrafted/result';
import { createCaptureRecovery } from './capture-recovery';
import { RecorderServiceErr } from './recorder/types';

function fixture() {
	let saved: string | null = '{malformed saved metadata PID=1234';
	let healthy = false;
	let confirmations = 0;
	let clears = 0;
	let confirm: (raw: string) => Promise<string | null> = async () => '/private/archive.json';
	let clear = () => { saved = null; };
	const seen: string[] = [];
	const service = createCaptureRecovery({
		ready: async () => healthy ? Ok(undefined) : RecorderServiceErr({ message: 'unverified' }),
		readSaved: () => saved,
		confirmAndArchive: async (raw) => { confirmations++; seen.push(raw); return confirm(raw); },
		clearSaved: () => { clears++; clear(); },
	});
	return {
		service, seen,
		get saved() { return saved; },
		set saved(value: string | null) { saved = value; },
		get confirmations() { return confirmations; },
		get clears() { return clears; },
		set healthy(value: boolean) { healthy = value; },
		set confirm(value: typeof confirm) { confirm = value; },
		set clear(value: typeof clear) { clear = value; },
	};
}
describe('saved capture metadata recovery (injected, no native dialog/devices)', () => {
	test('status never confirms, clears metadata or treats a failed startup as idle', async () => {
		const f = fixture();
		expect((await f.service.getStatus()).data).toEqual({ blocked: true, hasSavedSession: true, reason: 'unverified' });
		expect(f.confirmations).toBe(0);
		expect(f.clears).toBe(0);
	});
	test('successful native archive clears only matching metadata, not admission', async () => {
		const f = fixture();
		const original = f.saved;
		expect((await f.service.recover()).data).toEqual({ status: 'restart-required', archivePath: '/private/archive.json' });
		expect(f.seen).toEqual([original!]);
		expect(f.saved).toBeNull();
		expect((await f.service.getStatus()).data).toEqual({ blocked: true, hasSavedSession: false, reason: 'unverified' });
		expect((await f.service.recover()).error).toBeTruthy();
		expect(f.confirmations).toBe(1);
	});
	test('cancel leaves the exact malformed saved record untouched', async () => {
		const f = fixture();
		f.confirm = async () => null;
		const original = f.saved;
		expect((await f.service.recover()).data).toEqual({ status: 'cancelled' });
		expect(f.saved).toBe(original);
		expect(f.clears).toBe(0);
	});
	test('healthy startup cannot use recovery as Stop or discard', async () => {
		const f = fixture();
		f.healthy = true;
		expect((await f.service.recover()).error).toBeTruthy();
		expect(f.confirmations).toBe(0);
		expect(f.clears).toBe(0);
	});
	test('archive/confirmation failure preserves metadata and can be retried explicitly', async () => {
		const f = fixture();
		f.confirm = async () => { throw new Error('archive permission denied'); };
		expect((await f.service.recover()).error?.message).toContain('permission denied');
		expect(f.clears).toBe(0);
		f.confirm = async () => '/private/retry.json';
		expect((await f.service.recover()).data?.status).toBe('restart-required');
	});
	test('invalid native acknowledgement never clears metadata', async () => {
		const f = fixture();
		f.confirm = async () => '';
		expect((await f.service.recover()).error).toBeTruthy();
		expect(f.clears).toBe(0);
	});
	test('a newer saved session arriving during confirmation is retained', async () => {
		const f = fixture();
		f.confirm = async () => { f.saved = '{"captureId":"newer"}'; return '/private/older.json'; };
		expect((await f.service.recover()).error?.message).toContain('changed');
		expect(f.saved).toBe('{"captureId":"newer"}');
		expect(f.clears).toBe(0);
	});
	test('concurrent recovery is rejected while native confirmation is pending', async () => {
		const f = fixture();
		let acknowledge!: (path: string | null) => void;
		f.confirm = () => new Promise((resolve) => { acknowledge = resolve; });
		const first = f.service.recover();
		await Promise.resolve();
		expect((await f.service.recover()).error?.message).toContain('already in progress');
		acknowledge(null);
		expect((await first).data?.status).toBe('cancelled');
		expect(f.confirmations).toBe(1);
	});
	test('localStorage write failure and ignored removal do not report completion', async () => {
		const f = fixture();
		f.clear = () => { throw new Error('storage denied'); };
		expect((await f.service.recover()).error?.message).toContain('storage denied');
		expect(f.saved).not.toBeNull();
		f.clear = () => {};
		expect((await f.service.recover()).error?.message).toContain('could not be cleared');
	});
});
