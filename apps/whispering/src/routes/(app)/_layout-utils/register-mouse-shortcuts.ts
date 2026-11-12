import { commands } from '$lib/commands';
import { rpc } from '$lib/query';
import { mouseShortcuts } from '$lib/services/mouse-shortcuts';
import { settings } from '$lib/stores/settings.svelte';

export async function registerMouseShortcuts() {
	const { data: unlisten, error } = await mouseShortcuts.listen((action) => {
		const command = commands.find((command) => command.id === action.commandId);
		if (command?.on.some((state) => state === action.state))
			command.callback(action.state, action.source);
	});
	if (error) {
		rpc.notify.error.execute({
			title: 'Mouse shortcut listener unavailable',
			description: error.message,
		});
		return () => {};
	}
	const configured = await mouseShortcuts.configure(
		settings.value['shortcuts.mouse.bindings'],
	);
	if (configured.error) {
		rpc.notify.error.execute({
			title: 'Mouse shortcuts are inactive',
			description: configured.error.message,
			action: {
				type: 'link',
				label: 'Configure mouse buttons',
				href: '/settings/shortcuts/mouse',
			},
		});
	}
	return unlisten;
}
