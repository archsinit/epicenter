import { invoke } from '@tauri-apps/api/core';
import { waitForCaptureStartup } from '../capture-startup-live';
import { createMeetingRecovery } from './recovery';
export const MeetingRecoveryLive = createMeetingRecovery(invoke, async () => {
	const result = await waitForCaptureStartup();
	if (result.error) throw new Error(result.error.message);
});
