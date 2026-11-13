import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, tryAsync } from 'wellcrafted/result';

export type CaptureLeaseError = { name: 'CaptureLeaseError'; message: string };
export type CaptureLease = ReturnType<typeof createCaptureLease>;
type Request = <T>(
	command: string,
	args?: Record<string, unknown>,
) => Promise<T>;
const failure = (message: string) =>
	Err({ name: 'CaptureLeaseError', message } as const);

/** Native admission, held before any device opens and through actual cleanup.
 * No timeout/automatic expiry: a slow start/stop must not admit a meeting.
 */
export function createCaptureLease({
	request,
	enabled,
}: {
	request: Request;
	enabled: boolean;
}) {
	let token: string | null = null;
	let pending = false;
	return {
		get held() {
			return token !== null || pending;
		},
		async acquire() {
			if (!enabled) return Ok(undefined);
			if (pending || token !== null)
				return failure(
					'Recording is already active or a capture operation is pending. Stop/cancel it first.',
				);
			pending = true;
			const result = await tryAsync({
				try: () => request<string>('capture_acquire_dictation'),
				catch: (error) => failure(extractErrorMessage(error)),
			});
			pending = false;
			if (result.error) return result;
			if (typeof result.data !== 'string' || !result.data)
				return failure(
					'Native capture admission returned an invalid token. Restart the app before recording.',
				);
			token = result.data;
			return Ok(undefined);
		},
		async release() {
			if (!enabled || (!pending && token === null)) return Ok(undefined);
			if (pending)
				return failure('Capture admission/release is still pending.');
			pending = true;
			const result = await tryAsync({
				try: () => request<void>('capture_release_dictation', { token }),
				catch: (error) => failure(extractErrorMessage(error)),
			});
			pending = false;
			if (result.error) return result;
			token = null; // only an acknowledged release frees local ownership
			return Ok(undefined);
		},
	};
}
