/** Real sample amplitude only; stale, unavailable or invalid samples settle to zero. */
export function waveformLevel(level: number | null, inputActive: boolean): number {
	if (!inputActive || level === null || !Number.isFinite(level)) return 0;
	return Math.min(1, Math.sqrt(Math.max(0, level)) * 2);
}

/** Keep native/fallback pointer data from producing invalid CSS transforms. */
export function gazeAxis(value: number | undefined): number {
	return value !== undefined && Number.isFinite(value)
		? Math.max(-1, Math.min(1, value)) : 0;
}
