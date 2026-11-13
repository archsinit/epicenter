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
	const update = native.slice(native.indexOf('pub fn set_tray_recording'));
	expect(update).toContain('tray.set_icon_as_template(true)');
	expect(update.indexOf('tray.set_icon(Some(')).toBeLessThan(update.indexOf('tray.set_icon_as_template(true)'));
	expect(update.indexOf('tray.set_icon_as_template(true)')).toBeLessThan(update.indexOf('tray.set_tooltip'));
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
