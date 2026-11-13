import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, tryAsync } from 'wellcrafted/result';
import { RecorderServiceErr } from './recorder/types';

/** Lazy import avoids the services/FFmpeg/device-stream initialization cycle.
 * Browser and non-Mac dictation do not run this fork's Mac restoration protocol.
 * This is a startup barrier, not the capture lifetime lease.
 */
export async function waitForCaptureStartup() {
	if (typeof window === 'undefined' || !window.__TAURI_INTERNALS__)
		return Ok(undefined);
	const attempted = await tryAsync({
		try: async () => {
			const { PLATFORM_TYPE } = await import('$lib/constants/platform');
			if (PLATFORM_TYPE !== 'macos') return Ok(undefined);
			const { FfmpegRecorderServiceLive } = await import('./recorder/ffmpeg');
			return FfmpegRecorderServiceLive.waitForStartup();
		},
		catch: (error) =>
			RecorderServiceErr({ message: extractErrorMessage(error) }),
	});
	return attempted.error ? Err(attempted.error) : attempted.data;
}
