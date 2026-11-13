import { invoke } from '@tauri-apps/api/core';
import { createCaptureLease } from './capture-lease';

export function createNativeCaptureLease() {
	return createCaptureLease({
		request: invoke,
		enabled:
			typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__),
	});
}
