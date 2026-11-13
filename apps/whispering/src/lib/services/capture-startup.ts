import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync } from 'wellcrafted/result';
import { RecorderServiceErr, type RecorderServiceError } from './recorder/types';

/** One inspection/acknowledgement shared by concurrent admission attempts.
 * A failed inspection is sticky: another backend must not bypass it.
 * Explicit stale-session recovery will provide a separate, user-driven path.
 */
export function createCaptureStartup({
	restore,
	confirm,
}: {
	restore: () => Promise<Result<void, RecorderServiceError>>;
	confirm: () => Promise<void>;
}) {
	let pending: Promise<Result<void, RecorderServiceError>> | undefined;
	return {
		wait() {
			pending ??= (async () => {
				const attempted = await tryAsync({
					try: async () => {
						const restored = await restore();
						if (restored.error) return Err(restored.error);
						await confirm();
						return Ok(undefined);
					},
					catch: (error) =>
						RecorderServiceErr({ message: extractErrorMessage(error) }),
				});
				return attempted.error ? Err(attempted.error) : attempted.data;
			})();
			return pending;
		},
	};
}
