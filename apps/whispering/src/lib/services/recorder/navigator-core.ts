import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, type Result, tryAsync, trySync } from 'wellcrafted/result';
import type {
	Device,
	DeviceAcquisitionOutcome,
	DeviceIdentifier,
	UpdateStatusMessageFn,
} from '../types';
import {
	RecorderServiceErr,
	type RecorderService,
	type RecorderServiceError,
	type NavigatorRecordingParams,
} from './types';

type Dependencies = {
	acquireStream: (params: {
		selectedDeviceId: DeviceIdentifier | null;
		sendStatus: UpdateStatusMessageFn;
	}) => Promise<
		Result<
			{ stream: MediaStream; deviceOutcome: DeviceAcquisitionOutcome },
			{ message: string }
		>
	>;
	enumerateDevices: () => Promise<Result<Device[], { message: string }>>;
	cleanupStream: (stream: MediaStream) => void;
	cleanupPendingStreams?: () => void;
	hasPendingStreams?: () => boolean;
	createRecorder: (stream: MediaStream, bits: number) => MediaRecorder;
	timesliceMs: number;
};
type Active = { stream: MediaStream; recorder: MediaRecorder; chunks: Blob[] };
const failed = (error: unknown) =>
	RecorderServiceErr({ message: extractErrorMessage(error) });

/** Own every acquired stream until track cleanup succeeds, including failed
 * constructors/start/stop. A rejected cleanup never turns into a false IDLE.
 * Dependencies make the actual lifecycle testable without a microphone.
 */
export function createNavigatorRecorderCore(
	deps: Dependencies,
): RecorderService {
	let active: Active | null = null;
	let pendingStream: MediaStream | null = null;
	let busy = false;
	function cleanup() {
		const stream = active?.stream ?? pendingStream;
		const result = trySync({
			try: () => { if (stream) deps.cleanupStream(stream); },
			catch: failed,
		});
		const pending = trySync({
			try: () => deps.cleanupPendingStreams?.(),
			catch: failed,
		});
		if (!result.error && !pending.error) {
			active = null;
			pendingStream = null;
		}
		return result.error ? result : pending;
	}
	async function guarded<T>(
		action: () => Promise<Result<T, RecorderServiceError>>,
	) {
		if (busy)
			return RecorderServiceErr({
				message: 'A recording operation is still pending.',
			});
		busy = true;
		const attempt = await tryAsync({ try: action, catch: failed });
		busy = false;
		return attempt.error ? Err(attempt.error) : attempt.data;
	}
	return {
		getRecorderState: async () =>
			Ok(busy || active || pendingStream || deps.hasPendingStreams?.() ? 'RECORDING' : 'IDLE'),
		enumerateDevices: async () => {
			const result = await deps.enumerateDevices();
			return result.error
				? RecorderServiceErr({ message: result.error.message })
				: Ok(result.data);
		},
		startRecording: (
			{ selectedDeviceId, bitrateKbps }: NavigatorRecordingParams,
			{ sendStatus },
		) =>
			guarded(async () => {
				if (active || pendingStream || deps.hasPendingStreams?.())
					return RecorderServiceErr({
						message:
							'A recording/stream is already active. Stop/cancel it first.',
					});
				const attempt = await tryAsync({
					try: async () => {
						sendStatus({
							title: '🎙️ Starting Recording',
							description: 'Setting up your microphone...',
						});
						const acquired = await deps.acquireStream({
							selectedDeviceId,
							sendStatus,
						});
						if (acquired.error)
							return RecorderServiceErr({ message: acquired.error.message });
						pendingStream = acquired.data.stream;
						const recorder = deps.createRecorder(
							pendingStream,
							Number(bitrateKbps) * 1000,
						);
						const chunks: Blob[] = [];
						active = { stream: pendingStream, recorder, chunks };
						pendingStream = null;
						recorder.addEventListener('dataavailable', (event: BlobEvent) => {
							if (event.data.size) chunks.push(event.data);
						});
						recorder.start(deps.timesliceMs);
						return Ok(acquired.data.deviceOutcome);
					},
					catch: failed,
				});
				const result = attempt.error ? Err(attempt.error) : attempt.data;
				if (result.error) {
					const cleaned = cleanup();
					if (cleaned.error)
						return RecorderServiceErr({
							message: `${result.error.message}; microphone cleanup failed: ${cleaned.error.message}. Retry Cancel before another recording.`,
						});
				}
				return result;
			}),
		stopRecording: ({ sendStatus }) =>
			guarded(async () => {
				if (!active)
					return RecorderServiceErr({
						message: 'No active recording to stop.',
					});
				const recording = active;
				const attempt = await tryAsync({
					try: async () => {
						sendStatus({
							title: '⏸️ Finishing Recording',
							description: 'Saving your audio...',
						});
						if (recording.recorder.state !== 'inactive') {
							await new Promise<void>((resolve, reject) => {
								const timer = setTimeout(() => {
									recording.recorder.removeEventListener('stop', done);
									reject(new Error('Recorder did not confirm Stop.'));
								}, 5000);
								function done() {
									clearTimeout(timer);
									resolve();
								}
								recording.recorder.addEventListener('stop', done, {
									once: true,
								});
								const stopped = trySync({
									try: () => recording.recorder.stop(),
									catch: failed,
								});
								if (stopped.error) {
									clearTimeout(timer);
									recording.recorder.removeEventListener('stop', done);
									reject(stopped.error);
								}
							});
						}
						return new Blob(recording.chunks, {
							type: recording.recorder.mimeType,
						});
					},
					catch: failed,
				});
				const cleaned = cleanup();
				if (cleaned.error) return Err(cleaned.error); // retain stream/chunks for retry
				if (attempt.error) return Err(attempt.error);
				return Ok(attempt.data);
			}),
		cancelRecording: () =>
			guarded(async () => {
				const recording = active;
				const stopped = trySync({
					try: () => {
						if (recording && recording.recorder.state !== 'inactive')
							recording.recorder.stop();
					},
					catch: failed,
				});
				const cleaned = cleanup(); // always stop tracks, even if recorder.stop throws
				if (cleaned.error) return Err(cleaned.error);
				if (stopped.error) return Err(stopped.error);
				return Ok({
					status: recording
						? ('cancelled' as const)
						: ('no-recording' as const),
				});
			}),
	};
}
