import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync } from 'wellcrafted/result';
import type { CaptureLease } from '../capture-lease';
import {
	RecorderServiceErr,
	type RecorderService,
	type RecorderServiceError,
} from './types';

/** Defer dereferencing the live clients until first use. FFmpeg imports the
 * service index itself; eagerly wrapping it here would create a TDZ when it is
 * first loaded by the lazy startup-restoration import.
 */
export function createLazyRecorderService(resolve: () => RecorderService): RecorderService {
	let resolved: RecorderService | undefined;
	const get = () => resolved ??= resolve();
	return {
		getRecorderState: () => get().getRecorderState(),
		enumerateDevices: () => get().enumerateDevices(),
		startRecording: (params, callbacks) => get().startRecording(params, callbacks),
		stopRecording: (callbacks) => get().stopRecording(callbacks),
		cancelRecording: (callbacks) => get().cancelRecording(callbacks),
	};
}

/** Mac live recorder admission. Never bypass an unverified saved session.
 *
 * Same recorder API/data. Only desktop capture admission/lifetime is added.
 * Never release on an unknown/failed cleanup and never discard a saved Blob
 * merely because releasing the native lease failed.
 */
export function createLeasedRecorderService(
	base: RecorderService,
	lease: CaptureLease,
	cleanupEnumeration?: () => void,
): RecorderService {
	let busy = false;
	let safeToRelease = false;
	async function call<T>(
		operation: () => Promise<Result<T, RecorderServiceError>>,
	): Promise<Result<T, RecorderServiceError>> {
		const result = await tryAsync({
			try: operation,
			catch: (error) =>
				RecorderServiceErr({ message: extractErrorMessage(error) }),
		});
		return result.error ? Err(result.error) : result.data;
	}
	async function guarded<T>(
		operation: () => Promise<Result<T, RecorderServiceError>>,
	): Promise<Result<T, RecorderServiceError>> {
		if (busy)
			return RecorderServiceErr({
				message: 'A recording operation is still in progress.',
			});
		busy = true;
		const result = await call(operation);
		busy = false;
		return result;
	}
	async function release() {
		const result = await lease.release();
		if (result.error)
			return RecorderServiceErr({ message: result.error.message });
		safeToRelease = false;
		return Ok(undefined);
	}
	async function afterCleanup() {
		safeToRelease = true;
		const result = await release();
		if (result.error)
			console.warn(
				'Capture lock release failed. Retry Cancel or restart the app before starting a meeting.',
			);
	}
	return {
		getRecorderState: async () => {
			const state = await call(() => base.getRecorderState());
			if (state.error) return Err(state.error);
			return Ok(lease.held ? 'RECORDING' : state.data);
		},
		enumerateDevices: () => {
			// CPAL and FFmpeg list devices without opening a recording input.
			// Navigator opens a temporary getUserMedia stream and needs admission.
			if (!cleanupEnumeration) return base.enumerateDevices();
			return guarded(async () => {
				if (safeToRelease) {
					const released = await release();
					if (released.error) return released;
				}
				const acquired = await lease.acquire();
				if (acquired.error) return RecorderServiceErr({ message: acquired.error.message });
				const result = await call(() => base.enumerateDevices());
				const cleaned = await call(async () => {
					cleanupEnumeration();
					return Ok(undefined);
				});
				if (cleaned.error) return Err(cleaned.error); // retain lease; Cancel retries cleanup
				safeToRelease = true;
				const released = await release();
				if (released.error) return Err(released.error);
				return result;
			});
		},
		startRecording: (params, callbacks) =>
			guarded(async () => {
				if (safeToRelease) {
					const released = await release();
					if (released.error) return released;
				}
				const acquired = await lease.acquire();
				if (acquired.error)
					return RecorderServiceErr({ message: acquired.error.message });
				const state = await call(() => base.getRecorderState());
				if (state.error) return Err(state.error); // unknown -> retain lease
				if (state.data !== 'IDLE')
					return RecorderServiceErr({
						message:
							'A recording is already active. Stop/cancel it before starting another.',
					});
				const started = await call(() =>
					base.startRecording(params, callbacks),
				);
				if (started.error) {
					// Only clean up this attempted start, never a pre-existing recording.
					const cleaned = await call(() => base.cancelRecording(callbacks));
					if (!cleaned.error) await afterCleanup();
				}
				return started;
			}),
		stopRecording: (callbacks) =>
			guarded(async () => {
				const stopped = await call(() => base.stopRecording(callbacks));
				if (!stopped.error) await afterCleanup();
				return stopped; // a release failure must not lose the saved audio
			}),
		cancelRecording: (callbacks) =>
			guarded(async () => {
				if (safeToRelease) {
					const released = await release();
					return released.error
						? released
						: Ok({ status: 'no-recording' as const });
				}
				const cancelled = await call(() => base.cancelRecording(callbacks));
				if (!cancelled.error) await afterCleanup();
				return cancelled;
			}),
	};
}
