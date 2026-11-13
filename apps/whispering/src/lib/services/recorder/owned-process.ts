import { type } from 'arktype';
import { Err, Ok, tryAsync } from 'wellcrafted/result';
import { extractErrorMessage } from 'wellcrafted/error';
import { RecorderServiceErr } from './types';

const ProcessInfo = type({
	captureId: 'string',
	pid: 'number',
	running: 'boolean',
	forced: 'boolean',
	exitCode: 'number | null',
});
export type RecordingProcessInfo = typeof ProcessInfo.infer;
type Request = <T>(
	command: string,
	args?: Record<string, unknown>,
) => Promise<T>;

/** No PID-based stop fallback. Only the native owner's acknowledged exit can
 * release an FFmpeg session; file size/IDLE flags are not closure evidence.
 */
export function createOwnedRecordingProcessClient(request: Request) {
	async function call(
		command: string,
		args: Record<string, unknown>,
		expectedId?: string,
	) {
		const response = await tryAsync({
			try: () => request<unknown>(command, args),
			catch: (error) =>
				RecorderServiceErr({ message: extractErrorMessage(error) }),
		});
		if (response.error) return Err(response.error);
		const info = ProcessInfo(response.data);
		if (
			info instanceof type.errors ||
			!/^ffmpeg-[a-zA-Z0-9-]+$/.test(info.captureId) ||
			!Number.isInteger(info.pid) ||
			info.pid <= 0 ||
			(expectedId && info.captureId !== expectedId)
		) {
			return RecorderServiceErr({
				message:
					'Invalid native recording-process acknowledgement; ownership remains reserved.',
			});
		}
		return Ok(info);
	}
	return {
		async spawn(command: string) {
			const result = await call('capture_ffmpeg_spawn', { command });
			if (result.error) return result;
			if (!result.data.running)
				return RecorderServiceErr({
					message:
						'Recording process did not start. Any output file was retained.',
				});
			return result;
		},
		async stop(captureId: string) {
			const result = await call(
				'capture_ffmpeg_stop',
				{ captureId },
				captureId,
			);
			if (result.error) return result;
			if (result.data.running)
				return RecorderServiceErr({
					message:
						'Recording process has not confirmed exit. The session and audio file were retained.',
				});
			return result;
		},
		restore(captureId?: string) {
			return call(
				'capture_ffmpeg_restore',
				{ captureId: captureId ?? null },
				captureId,
			);
		},
	};
}
