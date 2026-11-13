import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';

const root = new URL('../apps/whispering/src-tauri/src/', import.meta.url);
const app = readFileSync(new URL('lib.rs', root), 'utf8');
const commands = readFileSync(new URL('meeting/commands.rs', root), 'utf8');
const cpal = readFileSync(new URL('recorder/commands.rs', root), 'utf8');
const clientRoot = new URL(
	'../apps/whispering/src/lib/services/recorder/',
	import.meta.url,
);
const ffmpegClient = readFileSync(new URL('ffmpeg.ts', clientRoot), 'utf8');
const cpalClient = readFileSync(new URL('cpal.ts', clientRoot), 'utf8');
function command(name: string) {
	const start = commands.indexOf(`pub async fn ${name}(`);
	expect(start).toBeGreaterThan(-1);
	const end = commands.indexOf('#[tauri::command]', start + 1);
	return commands.slice(
		start,
		end < 0 ? commands.indexOf('/// Intercept every normal', start) : end,
	);
}
describe('native meeting application wiring (source contracts, not GUI/capture acceptance)', () => {
	test('all controls require the real main webview and are registered', () => {
		for (const name of [
			'capture_acquire_dictation',
			'capture_release_dictation',
			'capture_ffmpeg_spawn',
			'capture_ffmpeg_stop',
			'capture_ffmpeg_restore',
			'capture_restore_ready',
			'capture_recover_saved_session',
			'meeting_sources',
			'meeting_request_permission',
			'meeting_start',
			'meeting_status',
			'meeting_stop',
			'meeting_capture_now',
			'meeting_pause_screenshots',
		]) {
			expect(command(name)).toContain('main_window(&window)?');
			expect(app).toContain(`meeting_commands::${name},`);
		}
		expect(commands).toContain('window.label() != "main"');
	});
	test('startup fence is acknowledged only after saved-session inspection', () => {
		expect(commands).toContain('MeetingController::awaiting_restore()');
		expect(command('capture_restore_ready')).toContain('finish_startup_restore()');
		expect(ffmpegClient).toContain('createCaptureStartup({');
		expect(ffmpegClient).toContain("await invoke<void>('capture_restore_ready')");
		expect(ffmpegClient).toContain('invalidSavedSession');
		const streams = readFileSync(new URL('../device-stream.ts', clientRoot), 'utf8');
		expect(streams.match(/await waitForCaptureStartup\(\)/g)?.length).toBe(2);
		expect(cpalClient.indexOf('await waitForCaptureStartup()')).toBeLessThan(
			cpalClient.indexOf('const { data: devices, error: enumerateError }'),
		);
	});
	test('saved metadata recovery requires native confirmation, archives before acknowledgement, and never clears quarantine', () => {
		const recover = command('capture_recover_saved_session');
		expect(recover.match(/ensure_metadata_recovery_allowed\(\)/g)?.length).toBe(2);
		expect(recover.indexOf('validate_saved_metadata')).toBeLessThan(recover.indexOf('.blocking_show()'));
		expect(recover.indexOf('.blocking_show()')).toBeLessThan(recover.indexOf('archive_confirmed_metadata'));
		expect(recover.indexOf('if !confirmed')).toBeLessThan(recover.indexOf('archive_confirmed_metadata'));
		for (const unsafe of ['signal_owned_process(', 'stop_ffmpeg(', 'finish_startup_restore(', 'remove_file(', 'write_exit_receipt('])
			expect(recover).not.toContain(unsafe);
		const page = readFileSync(new URL('../apps/whispering/src/routes/(app)/(config)/settings/recording/+page.svelte', import.meta.url), 'utf8');
		expect(page).toContain('{#if IS_MACOS && window.__TAURI_INTERNALS__}');
		expect(page).toContain('<SavedSessionRecovery />');
	});
	test('Quit interception covers ExitRequested rather than only tray actions', () => {
		expect(app).toContain('RunEvent::ExitRequested { api, .. }');
		expect(app).toContain('meeting_commands::handle_exit(handler, api)');
		expect(commands).toContain('controller.stop_blocking()');
		expect(commands).toContain('Quit anyway');
	});
	test('startup manages idle application state, with no native capture factory call', () => {
		expect(app).toContain(
			'.manage(meeting_commands::MeetingAppState::default())',
		);
		expect(app).not.toContain('MacMeetingBackend::new');
		expect(command('meeting_start')).toContain('params.into_manifest()?');
		expect(command('meeting_start')).not.toContain(
			'request_microphone_permission',
		);
		expect(command('meeting_start')).not.toContain('request_screen_permission');
	});
	test('Mac FFmpeg stop requires acknowledged exit before clearing state or reading audio', () => {
		const start = ffmpegClient.indexOf('stopRecording: async');
		const branch = ffmpegClient.indexOf(
			"if (DESKTOP_PLATFORM === 'macos')",
			start,
		);
		const source = ffmpegClient.slice(
			branch,
			ffmpegClient.indexOf('// Existing non-Mac stop path', branch),
		);
		const beforeStop = ffmpegClient.slice(start, branch);
		expect(beforeStop.indexOf('await initialized')).toBeGreaterThan(-1);
		expect(beforeStop.indexOf('await initialized')).toBeLessThan(
			beforeStop.indexOf('const session = sessionState.value'),
		);
		expect(source).toContain('await ownedProcess.stop(session.captureId)');
		expect(source.indexOf('if (stopped.error)')).toBeLessThan(
			source.indexOf('sessionState.value = null'),
		);
		expect(source.indexOf('await ownedProcess.stop')).toBeLessThan(
			source.indexOf('pathToBlob'),
		);
		for (const guess of ['child.kill', 'setTimeout', 'stat('])
			expect(source).not.toContain(guess);
	});
	test('CPAL closes initialized sessions even with no recording ID and before delivering a Blob', () => {
		const start = cpalClient.indexOf('stopRecording: async');
		const stop = cpalClient.slice(
			start,
			cpalClient.indexOf('cancelRecording: async', start),
		);
		expect(stop.indexOf('await closeSession()')).toBeGreaterThan(-1);
		expect(stop.indexOf('await closeSession()')).toBeLessThan(
			stop.indexOf('FsServiceLive.pathToBlob'),
		);
		const noId = cpalClient.slice(
			cpalClient.indexOf('if (!recordingId)'),
			cpalClient.indexOf("title: '🛑 Cancelling'"),
		);
		expect(noId).toContain('await closeSession()');
		expect(noId).toContain('if (closed.error)');
	});
	test('direct CPAL init/start are guarded; initialized streams remain tracked until close', () => {
		for (const name of ['init_recording_session', 'start_recording']) {
			const start = cpal.indexOf(`pub async fn ${name}(`);
			const end = cpal.indexOf('#[tauri::command]', start + 1);
			const source = cpal.slice(start, end);
			expect(source.indexOf('capture_gate()')).toBeLessThan(
				source.indexOf('.recorder'),
			);
			expect(source).toContain('ensure_cpal_allowed()');
		}
		expect(
			cpal.match(/capture\.set_cpal_open\(recorder\.has_open_session\(\)\)/g)
				?.length,
		).toBe(3);
	});
});
