import { invoke } from '@tauri-apps/api/core';

type Phase = 'idle' | 'recording' | 'transcribing' | 'cleaning' | 'error';
const jobs = new Map<string, 'transcribing' | 'cleaning'>();
let recording = false;
let metered = false;
let failed = false;
let errorTimer: ReturnType<typeof setTimeout> | undefined;
let updates = Promise.resolve();

function publish() {
	if (!window.__TAURI_INTERNALS__) return;
	// Recording takes precedence over earlier jobs so their completion cannot hide a new take.
	const phase: Phase = recording
		? 'recording'
		: (jobs.values().next().value ?? (failed ? 'error' : 'idle'));
	const hasMeter = metered;
	updates = updates
		.then(() =>
			invoke<void>('set_indicator_state', {
				phase,
				metered: hasMeter,
			}),
		)
		.catch((error) => console.warn('Recording indicator unavailable:', error));
}

export const indicator = {
	recording(active: boolean, hasMeter: boolean) {
		recording = active;
		metered = hasMeter;
		publish();
	},
	begin(id: string) {
		jobs.set(id, 'transcribing');
		publish();
	},
	cleaning(id: string) {
		jobs.set(id, 'cleaning');
		publish();
	},
	fail() {
		failed = true;
		clearTimeout(errorTimer);
		errorTimer = setTimeout(() => {
			failed = false;
			publish();
		}, 5000);
		publish();
	},
	finish(id: string) {
		jobs.delete(id);
		publish();
	},
};
