import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const app = join(import.meta.dir, '../apps/whispering');
const native = readFileSync(join(app, 'src-tauri/src/menu_bar.rs'), 'utf8');

test('native startup owns the tray before hiding the Dock icon', () => {
	const startup = readFileSync(join(app, 'src-tauri/src/lib.rs'), 'utf8');
	expect(startup).toContain('menu_bar::create(app)?;');
	expect(startup).toContain('menu_bar::set_tray_recording,');
	expect(native.indexOf('.build(&*app)?;')).toBeLessThan(
		native.indexOf('app.set_activation_policy(tauri::ActivationPolicy::Accessory)'),
	);
	expect(native).toContain('.icon_as_template(true)');
});

test('recording updates restore the template flag on each replacement image', () => {
	const update = native.slice(native.indexOf('fn refresh_capture_mark'), native.indexOf('pub(crate) fn meeting_transition'));
	expect(update).toContain('tray.set_icon_as_template(true)');
	expect(update.indexOf('tray.set_icon(Some(')).toBeLessThan(update.indexOf('tray.set_icon_as_template(true)'));
	expect(update.indexOf('tray.set_icon_as_template(true)')).toBeLessThan(update.indexOf('tray.set_tooltip'));
});

test('native meeting state cannot be cleared by a frontend dictation-idle update', () => {
	const dictation = native.slice(native.indexOf('pub fn set_tray_recording'), native.indexOf('#[cfg(test)]'));
	expect(dictation).toContain('marks.dictation.store(recording');
	expect(dictation).not.toContain('marks.meeting.store');
	const refresh = native.slice(native.indexOf('fn refresh_capture_mark'), native.indexOf('pub(crate) fn meeting_transition'));
	expect(refresh).toContain('run_on_main_thread');
	expect(refresh).toContain('recording_mark(marks.dictation.load');
	const synchronize = native.slice(native.indexOf('pub(crate) fn sync_meeting_recording'), native.indexOf('#[tauri::command]', native.indexOf('pub(crate) fn sync_meeting_recording')));
	expect(synchronize.indexOf('meeting_transition.try_lock()')).toBeLessThan(synchronize.indexOf('reserved()'));
	expect(synchronize).toContain('TryLockError::WouldBlock');
});
test('menu provides recovery and explicit quit without targeting auxiliary windows', () => {
	for (const action of ['tray-show', 'tray-settings', 'tray-quit']) {
		expect(native).toContain(`"${action}" =>`);
	}
	expect(native).toContain('app.get_webview_window("main")');
	expect(native).toContain('window.unminimize()?;');
	expect(native).toContain('window.set_focus()?;');
	expect(native).toContain('app.exit(0);');
	expect(native).toContain('"navigate-main-window"');
});

test('frontend only updates native recording state, with no resource paths or duplicate tray', () => {
	const service = readFileSync(join(app, 'src/lib/services/tray.ts'), 'utf8');
	expect(service).toContain("invoke<void>('set_tray_recording'");
	expect(service).not.toMatch(/TrayIcon\.new|resolveResource|getCurrentWindow|recorder-state-icons/);
});
