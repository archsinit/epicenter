import { MicVAD, utils } from '@ricky0123/vad-web';
import { Ok } from 'wellcrafted/result';
import type { VadState } from '$lib/constants/audio';
import { WhisperingErr } from '$lib/result';
import { createNativeCaptureLease } from '$lib/services/capture-lease-live';
import {
	cleanupRecordingStream,
	createDeviceEnumeration,
	getRecordingStream,
} from '$lib/services/device-stream';
import { createVadRecorderCore } from '$lib/services/vad-core';
import { settings } from '$lib/stores/settings.svelte';
import { defineQuery } from './_client';

/** Reactive adapter only. The injected service owns capture across every VAD
 * speech chunk, startup, enumeration, Stop and retryable cleanup.
 */
function createVadRecorder() {
	let state = $state<VadState>('IDLE');
	const devices = createDeviceEnumeration();
	const core = createVadRecorderCore({
		lease: createNativeCaptureLease(),
		acquireStream: (params) => getRecordingStream({
			...params,
			enumerateForFallback: devices.enumerateDevices,
		}),
		create: (stream, callbacks) => MicVAD.new({
			stream,
			...callbacks,
			submitUserSpeechOnPause: true,
			model: 'v5',
		}),
		enumerateDevices: devices.enumerateDevices,
		cleanupStream: cleanupRecordingStream,
		cleanupPendingStreams: devices.cleanup,
		onState: (value) => { state = value; },
	});
	return {
		get state(): VadState { return state; },
		enumerateDevices: defineQuery({
			queryKey: ['vad', 'devices'],
			queryFn: async () => {
				const result = await core.enumerateDevices();
				if (result.error) return WhisperingErr({
					title: '❌ Failed to enumerate devices',
					serviceError: result.error,
				});
				return Ok(result.data);
			},
		}),
		async startActiveListening({
			onSpeechStart,
			onSpeechEnd,
			onVADMisfire,
			onSpeechRealStart,
		}: {
			onSpeechStart: () => void;
			onSpeechEnd: (blob: Blob) => void;
			onVADMisfire?: () => void;
			onSpeechRealStart?: () => void;
		}) {
			const result = await core.start({
				selectedDeviceId: settings.value['recording.navigator.deviceId'],
				sendStatus: (status) => console.log('VAD getRecordingStream status update:', status),
			}, {
				onSpeechStart,
				onSpeechEnd: (audio) => onSpeechEnd(new Blob([utils.encodeWAV(audio)], { type: 'audio/wav' })),
				onVADMisfire,
				onSpeechRealStart,
			});
			if (result.error) return WhisperingErr({
				title: '❌ Could not start VAD',
				serviceError: result.error,
			});
			return Ok(result.data);
		},
		async stopActiveListening() {
			const result = await core.stop();
			if (result.error) return WhisperingErr({
				title: '❌ VAD cleanup did not complete',
				serviceError: result.error,
			});
			return Ok(undefined);
		},
	};
}
export const vadRecorder = createVadRecorder();
