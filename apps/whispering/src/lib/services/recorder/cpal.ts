import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { remove } from '@tauri-apps/plugin-fs';
import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync } from 'wellcrafted/result';
import type {
	CancelRecordingResult,
	WhisperingRecordingState,
} from '$lib/constants/audio';
import { FsServiceLive } from '../fs';
import type { Device, DeviceAcquisitionOutcome } from '../types';
import { asDeviceIdentifier } from '../types';
import type {
	CpalRecordingParams,
	RecorderService,
	RecorderServiceError,
} from './types';
import { RecorderServiceErr } from './types';

/**
 * Audio recording data returned from the Rust method
 */
type AudioRecording = {
	sampleRate: number;
	channels: number;
	durationSeconds: number;
	filePath?: string;
};

/**
 * Creates a CPAL recorder service that interfaces with Rust audio recording methods.
 * This service handles device enumeration, recording start/stop operations, and file management
 * for desktop audio recording using the CPAL library.
 *
 * @returns {RecorderService} A recorder service instance with methods for audio recording operations
 */
export function createCpalRecorderService(): RecorderService {
	async function closeSession() {
		const result = await invoke<void>('close_recording_session');
		if (result.error)
			return RecorderServiceErr({
				message:
					'The audio session could not confirm closure. Audio files were retained; retry Cancel before another capture.',
			});
		return Ok(undefined);
	}
	/**
	 * Enumerates available recording devices from the system.
	 *
	 * @returns {Promise<Result<Device[], RecorderServiceError>>} A promise that resolves to either a list of available devices or an error
	 */
	const enumerateDevices = async (): Promise<
		Result<Device[], RecorderServiceError>
	> => {
		const { data: deviceNames, error: enumerateRecordingDevicesError } =
			await invoke<string[]>('enumerate_recording_devices');
		if (enumerateRecordingDevicesError) {
			return RecorderServiceErr({
				message: 'Failed to enumerate recording devices',
			});
		}
		// On desktop, device names serve as both ID and label
		return Ok(
			deviceNames.map((name) => ({
				id: asDeviceIdentifier(name),
				label: name,
			})),
		);
	};

	return {
		/**
		 * Gets the current state of the recorder.
		 *
		 * @returns {Promise<Result<WhisperingRecordingState, RecorderServiceError>>} A promise that resolves to either 'RECORDING' or 'IDLE' state, or an error
		 */
		getRecorderState: async (): Promise<
			Result<WhisperingRecordingState, RecorderServiceError>
		> => {
			const { data: recordingId, error: getRecorderStateError } = await invoke<
				string | null
			>('get_current_recording_id');
			if (getRecorderStateError)
				return RecorderServiceErr({
					message:
						'We encountered an issue while getting the recorder state. This could be because your microphone is being used by another app, your microphone permissions are denied, or the selected recording device is disconnected',
				});

			return Ok(recordingId ? 'RECORDING' : 'IDLE');
		},

		enumerateDevices,

		/**
		 * Starts a recording session with the specified parameters.
		 * Handles device selection, fallback logic, and recording initialization.
		 *
		 * @param {CpalRecordingParams} params - Recording parameters including device ID, recording ID, output folder, and sample rate
		 * @param {Object} callbacks - Callback functions for status updates
		 * @param {Function} callbacks.sendStatus - Function to send status updates during the recording process
		 * @returns {Promise<Result<DeviceAcquisitionOutcome, RecorderServiceError>>} A promise that resolves to device acquisition outcome or an error
		 */
		startRecording: async (
			{
				selectedDeviceId,
				recordingId,
				outputFolder,
				sampleRate,
			}: CpalRecordingParams,
			{ sendStatus },
		): Promise<Result<DeviceAcquisitionOutcome, RecorderServiceError>> => {
			const { data: devices, error: enumerateError } = await enumerateDevices();
			if (enumerateError) return Err(enumerateError);

			/**
			 * Acquires a recording device, either the selected one or a fallback.
			 *
			 * @returns {Result<DeviceAcquisitionOutcome, RecorderServiceError>} The device acquisition result
			 */
			const acquireDevice = (): Result<
				DeviceAcquisitionOutcome,
				RecorderServiceError
			> => {
				const deviceIds = devices.map((d) => d.id);
				const fallbackDeviceId = deviceIds.at(0);
				if (!fallbackDeviceId) {
					return RecorderServiceErr({
						message: selectedDeviceId
							? "We couldn't find the selected microphone. Make sure it's connected and try again!"
							: "We couldn't find any microphones. Make sure they're connected and try again!",
					});
				}

				if (!selectedDeviceId) {
					sendStatus({
						title: '🔍 No Device Selected',
						description:
							"No worries! We'll find the best microphone for you automatically...",
					});
					return Ok({
						outcome: 'fallback',
						reason: 'no-device-selected',
						deviceId: fallbackDeviceId,
					});
				}

				// Check if the selected device exists in the devices array
				const deviceExists = deviceIds.includes(selectedDeviceId);

				if (deviceExists)
					return Ok({ outcome: 'success', deviceId: selectedDeviceId });

				sendStatus({
					title: '⚠️ Finding a New Microphone',
					description:
						"That microphone isn't available. Let's try finding another one...",
				});

				return Ok({
					outcome: 'fallback',
					reason: 'preferred-device-unavailable',
					deviceId: fallbackDeviceId,
				});
			};

			const { data: deviceOutcome, error: acquireDeviceError } =
				acquireDevice();
			if (acquireDeviceError) return Err(acquireDeviceError);

			// Use the device from the outcome
			const deviceIdentifier = deviceOutcome.deviceId;

			// Now initialize recording with the chosen device
			sendStatus({
				title: '🎤 Setting Up',
				description:
					'Initializing your recording session and checking microphone access...',
			});

			// Convert sample rate string to number if provided
			const sampleRateNum = sampleRate
				? Number.parseInt(sampleRate, 10)
				: undefined;

			const { error: initRecordingSessionError } = await invoke(
				'init_recording_session',
				{
					deviceIdentifier,
					recordingId,
					outputFolder,
					sampleRate: sampleRateNum,
				},
			);
			if (initRecordingSessionError)
				return RecorderServiceErr({
					message:
						'We encountered an issue while setting up your recording session. This could be because your microphone is being used by another app, your microphone permissions are denied, or the selected recording device is disconnected',
				});

			sendStatus({
				title: '🎙️ Starting Recording',
				description:
					'Recording session initialized, now starting to capture audio...',
			});
			const { error: startRecordingError } =
				await invoke<void>('start_recording');
			if (startRecordingError) {
				const closed = await closeSession();
				return RecorderServiceErr({
					message:
						closed.error?.message ??
						'Unable to start recording. The session was closed and any audio file was retained.',
				});
			}

			return Ok(deviceOutcome);
		},

		/**
		 * Stops the current recording session and returns the recorded audio as a Blob.
		 * Handles file reading, session cleanup, and resource management.
		 *
		 * @param {Object} callbacks - Callback functions for status updates
		 * @param {Function} callbacks.sendStatus - Function to send status updates during the stop process
		 * @returns {Promise<Result<Blob, RecorderServiceError>>} A promise that resolves to the recorded audio blob or an error
		 */
		stopRecording: async ({
			sendStatus,
		}): Promise<Result<Blob, RecorderServiceError>> => {
			const { data: audioRecording, error: stopRecordingError } =
				await invoke<AudioRecording>('stop_recording');
			if (stopRecordingError) {
				return RecorderServiceErr({
					message: 'Unable to save your recording. Please try again.',
				});
			}

			// Stop alone leaves the CPAL stream initialized. Close before reading
			// audio or reporting success so the meeting handoff has real evidence.
			const closed = await closeSession();
			if (closed.error) return Err(closed.error);
			const { filePath } = audioRecording;
			// Desktop recorder should always write to a file
			if (!filePath) {
				return RecorderServiceErr({
					message: 'Recording file path not provided by method.',
				});
			}
			// audioRecording is now AudioRecordingWithFile

			// Read the WAV file from disk
			sendStatus({
				title: '📁 Reading Recording',
				description: 'Loading your recording from disk...',
			});

			const { data: blob, error: readRecordingFileError } =
				await FsServiceLive.pathToBlob(filePath);
			if (readRecordingFileError)
				return RecorderServiceErr({
					message: `Unable to read recording file: ${readRecordingFileError.message}`,
				});

			return Ok(blob);
		},

		/**
		 * Cancels the current recording session and cleans up resources.
		 * Deletes any temporary recording files and closes the recording session.
		 *
		 * @param {Object} callbacks - Callback functions for status updates
		 * @param {Function} callbacks.sendStatus - Function to send status updates during the cancel process
		 * @returns {Promise<Result<CancelRecordingResult, RecorderServiceError>>} A promise that resolves to the cancellation result or an error
		 */
		cancelRecording: async ({
			sendStatus,
		}): Promise<Result<CancelRecordingResult, RecorderServiceError>> => {
			// Check current state first
			const { data: recordingId, error: getRecordingIdError } = await invoke<
				string | null
			>('get_current_recording_id');
			if (getRecordingIdError) {
				return RecorderServiceErr({
					message:
						'Unable to check recording state. Please try closing the app and starting again.',
				});
			}

			if (!recordingId) {
				// A failed Start or prior Stop can still own an initialized stream.
				const closed = await closeSession();
				if (closed.error) return Err(closed.error);
				return Ok({ status: 'no-recording' });
			}

			sendStatus({
				title: '🛑 Cancelling',
				description:
					'Safely stopping your recording and cleaning up resources...',
			});

			// First get the recording data to know if there's a file to delete
			const { data: audioRecording } =
				await invoke<AudioRecording>('stop_recording');

			const closed = await closeSession();
			if (closed.error) return Err(closed.error);

			// If there's a file path, delete the file using Tauri FS plugin
			if (audioRecording?.filePath) {
				const { filePath } = audioRecording;
				const { error: removeError } = await tryAsync({
					try: () => remove(filePath),
					catch: (error) =>
						RecorderServiceErr({
							message: `Failed to delete recording file: ${extractErrorMessage(error)}`,
						}),
				});
				if (removeError)
					sendStatus({
						title: '❌ Error Deleting Recording File',
						description:
							"We couldn't delete the recording file. Continuing with the cancellation process...",
					});
			}

			return Ok({ status: 'cancelled' });
		},
	};
}

/**
 * CPAL recorder service that uses the Rust CPAL method.
 * This is the CPAL audio recorder for desktop environments.
 */
export const CpalRecorderServiceLive = createCpalRecorderService();

/**
 * Wrapper function for Tauri invoke calls that handles errors consistently.
 * Converts Tauri invoke calls into Result types for better error handling.
 *
 * @template T - The expected return type from the Tauri command
 * @param {string} command - The Tauri command to invoke
 * @param {Record<string, unknown>} [args] - Optional arguments to pass to the command
 * @returns {Promise<Result<T, {name: 'TauriInvokeError', command: string, error: unknown}>>} A promise that resolves to either the command result or a structured error
 */
async function invoke<T>(command: string, args?: Record<string, unknown>) {
	return tryAsync({
		try: async () => await tauriInvoke<T>(command, args),
		catch: (error) =>
			Err({ name: 'TauriInvokeError', command, error } as const),
	});
}
