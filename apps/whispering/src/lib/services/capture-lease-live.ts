import { invoke } from '@tauri-apps/api/core';
import { createCaptureLease } from './capture-lease';
import { OsServiceLive } from './os';
import { waitForCaptureStartup } from './capture-startup-live';

export function createNativeCaptureLease() {
	return createCaptureLease({
		request: async <T>(command: string, args?: Record<string, unknown>) => {
			if (command === 'capture_acquire_dictation') {
				const ready = await waitForCaptureStartup();
				if (ready.error) throw new Error(ready.error.message);
			}
			return invoke<T>(command, args);
		},
		enabled:
			typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__) && OsServiceLive.type() === 'macos',
	});
}
