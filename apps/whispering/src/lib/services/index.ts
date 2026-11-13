import { CommandServiceLive } from './command';
import { createNativeCaptureLease } from './capture-lease-live';
import { createLazyRecorderService, createLeasedRecorderService } from './recorder/leased';
import type { RecorderService } from './recorder/types';
import * as completions from './completion';
import { DbServiceLive } from './db';
import { DownloadServiceLive } from './download';
import { FfmpegServiceLive } from './ffmpeg';
import { FsServiceLive } from './fs';
import { GlobalShortcutManagerLive } from './global-shortcut-manager';
import { LocalShortcutManagerLive } from './local-shortcut-manager';
import { NotificationServiceLive } from './notifications';
import { OsServiceLive } from './os';
import { PermissionsServiceLive } from './permissions';
import { CpalRecorderServiceLive } from './recorder/cpal';
import { FfmpegRecorderServiceLive } from './recorder/ffmpeg';
import { NavigatorRecorderServiceLive } from './recorder/navigator';
import { PlaySoundServiceLive } from './sound';
import { TextServiceLive } from './text';
import { ToastServiceLive } from './toast';
import * as transcriptions from './transcription';
import { TrayIconServiceLive } from './tray';

// Keep browser/non-Mac dictation behavior unchanged. All Mac live entry points
// acquire before devices open; raw FFmpeg is used only by startup restoration.
function macRecorder(resolve: () => RecorderService, cleanupEnumeration?: () => void) {
	return createLazyRecorderService(() => {
		const base = resolve();
		return typeof window !== 'undefined' && window.__TAURI_INTERNALS__ && OsServiceLive.type() === 'macos'
			? createLeasedRecorderService(base, createNativeCaptureLease(), cleanupEnumeration)
			: base;
	});
}
export const cpalRecorder = macRecorder(() => CpalRecorderServiceLive);
export const navigatorRecorder = macRecorder(() => NavigatorRecorderServiceLive, () => NavigatorRecorderServiceLive.cleanupDeviceEnumeration());
export const ffmpegRecorder = macRecorder(() => FfmpegRecorderServiceLive);

/**
 * Unified services object providing consistent access to all services.
 */
export {
	TextServiceLive as text,
	CommandServiceLive as command,
	completions,
	TrayIconServiceLive as tray,
	DbServiceLive as db,
	DownloadServiceLive as download,
	FfmpegServiceLive as ffmpeg,
	FsServiceLive as fs,
	GlobalShortcutManagerLive as globalShortcutManager,
	LocalShortcutManagerLive as localShortcutManager,
	NotificationServiceLive as notification,
	PermissionsServiceLive as permissions,
	ToastServiceLive as toast,
	OsServiceLive as os,
	PlaySoundServiceLive as sound,
	transcriptions,
};
