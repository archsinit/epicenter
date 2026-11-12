import { Ok } from 'wellcrafted/result';
import { WhisperingErr } from '$lib/result';
import {
	mouseShortcuts as service,
	type MouseBinding,
} from '$lib/services/mouse-shortcuts';
import { settings } from '$lib/stores/settings.svelte';
import { defineMutation } from './_client';

export const mouseShortcuts = {
	configure: defineMutation({
		mutationKey: ['mouseShortcuts', 'configure'] as const,
		mutationFn: async (bindings: MouseBinding[]) => {
			const result = await service.configure(bindings);
			if (result.error)
				return WhisperingErr({
					title: 'Mouse shortcuts could not be enabled',
					description: result.error.message,
				});
			settings.updateKey('shortcuts.mouse.bindings', bindings);
			return Ok(undefined);
		},
	}),
};
