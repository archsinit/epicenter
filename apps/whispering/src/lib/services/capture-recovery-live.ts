import { invoke } from '@tauri-apps/api/core';
import { createCaptureRecovery } from './capture-recovery';
import { waitForCaptureStartup } from './capture-startup-live';

// Same session metadata key as the FFmpeg recorder, not user preferences.
// Do not reset its in-memory state or startup barrier in this application.
const SAVED_SESSION_KEY = 'whispering-ffmpeg-recording-session';
export const CaptureRecoveryServiceLive = createCaptureRecovery({
	ready: waitForCaptureStartup,
	readSaved: () => window.localStorage.getItem(SAVED_SESSION_KEY),
	confirmAndArchive: (savedSession) =>
		invoke<string | null>('capture_recover_saved_session', { savedSession }),
	clearSaved: () => window.localStorage.removeItem(SAVED_SESSION_KEY),
});
