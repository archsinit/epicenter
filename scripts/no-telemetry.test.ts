import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dir, '..');
const app = join(root, 'apps/whispering');

function sourceFiles(directory: string): string[] {
	return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
		const path = join(directory, entry.name);
		return entry.isDirectory() ? sourceFiles(path) : [path];
	});
}

// Source guards, not a runtime network audit. Prevent restoring the removed
// collection paths, including native startup/exit events that bypassed opt-in.
describe('telemetry-free Intel fork', () => {
	test('frontend and native source contain no telemetry collection paths', () => {
		const files = [
			...sourceFiles(join(app, 'src')),
			...sourceFiles(join(app, 'src-tauri/src')),
		];
		const matches = files.filter((path) =>
			/aptabase|posthog|sentry|analytics|track_event|trackEvent|flush_events/i.test(
				readFileSync(path, 'utf8'),
			),
		);
		expect(matches).toEqual([]);
	});

	test('no telemetry SDK or native event capability is shipped', () => {
		for (const path of [
			join(app, 'package.json'),
			join(root, 'bun.lock'),
			join(app, 'src-tauri/Cargo.toml'),
			join(app, 'src-tauri/Cargo.lock'),
			join(app, 'src-tauri/capabilities/default.json'),
		]) {
			expect(readFileSync(path, 'utf8')).not.toMatch(/aptabase/i);
		}
	});

	test('no analytics opt-in setting or settings navigation remains', () => {
		expect(readFileSync(join(app, 'src/lib/settings/settings.ts'), 'utf8'))
			.not.toContain('analytics.enabled');
		expect(
			readFileSync(
				join(app, 'src/routes/(app)/(config)/settings/SidebarNav.svelte'),
				'utf8',
			),
		).not.toContain('/settings/analytics');
	});
});
