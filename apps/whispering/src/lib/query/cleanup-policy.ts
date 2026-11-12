import type { Settings } from '$lib/settings';

/** Automatic cleanup requires both explicit opt-in and a selected transformation. */
export function automaticCleanupId(
	settings: Pick<
		Settings,
		| 'transformations.cleanupEnabled'
		| 'transformations.selectedTransformationId'
	>,
): string | null {
	return settings['transformations.cleanupEnabled']
		? settings['transformations.selectedTransformationId']
		: null;
}
