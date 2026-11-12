import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createTaggedError, extractErrorMessage } from 'wellcrafted/error';
import { tryAsync } from 'wellcrafted/result';
import type { ShortcutEventState } from '$lib/commands';

export type MouseBinding = { button: number; commandId: string };
export type MouseAction = {
	commandId: string;
	state: ShortcutEventState;
	source: string;
};
const { MouseShortcutErr } = createTaggedError('MouseShortcutError');

export const mouseShortcuts = {
	configure(bindings: MouseBinding[]) {
		return tryAsync({
			try: () => invoke<void>('configure_mouse_bindings', { bindings }),
			catch: (error) =>
				MouseShortcutErr({ message: extractErrorMessage(error) }),
		});
	},
	active() {
		return tryAsync({
			try: () => invoke<boolean>('mouse_shortcuts_active'),
			catch: (error) =>
				MouseShortcutErr({ message: extractErrorMessage(error) }),
		});
	},
	listen(callback: (action: MouseAction) => void) {
		return tryAsync({
			try: () =>
				listen<MouseAction>('mouse-shortcut', (event) =>
					callback(event.payload),
				),
			catch: (error) =>
				MouseShortcutErr({ message: extractErrorMessage(error) }),
		});
	},
};
