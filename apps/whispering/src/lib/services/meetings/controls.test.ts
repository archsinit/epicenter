import { describe, expect, test } from 'bun:test';
import { Err, Ok, type Result } from 'wellcrafted/result';
import { createMeetingControls, validateMeetingStart, meetingStartAvailability, type StartMeetingParams } from './controls';

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((done) => { resolve = done; });
	return { promise, resolve };
}
const params: StartMeetingParams = { title: 'Meeting', mode: 'microphone', microphoneId: '', displayId: null, confirmed: true };
function progress(extra: Record<string, unknown> = {}) {
	return { id: 'meeting-1', title: 'Meeting', mode: 'microphone', microphoneId: '', displayId: null,
		captureReserved: true, cleanupConfirmed: false, phase: 'starting', elapsedMs: 0,
		audioChunks: 0, sourceChunks: 0, screenshots: 0, screenshotsPaused: false,
		lastScreenshotError: null, error: null, ...extra };
}
const sources = { microphones: [{ id: 'mic-1', name: 'Fixture microphone' }], displays: [{ id: 1, width: 1920, height: 1080, main: true }], screenSupported: true };
function fixture() {
	const calls: { command: string; args?: Record<string, unknown> }[] = [];
	const events: string[] = [];
	const hooks = {
		restore: async (): Promise<Result<void, { message: string }>> => Ok(undefined),
		request: async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
			if (command === 'meeting_sources') return sources;
			if (command === 'meeting_request_permission') return true;
			if (command === 'meeting_start') return progress();
			if (command === 'meeting_stop') return progress({ phase: 'stopping' });
			return null;
		},
	};
	const service = createMeetingControls({
		restore: async () => { events.push('restore'); return hooks.restore(); },
		request: async <T>(command: string, args?: Record<string, unknown>) => {
			events.push(command); calls.push({ command, args });
			return await hooks.request(command, args) as T;
		},
	});
	return { service, hooks, calls, events };
}

describe('meeting user-action controls (synthetic IPC; no devices, permissions or providers)', () => {
	test('constructing/status observation performs no source enumeration, permission request or capture', async () => {
		const f = fixture(); expect(f.calls).toHaveLength(0);
		expect((await f.service.getStatus()).data).toEqual({ progress: null, requiresRestart: false });
		expect(f.events).toEqual(['meeting_status']);
	});
	test('consent, bounds, source combinations and unexpected paths are rejected before restoration/capture', async () => {
		const f = fixture();
		for (const input of [
			{ ...params, confirmed: false }, { ...params, title: ' ' }, { ...params, title: 'a'.repeat(201) },
			{ ...params, mode: 'system-and-microphone' as const }, { ...params, displayId: 0 },
			{ ...params, displayId: 1.5 }, { ...params, microphoneId: 'bad\0id' },
			{ ...params, microphoneId: 'あ'.repeat(200) }, { ...params, outputFolder: '/outside' },
		]) expect((await f.service.start(input)).error).toBeTruthy();
		expect(f.events).toHaveLength(0);
		expect(validateMeetingStart({ ...params, title: ' あ'.repeat(100) }).error).toBeNull();
	});
	test('Start awaits restoration and sends only explicit native-owned input fields', async () => {
		const f = fixture(); const ready = deferred<Result<void, { message: string }>>();
		f.hooks.restore = () => ready.promise;
		const starting = f.service.start({ ...params, title: ' Meeting ' });
		expect(f.events).toEqual(['restore']); ready.resolve(Ok(undefined));
		expect((await starting).data).toMatchObject({ phase: 'starting', captureReserved: true });
		expect(f.calls).toEqual([{ command: 'meeting_start', args: { params } }]);
	});
	test('unverified dictation restoration never dispatches meeting Start or permission requests', async () => {
		const f = fixture(); f.hooks.restore = async () => Err({ message: 'unverified FFmpeg session' });
		expect((await f.service.start(params)).error?.message).toContain('unverified'); expect(f.calls).toHaveLength(0);
	});
	test('Stop during restoration cancels Start without stopping an unrelated native meeting', async () => {
		const f = fixture(); const ready = deferred<Result<void, { message: string }>>();
		f.hooks.restore = () => ready.promise;
		const starting = f.service.start(params); const stopping = f.service.stop();
		expect((await f.service.start(params)).error).toBeTruthy(); expect(f.calls).toHaveLength(0);
		ready.resolve(Ok(undefined));
		expect((await starting).error?.message).toContain('cancelled'); expect((await stopping).error).toBeNull();
		expect(f.calls.map((c) => c.command)).toEqual(['meeting_status']);
	});
	test('Stop after Start dispatch waits for acknowledgement and shares one Stop request', async () => {
		const f = fixture(); const entered = deferred<void>(); const started = deferred<unknown>();
		const original = f.hooks.request;
		f.hooks.request = async (command, args) => {
			if (command === 'meeting_start') { entered.resolve(); return started.promise; }
			return original(command, args);
		};
		const starting = f.service.start(params); await entered.promise;
		const first = f.service.stop(); const second = f.service.stop(); expect(first).toBe(second);
		expect((await f.service.requestPermission('screen')).error).toBeTruthy();
		expect(f.calls.map((c) => c.command)).toEqual(['meeting_start']);
		started.resolve(progress()); await starting;
		expect((await first).data?.progress).toMatchObject({ phase: 'stopping', captureReserved: true });
		expect(f.calls.map((c) => c.command)).toEqual(['meeting_start', 'meeting_stop']);
	});
	test('lost Start acknowledgement stays quarantined after idle/Stop responses until app restart', async () => {
		const f = fixture(); f.hooks.request = async (command) => {
			if (command === 'meeting_start') throw new Error('transport lost'); return null;
		};
		expect((await f.service.start(params)).error).toBeTruthy();
		expect((await f.service.stop()).data).toEqual({ progress: null, requiresRestart: true });
		expect((await f.service.start(params)).error?.message).toContain('Quit/relaunch');
		expect(f.calls.filter((c) => c.command === 'meeting_start')).toHaveLength(1);
	});
	test('known native admission refusal is not automatically retried or turned into a permission prompt', async () => {
		const f = fixture(); f.hooks.request = async () => { throw { name: 'MeetingCommandError', message: 'Stop dictation first' }; };
		expect((await f.service.start(params)).error).toBeTruthy();
		expect(f.calls.map((c) => c.command)).toEqual(['meeting_start']);
		f.hooks.request = async () => null;
		expect((await f.service.getStatus()).data?.requiresRestart).toBe(false);
	});
	test('malformed or mismatched native Start acknowledgement cannot be accepted as success', async () => {
		for (const response of [null, progress({ title: 'Wrong meeting' }), progress({ displayId: 2 }), progress({ captureReserved: false })]) {
			const f = fixture(); f.hooks.request = async () => response;
			expect((await f.service.start(params)).error).toBeTruthy();
			f.hooks.request = async () => null;
			expect((await f.service.getStatus()).data?.requiresRestart).toBe(true);
		}
	});
	test('native status is never called IDLE based on terminal phase alone or malformed counters', async () => {
		const f = fixture(); f.hooks.request = async () => progress({ phase: 'failed', error: 'closure unknown' });
		expect((await f.service.getStatus()).data?.progress?.captureReserved).toBe(true);
		for (const response of [progress({ elapsedMs: -1 }), progress({ screenshots: 0.5 }), progress({ phase: 'recording', captureReserved: false, cleanupConfirmed: true }), progress({ phase: 'completed', captureReserved: false, cleanupConfirmed: false })]) {
			f.hooks.request = async () => response; expect((await f.service.getStatus()).error).toBeTruthy();
		}
		f.hooks.request = async () => progress({ phase: 'completed', captureReserved: false, cleanupConfirmed: true });
		expect((await f.service.getStatus()).data?.progress?.captureReserved).toBe(false);
	});
	test('permission requests are explicit and a false or invalid response is not treated as granted', async () => {
		const f = fixture(); f.hooks.request = async () => false;
		expect((await f.service.requestPermission('microphone')).data).toBe(false);
		expect(f.calls).toEqual([{ command: 'meeting_request_permission', args: { kind: 'microphone' } }]);
		f.hooks.request = async () => 'true'; expect((await f.service.requestPermission('screen')).error).toBeTruthy();
		expect(f.events).not.toContain('restore');
	});
	test('pending permission UI cannot race Start or launch a second OS request, while Stop stays available', async () => {
		const f = fixture(); const entered = deferred<void>(); const granted = deferred<unknown>();
		f.hooks.request = async (command) => {
			if (command === 'meeting_request_permission') { entered.resolve(); return granted.promise; }
			return null;
		};
		const permission = f.service.requestPermission('microphone'); await entered.promise;
		expect((await f.service.start(params)).error).toBeTruthy();
		expect((await f.service.requestPermission('screen')).error).toBeTruthy();
		expect((await f.service.stop()).error).toBeNull();
		granted.resolve(false);
		expect((await permission).data).toBe(false);
		expect(f.calls.map((c) => c.command)).toEqual(['meeting_request_permission', 'meeting_stop']);
	});
	test('manual screenshots and pause forward only narrow explicit controls, not a schedule/timer/provider request', async () => {
		const f = fixture(); expect((await f.service.captureNow()).error).toBeNull();
		expect((await f.service.pauseScreenshots(true)).error).toBeNull();
		expect((await f.service.pauseScreenshots(false)).error).toBeNull();
		expect(f.calls).toEqual([{ command: 'meeting_capture_now', args: undefined }, { command: 'meeting_pause_screenshots', args: { paused: true } }, { command: 'meeting_pause_screenshots', args: { paused: false } }]);
	});
	test('source enumeration is explicit, validated, and does not change ordinary settings', async () => {
		const f = fixture(); expect((await f.service.loadSources()).data).toEqual(sources);
		expect(f.calls).toEqual([{ command: 'meeting_sources', args: undefined }]);
		f.hooks.request = async () => ({ ...sources, displays: [{ id: 0, width: 1, height: 1, main: true }] });
		expect((await f.service.loadSources()).error).toBeTruthy();
	});
	test('Start availability requires known native release, explicit sources, consent and available selection', () => {
		const idle = { progress: null, requiresRestart: false };
		expect(meetingStartAvailability(params, sources, undefined)).not.toBeNull();
		expect(meetingStartAvailability(params, sources, { ...idle, requiresRestart: true })).not.toBeNull();
		expect(meetingStartAvailability(params, undefined, idle)).not.toBeNull();
		expect(meetingStartAvailability({ ...params, microphoneId: 'missing' }, sources, idle)).not.toBeNull();
		expect(meetingStartAvailability({ ...params, confirmed: false }, sources, idle)).not.toBeNull();
		expect(meetingStartAvailability(params, sources, idle)).toBeNull();
	});
});
