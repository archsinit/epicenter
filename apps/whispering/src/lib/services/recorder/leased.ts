import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync } from 'wellcrafted/result';
import type { CaptureLease } from '../capture-lease';
import {
	RecorderServiceErr,
	type RecorderService,
	type RecorderServiceError,
} from './types';

/** Not wired into the live services yet. Before activation, each backend must
 * prove cleanup really closes its input (notably FFmpeg process exit, not file
 * stabilization). VAD also needs a lease for its full listening lifetime.
 *
 * Same recorder API/data. Only desktop capture admission/lifetime is added.
 * Never release on an unknown/failed cleanup and never discard a saved Blob
 * merely because releasing the native lease failed.
 */
export function createLeasedRecorderService(
	base: RecorderService,
	lease: CaptureLease,
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
		getRecorderState: () => base.getRecorderState(),
		enumerateDevices: () => base.enumerateDevices(),
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
