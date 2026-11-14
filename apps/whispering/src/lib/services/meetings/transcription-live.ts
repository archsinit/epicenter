import { invoke } from '@tauri-apps/api/core';
import { createMeetingTranscription } from './transcription';
export const MeetingTranscriptionLive = createMeetingTranscription(invoke);
