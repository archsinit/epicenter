/**
 * Recording state constants and schemas
 */
import { type } from 'arktype';

export const WhisperingRecordingState = type("'IDLE' | 'RECORDING'");

export type WhisperingRecordingState = typeof WhisperingRecordingState.infer;

export type CancelRecordingResult =
	| { status: 'cancelled' }
	| { status: 'no-recording' };

export const RECORDER_STATE_TO_ICON = {
	IDLE: '🎙️',
	RECORDING: '⏹️',
} as const satisfies Record<WhisperingRecordingState, string>;

export const VadState = type("'IDLE' | 'STARTING' | 'LISTENING' | 'SPEECH_DETECTED' | 'STOPPING' | 'CLEANUP_REQUIRED'");

export type VadState = typeof VadState.infer;

export const VAD_STATE_TO_ICON = {
	IDLE: '🎤',
	STARTING: '⏳',
	STOPPING: '⏹️',
	CLEANUP_REQUIRED: '⚠️',
	LISTENING: '💬',
	SPEECH_DETECTED: '👂',
} as const satisfies Record<VadState, string>;
