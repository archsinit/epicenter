import { type } from 'arktype';
import { createTaggedError, extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, tryAsync } from 'wellcrafted/result';
import { isLocalMeetingId } from './library';

const Preview = type({
	sourceId: 'string',
	durationMs: 'number',
	sourceChunks: 'number',
	unregisteredChunks: 'number',
	ignoredMixedChunks: 'number',
	emptyChunks: 'number',
	normalizedHeaders: 'number',
	discardedPartialSampleBytes: 'number',
	durationMismatches: 'number',
	additionalBytesEstimate: 'number',
	microphoneChunks: 'number',
	systemChunks: 'number',
});
const Status = type({
	token: 'string',
	sourceId: 'string',
	phase: "'preparing'|'ready'|'running'|'completed'|'failed'|'cancelled'",
	preview: Preview.or('null'),
	derivedId: 'string|null',
	error: 'string|null',
});
export type RecoveryStatus = typeof Status.infer;
const { MeetingRecoveryErr } = createTaggedError('MeetingRecoveryError');
type Request = <T>(
	command: string,
	args?: Record<string, unknown>,
) => Promise<T>;
export function createMeetingRecovery(
	request: Request,
	restore: () => Promise<void> = async () => {},
) {
	async function call(
		command: string,
		args?: Record<string, unknown>,
		restoring = false,
	) {
		return tryAsync({
			try: async () => {
				if (restoring) await restore();
				return request<unknown>(command, args);
			},
			catch: (error) =>
				MeetingRecoveryErr({ message: extractErrorMessage(error) }),
		});
	}
	function decode(raw: unknown, sourceId?: string, token?: string) {
		const s = Status(raw);
		if (
			s instanceof type.errors ||
			!isLocalMeetingId(s.token) ||
			!isLocalMeetingId(s.sourceId) ||
			(sourceId && s.sourceId !== sourceId) ||
			(token && s.token !== token) ||
			(s.derivedId !== null && !isLocalMeetingId(s.derivedId)) ||
			(s.preview &&
				(s.preview.sourceId !== s.sourceId ||
					Object.values(s.preview).some(
						(v) => typeof v === 'number' && (!Number.isSafeInteger(v) || v < 0),
					) ||
					s.preview.durationMs > 86400000 ||
					s.preview.sourceChunks === 0)) ||
			(['ready', 'running', 'completed', 'cancelled'].includes(s.phase) &&
				!s.preview) ||
			(s.phase === 'completed') !== (s.derivedId !== null) ||
			(s.phase === 'failed') !== (s.error !== null)
		)
			return MeetingRecoveryErr({
				message:
					'Invalid recovery acknowledgement. Observe native status; do not retry execution blindly.',
			});
		return Ok(s);
	}
	return {
		async status() {
			const r = await call('meeting_recovery_status');
			if (r.error) return Err(r.error);
			return r.data === null ? Ok(null) : decode(r.data);
		},
		async prepare(id: string) {
			if (!isLocalMeetingId(id))
				return MeetingRecoveryErr({ message: 'Invalid source meeting.' });
			const r = await call('meeting_prepare_recovery', { id }, true);
			if (r.error) return Err(r.error);
			return decode(r.data, id);
		},
		async execute(token: string, confirmed: boolean) {
			if (!isLocalMeetingId(token) || confirmed !== true)
				return MeetingRecoveryErr({
					message: 'Review and confirm recovery first.',
				});
			const r = await call(
				'meeting_execute_recovery',
				{ token, confirmed: true },
				true,
			);
			if (r.error) return Err(r.error);
			return decode(r.data, undefined, token);
		},
		async cancel(token: string) {
			if (!isLocalMeetingId(token))
				return MeetingRecoveryErr({ message: 'Invalid review token.' });
			const r = await call('meeting_cancel_recovery', { token });
			if (r.error) return Err(r.error);
			return decode(r.data, undefined, token);
		},
	};
}
