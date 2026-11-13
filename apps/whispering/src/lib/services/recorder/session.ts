import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync } from 'wellcrafted/result';
import { RecorderServiceErr, type RecorderService, type RecorderServiceError } from './types';

type Callbacks = Parameters<RecorderService['stopRecording']>[0];
/** Pin the backend and recording ID through retryable cleanup. Changing a
 * preference cannot redirect Stop to another recorder or discard the old ID.
 * After audio delivery, an unacknowledged lease release needs Cancel only.
 */
export function createRecorderSession() {
	let current: { service: RecorderService; id: string | null } | null = null;
	let busy = false;
	async function call<T>(operation: () => Promise<Result<T, RecorderServiceError>>) {
		const result = await tryAsync({
			try: operation,
			catch: (error) => RecorderServiceErr({ message: extractErrorMessage(error) }),
		});
		return result.error ? Err(result.error) : result.data;
	}
	async function guarded<T>(operation: () => Promise<Result<T, RecorderServiceError>>) {
		if (busy) return RecorderServiceErr({ message: 'A recording operation is still pending. Retry Stop/Cancel when it completes.' });
		busy = true;
		const result = await call(operation);
		busy = false;
		return result;
	}
	async function clearIfIdle() {
		if (!current) return;
		const state = await call(() => current!.service.getRecorderState());
		if (!state.error && state.data === 'IDLE') current = null;
	}
	return {
		async getRecorderState(fallback: RecorderService) {
			if (busy) return Ok('RECORDING' as const);
			return call(() => (current?.service ?? fallback).getRecorderState());
		},
		start(service: RecorderService, params: Parameters<RecorderService['startRecording']>[0], callbacks: Callbacks) {
			return guarded(async () => {
				if (current) return RecorderServiceErr({ message: 'Stop/cancel the previous recording or its pending cleanup before starting another.' });
				const state = await call(() => service.getRecorderState());
				if (state.error) return Err(state.error);
				if (state.data !== 'IDLE') return RecorderServiceErr({ message: 'A pre-existing recording is active. Stop/cancel it normally before starting another.' });
				current = { service, id: params.recordingId };
				const result = await call(() => service.startRecording(params, callbacks));
				if (result.error) await clearIfIdle();
				return result;
			});
		},
		stop(fallback: RecorderService, callbacks: Callbacks) {
			return guarded(async () => {
				if (current?.id === null) return RecorderServiceErr({ message: 'Audio was already delivered. Retry Cancel to acknowledge the remaining input cleanup; do not stop/deliver it twice.' });
				const recordingId = current?.id;
				const result = await call(() => (current?.service ?? fallback).stopRecording(callbacks));
				if (result.error) return Err(result.error); // ID and owner survive retry
				if (current) current.id = null;
				await clearIfIdle();
				if (!recordingId) return RecorderServiceErr({ message: 'Recording ID is unavailable after restoration. Local audio files were retained; do not start another recording until cleanup is complete.' });
				return Ok({ blob: result.data, recordingId });
			});
		},
		cancel(fallback: RecorderService, callbacks: Callbacks) {
			return guarded(async () => {
				const result = await call(() => (current?.service ?? fallback).cancelRecording(callbacks));
				if (!result.error) current = null;
				return result;
			});
		},
	};
}
