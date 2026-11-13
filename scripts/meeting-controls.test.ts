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
			'meeting_list_saved',
			'meeting_saved_detail',
			'meeting_read_mixed_audio',
			'meeting_read_screenshot',
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
	test('live Mac recorder and VAD entry points bind lifetime ownership without eager client cycles', () => {
		const services = readFileSync(new URL('../index.ts', clientRoot), 'utf8');
		for (const client of ['Cpal', 'Navigator', 'Ffmpeg'])
			expect(services).toContain(`macRecorder(() => ${client}RecorderServiceLive`);
		expect(services).toContain('createLazyRecorderService(() =>');
		expect(services).toContain('createLeasedRecorderService(base, createNativeCaptureLease(), cleanupEnumeration)');
		expect(services).toContain("OsServiceLive.type() === 'macos'");
		const vad = readFileSync(new URL('../../query/vad.svelte.ts', clientRoot), 'utf8');
		expect(vad).toContain('lease: createNativeCaptureLease()');
		expect(vad).toContain('createVadRecorderCore({');
		expect(vad).toContain('await core.enumerateDevices()');
		expect(vad).toContain('await core.start(');
		expect(vad).toContain('await core.stop()');
		expect(vad).toContain("model: 'v5'");
		const navigator = readFileSync(new URL('navigator.ts', clientRoot), 'utf8');
		expect(navigator).toContain('enumerateForFallback: devices.enumerateDevices');
		expect(navigator).toContain('cleanupPendingStreams: devices.cleanup');
		const dictation = readFileSync(new URL('../../query/recorder.ts', clientRoot), 'utf8');
		expect(dictation).toContain('const session = createRecorderSession()');
		expect(dictation).toContain('await session.start(recorderService(), params');
		expect(dictation).toContain('await session.stop(recorderService(),');
		expect(dictation).toContain('await session.cancel(recorderService(),');
		expect(dictation).not.toContain('currentRecordingId = null');
	});
	test('library IPC is native-rooted, read-only, and separates screenshot bytes from mixed audio', () => {
		const library = readFileSync(new URL('meeting/library.rs', root), 'utf8');
		for (const unsafe of ['MacMeetingBackend', 'remove_file(', 'fs::write(', 'request_screen_permission', 'reqwest', 'transcribe('])
			expect(library).not.toContain(unsafe);
		expect(command('meeting_read_mixed_audio')).toContain('read_mixed_audio(');
		expect(command('meeting_read_mixed_audio')).toContain('active_meeting_id()');
		expect(command('meeting_read_screenshot')).toContain('read_screenshot(');
		expect(command('meeting_read_screenshot')).not.toContain('read_mixed_audio(');
		expect(commands).toContain('tauri::ipc::Response::new');
		const client = readFileSync(new URL('../meetings/library.ts', clientRoot), 'utf8');
		for (const unsafe of ['fetch(', '/transcription', "from './clipboard", '/delivery'])
			expect(client).not.toContain(unsafe);
		const page = readFileSync(new URL('../apps/whispering/src/routes/(app)/(config)/meetings/MeetingLibrary.svelte', import.meta.url), 'utf8');
		expect(page).toContain('return () => resource.close()');
		expect(page).toContain('data.id !== selectedId');
		expect(page).not.toContain('autoplay');
		expect(page).not.toContain('{@html');
		expect(page).not.toContain('meeting_start');
		expect(page).not.toContain('transcribeRecordings');
	});
	test('meeting UI uses explicit mutations and native progress, never page-mount capture or frontend screenshot timers', () => {
		const controls = readFileSync(new URL('../meetings/controls.ts', clientRoot), 'utf8');
		const query = readFileSync(new URL('../../query/meeting-controls.ts', clientRoot), 'utf8');
		const page = readFileSync(new URL('../apps/whispering/src/routes/(app)/(config)/meetings/MeetingRecorder.svelte', import.meta.url), 'utf8');
		expect(page).toContain('formatMeetingOffset(progress.elapsedMs)');
		expect(page).toContain('onclick={() => sources.mutate()}');
		expect(page).toContain("permission.mutate('microphone'");
		expect(page).toContain("permission.mutate('screen'");
		expect(page).toContain('start.mutate(params,');
		expect(page).toContain('stop.mutate()');
		expect(page).toContain('capture.mutate()');
		expect(page).toContain('pause.mutate(!progress.screenshotsPaused)');
		expect(page).toContain('onSuccess: () => { confirmed = false; }');
		for (const unsafe of ['setInterval(', 'setTimeout(', 'onMount(', 'settings.update', 'transcribeRecordings', 'getUserMedia('])
			expect(page).not.toContain(unsafe);
		for (const unsafe of ['fetch(', '/transcription', '/clipboard', 'setInterval(', 'setTimeout('])
			expect(controls).not.toContain(unsafe);
		const status = query.slice(query.indexOf('status: defineQuery'), query.indexOf('loadSources: defineMutation'));
		expect(status).toContain('await service.getStatus()');
		expect(status).not.toContain('service.start(');
		expect(status).not.toContain('service.requestPermission(');
		const controller = readFileSync(new URL('meeting/controller.rs', root), 'utf8');
		expect(controller).toContain('if !progress.cleanup_confirmed');
		expect(controller).toContain('progress.capture_reserved = false');
		expect(command('meeting_stop')).toContain('controller.stop_blocking()');
		expect(command('meeting_start')).toContain('menu_bar::meeting_transition');
		expect(command('meeting_status')).toContain('sync_meeting_mark(');
	});
	test('audio recovery is a local copy engine, unexposed until maintenance admission and confirmation exist', () => {
        const engine = readFileSync(new URL('meeting/audio_recovery.rs', root), 'utf8');
        for (const unsafe of ['remove_file(', 'remove_dir(', 'MacMeetingBackend', 'request_permission(', 'reqwest', 'open_screenshot_for_local_preview(', 'open_audio_for_transcription('])
            expect(engine).not.toContain(unsafe);
        expect(engine).toContain('mix_tracks(&mut store');
        expect(engine).toContain('store.finish(self.preview.duration_ms, true)');
        expect(engine).toContain('"captureCompletenessVerified": false');
        expect(engine).toContain('"screenshotsRemainInSource": true');
        expect(engine).toContain('MAX_RECOVERY_DIRECTORY_ENTRIES');
        expect(engine).toContain('MAX_SOURCE_BYTES');
        expect(engine).not.toContain('impl Deserialize for AudioRecoveryPlan');
        expect(commands).not.toContain('audio_recovery::');
        expect(app).not.toContain('meeting_commands::meeting_recover_audio');
        const library = readFileSync(new URL('meeting/library.rs', root), 'utf8');
        expect(library).not.toContain('audio_recovery::');
    });
	test('bounded Quit observes one cleanup job without late auto-exit or admission release', () => {
        const monitor = commands.slice(commands.indexOf('pub fn handle_exit('), commands.indexOf('fn finalize_native_capture('));
        const cleanup = commands.slice(commands.indexOf('fn finalize_native_capture('));
        expect(monitor).toContain('run.observe(Duration::from_secs(20))');
        expect(monitor).toContain('Observation::Pending');
        expect(monitor).toContain('if quit || completed');
        expect(monitor).toContain('capture admission stays closed until full Quit/relaunch');
        expect(monitor).toContain('Quit again to retry this confirmation');
        expect(monitor).toContain('ExitMonitorGuard(');
        expect(monitor.indexOf('if state.exit_task.load')).toBeLessThan(monitor.indexOf('controller.request_exit()'));
        expect(cleanup).toContain('sync_meeting_mark(handle, &state.controller)');
        for (const unsafe of ['.exit(', 'resolve_exit(', '.dialog()', 'exit_approved.store', 'cancel_shutdown('])
            expect(cleanup).not.toContain(unsafe);
        expect(cleanup).toContain('controller.stop_blocking()');
        expect(cleanup).toContain('stop_owned_ffmpeg(');
        expect(monitor).not.toContain('controller.stop_blocking()');
        const job = readFileSync(new URL('meeting/quit.rs', root), 'utf8');
        for (const unsafe of ['.exit(', 'resolve_exit(', '.kill(', 'remove_file(', 'begin_meeting(', 'end_meeting('])
            expect(job).not.toContain(unsafe);
        expect(job).toContain('if !run.is_complete()?');
        expect(job).toContain('catch_unwind');
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
