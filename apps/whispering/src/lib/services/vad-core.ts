import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync, trySync } from 'wellcrafted/result';
import type { VadState } from '$lib/constants/audio';
import type { CaptureLease } from './capture-lease';
import type { Device, DeviceAcquisitionOutcome, DeviceIdentifier, UpdateStatusMessageFn } from './types';
import { RecorderServiceErr, type RecorderServiceError } from './recorder/types';

export type VadCallbacks = {
	onSpeechStart: () => void;
	onSpeechEnd: (audio: Float32Array) => void;
	onVADMisfire?: () => void;
	onSpeechRealStart?: () => void;
};
type VadHandle = { start(): void | Promise<void>; destroy(): void | Promise<void> };
type Dependencies = {
	lease: CaptureLease;
	acquireStream: (params: { selectedDeviceId: DeviceIdentifier | null; sendStatus: UpdateStatusMessageFn }) => Promise<Result<{ stream: MediaStream; deviceOutcome: DeviceAcquisitionOutcome }, { message: string }>>;
	create: (stream: MediaStream, callbacks: VadCallbacks) => Promise<VadHandle>;
	enumerateDevices: () => Promise<Result<Device[], { message: string }>>;
	cleanupStream: (stream: MediaStream) => void;
	cleanupPendingStreams: () => void;
	onState: (state: VadState) => void;
};
const failed = (error: unknown) => RecorderServiceErr({ message: extractErrorMessage(error) });

/** One lease spans acquisition, model initialization, all speech chunks, and
 * actual cleanup. Failed cleanup retains its handle/stream/lease for Stop retry.
 * Stop during initialization waits for ownership transfer, then cancels it.
 * No models, devices or timers are created by constructing this core.
 */
export function createVadRecorderCore(deps: Dependencies) {
	let instance: VadHandle | null = null;
	let stream: MediaStream | null = null;
	let busy = false;
	let cleanupRequired = false;
	let stopRequested = false;
	let stopping = false;
	let callbacksEnabled = false;
	let generation = 0;
	let startTask: Promise<Result<DeviceAcquisitionOutcome, RecorderServiceError>> | null = null;
	let stopTask: Promise<Result<void, RecorderServiceError>> | null = null;
	async function call<T>(action: () => Promise<Result<T, RecorderServiceError>>) {
		const result = await tryAsync({ try: action, catch: failed });
		return result.error ? Err(result.error) : result.data;
	}
	async function cleanup() {
		stopping = true;
		deps.onState('STOPPING');
		const destroyed = await tryAsync({
			try: async () => { if (instance) await instance.destroy(); },
			catch: failed,
		});
		// Keep normal Stop's final onSpeechEnd flush, but suppress late worklet
		// events after destroy (including a failed destroy). Never resurrect UI.
		callbacksEnabled = false;
		if (!destroyed.error) instance = null;
		const closed = trySync({
			try: () => { if (stream) deps.cleanupStream(stream); },
			catch: failed,
		});
		if (!closed.error) stream = null;
		const pending = trySync({ try: deps.cleanupPendingStreams, catch: failed });
		const error = destroyed.error ?? closed.error ?? pending.error;
		if (error) {
			cleanupRequired = true;
			deps.onState('CLEANUP_REQUIRED');
			return Err(error);
		}
		const released = await deps.lease.release();
		if (released.error) {
			cleanupRequired = true;
			deps.onState('CLEANUP_REQUIRED');
			return RecorderServiceErr({ message: released.error.message });
		}
		cleanupRequired = false;
		deps.onState('IDLE');
		return Ok(undefined);
	}
	return {
		start(params: { selectedDeviceId: DeviceIdentifier | null; sendStatus: UpdateStatusMessageFn }, callbacks: VadCallbacks) {
			if (busy || stopTask || instance || stream || cleanupRequired || deps.lease.held)
				return Promise.resolve(RecorderServiceErr({ message: 'VAD is active or cleanup is pending. Stop it before starting another session.' }));
			busy = true;
			deps.onState('STARTING');
			stopRequested = false;
			stopping = false;
			callbacksEnabled = false;
			const session = ++generation;
			const cancelled = () => RecorderServiceErr({ message: 'VAD startup was cancelled by Stop.' });
			startTask = (async () => {
				const result = await call(async () => {
					const admitted = await deps.lease.acquire();
					if (admitted.error) return RecorderServiceErr({ message: admitted.error.message });
					if (stopRequested) return cancelled();
					const acquired = await deps.acquireStream(params);
					if (acquired.error) return RecorderServiceErr({ message: acquired.error.message });
					stream = acquired.data.stream;
					if (stopRequested) return cancelled();
					const current = () => callbacksEnabled && generation === session;
					instance = await deps.create(stream, {
						onSpeechStart: () => {
							if (!current() || stopping) return;
							deps.onState('SPEECH_DETECTED');
							callbacks.onSpeechStart();
						},
						onSpeechEnd: (audio) => {
							if (!current()) return;
							if (!stopping) deps.onState('LISTENING');
							callbacks.onSpeechEnd(audio);
						},
						onVADMisfire: () => {
							if (!current() || stopping) return;
							deps.onState('LISTENING');
							callbacks.onVADMisfire?.();
						},
						onSpeechRealStart: () => {
							if (current() && !stopping) callbacks.onSpeechRealStart?.();
						},
					});
					if (stopRequested) return cancelled();
					callbacksEnabled = true;
					await instance.start();
					if (stopRequested) return cancelled();
					deps.onState('LISTENING');
					return Ok(acquired.data.deviceOutcome);
				});
				let final = result;
				if (result.error) {
					callbacksEnabled = false; // failed/cancelled Start never flushes speech
					const cleaned = await cleanup();
					if (cleaned.error) final = RecorderServiceErr({ message: `${result.error.message}; cleanup: ${cleaned.error.message}. Retry Stop.` });
				}
				busy = false;
				startTask = null;
				return final;
			})();
			return startTask;
		},
		stop() {
			if (stopTask) return stopTask;
			if (busy && !startTask)
				return Promise.resolve(RecorderServiceErr({ message: 'Device enumeration is still pending. Retry Stop after it finishes.' }));
			stopRequested = true;
			deps.onState('STOPPING');
			// Close known tracks even while model initialization is still pending.
			// Keep references/lease until the late handle is drained and cleaned.
			if (startTask) {
				trySync({
					try: () => { if (stream) deps.cleanupStream(stream); },
					catch: failed,
				});
				trySync({ try: deps.cleanupPendingStreams, catch: failed });
			}
			stopTask = (async () => {
				if (startTask) await startTask;
				busy = true;
				const result = await cleanup();
				busy = false;
				stopTask = null;
				return result;
			})();
			return stopTask;
		},
		async enumerateDevices() {
			if (busy || stopTask || instance || stream || cleanupRequired || deps.lease.held)
				return RecorderServiceErr({ message: 'Stop VAD/capture before enumerating microphones.' });
			busy = true;
			const result = await call(async () => {
				const admitted = await deps.lease.acquire();
				if (admitted.error) return RecorderServiceErr({ message: admitted.error.message });
				const listed = await call(async () => {
					const devices = await deps.enumerateDevices();
					return devices.error ? RecorderServiceErr({ message: devices.error.message }) : Ok(devices.data);
				});
				const cleaned = await cleanup();
				return cleaned.error ? Err(cleaned.error) : listed;
			});
			busy = false;
			return result;
		},
	};
}
