import { TIMESLICE_MS } from '$lib/constants/audio';
import {
	cleanupRecordingStream,
	enumerateDevices,
	getRecordingStream,
} from '../device-stream';
import { createNavigatorRecorderCore } from './navigator-core';
import type { RecorderService } from './types';

export function createNavigatorRecorderService(): RecorderService {
	return createNavigatorRecorderCore({
		acquireStream: getRecordingStream,
		enumerateDevices,
		cleanupStream: cleanupRecordingStream,
		createRecorder: (stream, bitsPerSecond) =>
			new MediaRecorder(stream, { bitsPerSecond }),
		timesliceMs: TIMESLICE_MS,
	});
}

export const NavigatorRecorderServiceLive = createNavigatorRecorderService();
