import { invoke } from '@tauri-apps/api/core';
import { createTaggedError, extractErrorMessage } from 'wellcrafted/error';
import { type Err, Ok, tryAsync } from 'wellcrafted/result';
import type { WhisperingRecordingState } from '$lib/constants/audio';

const { SetTrayIconServiceError, SetTrayIconServiceErr } = createTaggedError(
	'SetTrayIconServiceError',
);
type SetTrayIconServiceError = ReturnType<typeof SetTrayIconServiceError>;

type SetTrayIconService = {
	setTrayIcon: (
		icon: WhisperingRecordingState,
	) => Promise<Ok<void> | Err<SetTrayIconServiceError>>;
};

export function createTrayIconWebService(): SetTrayIconService {
	return {
		setTrayIcon: async () => Ok(undefined),
	};
}

export function createTrayIconDesktopService(): SetTrayIconService {
	// The native startup path owns the sole tray icon and its menu. Webviews
	// only update recording status, so hidden/auxiliary windows cannot replace it.
	return {
		setTrayIcon: (recorderState) =>
			tryAsync({
				try: () =>
					invoke<void>('set_tray_recording', {
						recording: recorderState === 'RECORDING',
					}),
				catch: (error) =>
					SetTrayIconServiceErr({
						message: `Failed to set tray icon: ${extractErrorMessage(error)}`,
					}),
			}),
	};
}

export const TrayIconServiceLive = window.__TAURI_INTERNALS__
	? createTrayIconDesktopService()
	: createTrayIconWebService();
