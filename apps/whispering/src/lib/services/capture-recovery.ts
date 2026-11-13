import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync } from 'wellcrafted/result';
import { RecorderServiceErr, type RecorderServiceError } from './recorder/types';

export type CaptureRecoveryStatus = {
	blocked: boolean;
	hasSavedSession: boolean;
	reason: string | null;
};
export type CaptureRecoveryOutcome =
	| { status: 'cancelled' }
	| { status: 'restart-required'; archivePath: string };

/** Metadata-only recovery. Native confirmation and durable archive must precede
 * clearing the matching saved record. No release/reset of capture admission:
 * full application Quit/relaunch is mandatory, even after successful recovery.
 */
export function createCaptureRecovery({
	ready,
	readSaved,
	confirmAndArchive,
	clearSaved,
}: {
	ready: () => Promise<Result<void, RecorderServiceError>>;
	readSaved: () => string | null;
	confirmAndArchive: (raw: string) => Promise<string | null>;
	clearSaved: () => void;
}) {
	let busy = false;
	async function guarded<T>(operation: () => Promise<Result<T, RecorderServiceError>>) {
		const result = await tryAsync({
			try: operation,
			catch: (error) => RecorderServiceErr({ message: extractErrorMessage(error) }),
		});
		return result.error ? Err(result.error) : result.data;
	}
	return {
		getStatus(): Promise<Result<CaptureRecoveryStatus, RecorderServiceError>> {
			return guarded(async () => {
				const restored = await ready();
				return Ok({
					blocked: Boolean(restored.error),
					hasSavedSession: readSaved() !== null,
					reason: restored.error?.message ?? null,
				});
			});
		},
		async recover(): Promise<Result<CaptureRecoveryOutcome, RecorderServiceError>> {
			if (busy) return RecorderServiceErr({ message: 'Saved-session recovery is already in progress.' });
			busy = true;
			const result = await guarded<CaptureRecoveryOutcome>(async () => {
				const restored = await ready();
				if (!restored.error)
					return RecorderServiceErr({ message: 'Capture startup is healthy. Stop the active recorder normally; recovery is not a Stop command.' });
				const raw = readSaved();
				if (raw === null)
					return RecorderServiceErr({ message: 'No saved metadata remains. Fully Quit and relaunch after independently stopping the earlier recorder.' });
				const archivePath = await confirmAndArchive(raw);
				if (archivePath === null) return Ok({ status: 'cancelled' });
				if (typeof archivePath !== 'string' || !archivePath)
					return RecorderServiceErr({ message: 'Native recovery did not acknowledge a saved archive. Metadata was retained.' });
				if (readSaved() !== raw)
					return RecorderServiceErr({ message: 'Saved recording metadata changed during confirmation. The newer record was retained; do not start another capture.' });
				clearSaved();
				if (readSaved() !== null)
					return RecorderServiceErr({ message: 'Saved metadata could not be cleared. The local archive was kept; capture remains blocked.' });
				return Ok({ status: 'restart-required', archivePath });
			});
			busy = false;
			return result;
		},
	};
}
