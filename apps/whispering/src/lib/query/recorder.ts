import { nanoid } from 'nanoid/non-secure';
import { Ok } from 'wellcrafted/result';
import type { WhisperingRecordingState } from '$lib/constants/audio';
import { WhisperingErr } from '$lib/result';
import * as services from '$lib/services';
import { getDefaultRecordingsFolder } from '$lib/services/recorder';
import { createRecorderSession } from '$lib/services/recorder/session';
import { settings } from '$lib/stores/settings.svelte';
import { defineMutation, defineQuery, queryClient } from './_client';
import { notify } from './notify';

const recorderKeys = {
	recorderState: ['recorder', 'recorderState'] as const,
	devices: ['recorder', 'devices'] as const,
	startRecording: ['recorder', 'startRecording'] as const,
	stopRecording: ['recorder', 'stopRecording'] as const,
	cancelRecording: ['recorder', 'cancelRecording'] as const,
} as const;

// The capture owner/ID survive Stop errors and changes to recording.method.
const session = createRecorderSession();

const invalidateRecorderState = () =>
	queryClient.invalidateQueries({ queryKey: recorderKeys.recorderState });

export const recorder = {
	// Query that enumerates available recording devices with labels
	enumerateDevices: defineQuery({
		queryKey: recorderKeys.devices,
		queryFn: async () => {
			const { data, error } = await recorderService().enumerateDevices();
			if (error) {
				return WhisperingErr({
					title: '❌ Failed to enumerate devices',
					serviceError: error,
				});
			}
			return Ok(data);
		},
	}),

	// Query that returns the recorder state (IDLE or RECORDING)
	getRecorderState: defineQuery({
		queryKey: recorderKeys.recorderState,
		queryFn: async () => {
			const { data: state, error: getStateError } =
				await session.getRecorderState(recorderService());
			if (getStateError) {
				return WhisperingErr({
					title: '❌ Failed to get recorder state',
					serviceError: getStateError,
				});
			}
			return Ok(state);
		},
		initialData: 'IDLE' as WhisperingRecordingState,
	}),

	startRecording: defineMutation({
		mutationKey: recorderKeys.startRecording,
		mutationFn: async ({ toastId }: { toastId: string }) => {
			// Generate a unique recording ID that will serve as the file name
			const recordingId = nanoid();

			// Prepare recording parameters based on which method we're using
			const baseParams = {
				recordingId,
			};

			// Resolve the output folder - use default if null
			const outputFolder = window.__TAURI_INTERNALS__
				? (settings.value['recording.cpal.outputFolder'] ??
					(await getDefaultRecordingsFolder()))
				: '';

			const paramsMap = {
				navigator: {
					...baseParams,
					method: 'navigator' as const,
					selectedDeviceId: settings.value['recording.navigator.deviceId'],
					bitrateKbps: settings.value['recording.navigator.bitrateKbps'],
				},
				ffmpeg: {
					...baseParams,
					method: 'ffmpeg' as const,
					selectedDeviceId: settings.value['recording.ffmpeg.deviceId'],
					globalOptions: settings.value['recording.ffmpeg.globalOptions'],
					inputOptions: settings.value['recording.ffmpeg.inputOptions'],
					outputOptions: settings.value['recording.ffmpeg.outputOptions'],
					outputFolder,
				},
				cpal: {
					...baseParams,
					method: 'cpal' as const,
					selectedDeviceId: settings.value['recording.cpal.deviceId'],
					outputFolder,
					sampleRate: settings.value['recording.cpal.sampleRate'],
				},
			} as const;

			const params =
				paramsMap[
					!window.__TAURI_INTERNALS__
						? 'navigator'
						: settings.value['recording.method']
				];

			const { data: deviceAcquisitionOutcome, error: startRecordingError } =
				await session.start(recorderService(), params, {
					sendStatus: (options) =>
						notify.loading.execute({ id: toastId, ...options }),
				});

			if (startRecordingError) {
				return WhisperingErr({
					title: '❌ Failed to start recording',
					serviceError: startRecordingError,
				});
			}
			return Ok(deviceAcquisitionOutcome);
		},
		onSettled: invalidateRecorderState,
	}),

	stopRecording: defineMutation({
		mutationKey: recorderKeys.stopRecording,
		mutationFn: async ({ toastId }: { toastId: string }) => {
			const { data: recording, error: stopRecordingError } =
				await session.stop(recorderService(), {
					sendStatus: (options) =>
						notify.loading.execute({ id: toastId, ...options }),
				});

			if (stopRecordingError) {
				return WhisperingErr({
					title: '❌ Failed to stop recording',
					serviceError: stopRecordingError,
				});
			}

			return Ok(recording);
		},
		onSettled: invalidateRecorderState,
	}),

	cancelRecording: defineMutation({
		mutationKey: recorderKeys.cancelRecording,
		mutationFn: async ({ toastId }: { toastId: string }) => {
			const { data: cancelResult, error: cancelRecordingError } =
				await session.cancel(recorderService(), {
					sendStatus: (options) =>
						notify.loading.execute({ id: toastId, ...options }),
				});

			if (cancelRecordingError) {
				return WhisperingErr({
					title: '❌ Failed to cancel recording',
					serviceError: cancelRecordingError,
				});
			}

			return Ok(cancelResult);
		},
		onSettled: invalidateRecorderState,
	}),
};

/**
 * Get the appropriate recorder service based on settings and environment
 */
export function recorderService() {
	// In browser, always use navigator recorder
	if (!window.__TAURI_INTERNALS__) return services.navigatorRecorder;

	const recorderMap = {
		navigator: services.navigatorRecorder,
		ffmpeg: services.ffmpegRecorder,
		cpal: services.cpalRecorder,
	};
	return recorderMap[settings.value['recording.method']];
}
