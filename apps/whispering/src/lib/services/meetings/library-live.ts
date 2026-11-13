import { invoke } from '@tauri-apps/api/core';
import { createMeetingLibraryService } from './library';

export const MeetingLibraryServiceLive = createMeetingLibraryService(invoke);
