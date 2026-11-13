import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';

const root = new URL('../apps/whispering/src-tauri/src/', import.meta.url);
const app = readFileSync(new URL('lib.rs', root), 'utf8');
const commands = readFileSync(new URL('meeting/commands.rs', root), 'utf8');
const cpal = readFileSync(new URL('recorder/commands.rs', root), 'utf8');
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
