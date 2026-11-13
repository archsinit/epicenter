import { type } from 'arktype';
import { createTaggedError, extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync } from 'wellcrafted/result';

const Mode = type("'microphone' | 'system-and-microphone'");
const Start = type({ title: 'string', mode: Mode, microphoneId: 'string', displayId: 'number | null', confirmed: 'boolean' });
const Sources = type({ microphones: type({ id: 'string', name: 'string' }).array(), displays: type({ id: 'number', width: 'number', height: 'number', main: 'boolean' }).array(), screenSupported: 'boolean' });
const Progress = type({
	id: 'string', title: 'string', mode: Mode, microphoneId: 'string', displayId: 'number | null',
	captureReserved: 'boolean', cleanupConfirmed: 'boolean',
	phase: "'starting' | 'recording' | 'stopping' | 'mixing' | 'completed' | 'interrupted' | 'failed'",
	elapsedMs: 'number', audioChunks: 'number', sourceChunks: 'number', screenshots: 'number', screenshotsPaused: 'boolean',
	lastScreenshotError: 'string | null', error: 'string | null',
});
export type StartMeetingParams = typeof Start.infer;
export type MeetingSources = typeof Sources.infer;
export type MeetingProgress = typeof Progress.infer;
export type MeetingControlStatus = { progress: MeetingProgress | null; requiresRestart: boolean };
const { MeetingControlServiceErr } = createTaggedError('MeetingControlServiceError');
type Error = ReturnType<typeof MeetingControlServiceErr>['error'];
type Request = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
const natural = (n: number) => Number.isSafeInteger(n) && n >= 0;
const displayId = (n: number | null) => n === null || (natural(n) && n > 0 && n <= 0xffffffff);
const failure = (message: string) => MeetingControlServiceErr({ message });

export function validateMeetingStart(raw: unknown) {
	const input = Start(raw);
	if (input instanceof type.errors) return failure('Invalid meeting title/source/consent fields.');
	if (Object.keys(input).some((key) => !['title', 'mode', 'microphoneId', 'displayId', 'confirmed'].includes(key))) return failure('Meeting paths and IDs are native-owned; unexpected Start fields are not allowed.');
	const title = input.title.trim();
	if (!title || [...title].length > 200) return failure('Use a meeting title of 1–200 characters.');
	if (!input.confirmed) return failure('Confirm participant recording/screenshot consent before Start.');
	if (new TextEncoder().encode(input.microphoneId).length > 512 || input.microphoneId.includes('\0')) return failure('Invalid microphone identifier.');
	if (!displayId(input.displayId) || (input.mode === 'system-and-microphone' && input.displayId === null)) return failure('Choose a display for system audio/screenshots.');
	return Ok({ title, mode: input.mode, microphoneId: input.microphoneId, displayId: input.displayId, confirmed: input.confirmed });
}
export function meetingStartAvailability(input: StartMeetingParams, sources: MeetingSources | undefined, status: MeetingControlStatus | undefined) {
	if (!status) return 'Wait for verified native status before Start.';
	if (status.requiresRestart) return 'A prior Start acknowledgement is unknown. Request Stop, then fully Quit and relaunch before another Start.';
	if (status.progress?.captureReserved) return 'Stop/finalize the existing meeting first. Unknown cleanup requires full Quit/relaunch.';
	if (!sources?.screenSupported) return 'Load sources; meeting mode requires macOS 14 or newer.';
	if (!sources.microphones.length) return 'Connect/authorize a microphone and refresh sources.';
	const validated = validateMeetingStart(input);
	if (validated.error) return validated.error.message;
	if (input.microphoneId && !sources.microphones.some((m) => m.id === input.microphoneId)) return 'Refresh sources and choose an available microphone.';
	if (input.displayId !== null && !sources.displays.some((d) => d.id === input.displayId)) return 'Refresh sources and choose an available display.';
	return null;
}
function parseProgress(raw: unknown) {
	const p = Progress(raw);
	if (p instanceof type.errors || !/^[a-zA-Z0-9-]{1,96}$/.test(p.id) || !displayId(p.displayId)
		|| (p.mode === 'system-and-microphone' && p.displayId === null) || !p.title.trim() || [...p.title].length > 200 || p.microphoneId.includes('\0') || new TextEncoder().encode(p.microphoneId).length > 512
		|| ![p.elapsedMs, p.audioChunks, p.sourceChunks, p.screenshots].every(natural)
		|| (!p.captureReserved && (!p.cleanupConfirmed || ['starting', 'recording', 'stopping', 'mixing'].includes(p.phase))))
		return failure('Invalid native progress/cleanup evidence. Capture state is unknown; request Stop or fully Quit/relaunch.');
	return Ok(p);
}
/** Explicit user-action controls. Construction/status never enumerate inputs,
 * request permissions, start capture, transcribe, paste, or run an LLM. All
 * audio/screenshot clocks and finalization remain in the native controller.
 */
export function createMeetingControls({ request, restore }: {
	request: Request;
	restore: () => Promise<Result<void, { message: string }>>;
}) {
	let uncertainStart = false;
	let permissionPending = false;
	type StartFlight = { cancelled: boolean; sent: boolean; task: Promise<Result<MeetingProgress, Error>> | null };
	let startFlight: StartFlight | null = null;
	let stopFlight: Promise<Result<MeetingControlStatus, Error>> | null = null;
	async function read(command: string, args?: Record<string, unknown>, starting = false) {
		return tryAsync({
			try: () => request<unknown>(command, args),
			catch: (error) => {
				// A structured native rejection completed its command. A transport
				// error or malformed acknowledgement cannot prove Start was absent.
				if (starting && !(typeof error === 'object' && error !== null && 'name' in error && error.name === 'MeetingCommandError' && 'message' in error && typeof error.message === 'string')) uncertainStart = true;
				return failure(extractErrorMessage(error));
			},
		});
	}
	async function status(command = 'meeting_status'): Promise<Result<MeetingControlStatus, Error>> {
		const result = await read(command);
		if (result.error) return Err(result.error);
		if (result.data === null) return Ok({ progress: null, requiresRestart: uncertainStart });
		const parsed = parseProgress(result.data);
		if (parsed.error) return Err(parsed.error);
		return Ok({ progress: parsed.data, requiresRestart: uncertainStart });
	}
	async function acknowledge(command: string, args?: Record<string, unknown>) {
		const result = await read(command, args);
		if (result.error) return Err(result.error);
		if (result.data !== null && result.data !== undefined) return failure('Invalid native control acknowledgement. Check native status before another action.');
		return Ok(undefined);
	}
	return {
		getStatus: () => status(),
		async loadSources() {
			const result = await read('meeting_sources');
			if (result.error) return Err(result.error);
			const s = Sources(result.data);
			if (s instanceof type.errors || s.microphones.length > 128 || s.displays.length > 64
				|| s.displays.some((d) => !displayId(d.id) || !natural(d.width) || !natural(d.height) || !d.width || !d.height || d.width > 0xffffffff || d.height > 0xffffffff)
				|| s.microphones.some((m) => !m.id || m.id.includes('\0') || new TextEncoder().encode(m.id).length > 512)
				|| new Set(s.microphones.map((m) => m.id)).size !== s.microphones.length || new Set(s.displays.map((d) => d.id)).size !== s.displays.length) return failure('Invalid native capture-source list.');
			return Ok(s);
		},
		async requestPermission(kind: 'microphone' | 'screen') {
			if (kind !== 'microphone' && kind !== 'screen') return failure('Unknown meeting permission.');
			if (startFlight || stopFlight || permissionPending) return failure('Wait for the pending recording/permission action before requesting permissions.');
			permissionPending = true;
			const result = await read('meeting_request_permission', { kind });
			permissionPending = false;
			if (result.error) return Err(result.error);
			return typeof result.data === 'boolean' ? Ok(result.data) : failure('Invalid native permission response; no permission grant was assumed.');
		},
		start(raw: StartMeetingParams): Promise<Result<MeetingProgress, Error>> {
			const validated = validateMeetingStart(raw);
			if (validated.error) return Promise.resolve(Err(validated.error));
			if (startFlight || stopFlight || permissionPending) return Promise.resolve(failure('A meeting Start/Stop/permission request is still pending.'));
			if (uncertainStart) return Promise.resolve(failure('Prior Start acknowledgement is unknown. Request Stop, then fully Quit/relaunch; do not retry Start.'));
			const flight: StartFlight = { cancelled: false, sent: false, task: null };
			startFlight = flight;
			flight.task = (async () => {
				const attempted = await tryAsync({
					try: async () => {
						const ready = await restore();
						if (ready.error) return failure(ready.error.message);
						if (flight.cancelled) return failure('Start cancelled before opening any input.');
						flight.sent = true;
						const result = await read('meeting_start', { params: validated.data }, true);
						if (result.error) return Err(result.error);
						const parsed = parseProgress(result.data);
						if (parsed.error || !parsed.data?.captureReserved || parsed.data.title !== validated.data.title || parsed.data.mode !== validated.data.mode || parsed.data.microphoneId !== validated.data.microphoneId || parsed.data.displayId !== validated.data.displayId) {
							uncertainStart = true;
							return failure('Native Start did not acknowledge its reserved meeting. Request Stop and fully Quit/relaunch.');
						}
						return parsed;
					},
					catch: (error) => failure(extractErrorMessage(error)),
				});
				startFlight = null;
				return attempted.error ? Err(attempted.error) : attempted.data;
			})();
			return flight.task;
		},
		stop(): Promise<Result<MeetingControlStatus, Error>> {
			if (stopFlight) return stopFlight;
			const flight = startFlight;
			if (flight) flight.cancelled = true;
			stopFlight = (async () => {
				if (flight?.task) await flight.task;
				const result = await status(flight && !flight.sent ? 'meeting_status' : 'meeting_stop');
				stopFlight = null;
				return result;
			})();
			return stopFlight;
		},
		captureNow: () => acknowledge('meeting_capture_now'),
		pauseScreenshots(paused: boolean) {
			return typeof paused === 'boolean' ? acknowledge('meeting_pause_screenshots', { paused }) : Promise.resolve(failure('Invalid screenshot-pause choice.'));
		},
	};
}
