import { invoke } from '@tauri-apps/api/core';
import { waitForCaptureStartup } from '../capture-startup-live';
import { createMeetingControls } from './controls';

export const MeetingControlsServiceLive = createMeetingControls({
	request: invoke,
	restore: waitForCaptureStartup,
});
