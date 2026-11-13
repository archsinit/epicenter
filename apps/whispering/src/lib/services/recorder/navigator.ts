import { TIMESLICE_MS } from '$lib/constants/audio';
import {
	cleanupRecordingStream,
	createDeviceEnumeration,
	getRecordingStream,
} from '../device-stream';
import { createNavigatorRecorderCore } from './navigator-core';
import type { RecorderService } from './types';

export function createNavigatorRecorderService(): RecorderService & { cleanupDeviceEnumeration(): void } {
	const devices = createDeviceEnumeration();
	const core = createNavigatorRecorderCore({
		acquireStream: (params) => getRecordingStream({
			...params,
			enumerateForFallback: devices.enumerateDevices,
		}),
		enumerateDevices: devices.enumerateDevices,
		cleanupPendingStreams: devices.cleanup,
		hasPendingStreams: () => devices.held,
		cleanupStream: cleanupRecordingStream,
		createRecorder: (stream, bitsPerSecond) =>
			new MediaRecorder(stream, { bitsPerSecond }),
		timesliceMs: TIMESLICE_MS,
	});
	return { ...core, cleanupDeviceEnumeration: devices.cleanup };
}

export const NavigatorRecorderServiceLive = createNavigatorRecorderService();
