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

export type FormMotion = { wave: number; processing: number; head: number; spin: number };
export function initialFormMotion(): FormMotion {
	return { wave: 0, processing: 0, head: 1, spin: 0 };
}

/** Preserve the entering morph, but settle the swirl before revealing a returning face. */
export function advanceFormMotion(
	previous: FormMotion, mode: 'idle' | 'wave' | 'vortex' | 'error',
	milliseconds: number, reduced: boolean,
): FormMotion {
	const dt = Number.isFinite(milliseconds) ? Math.max(0, Math.min(100, milliseconds)) : 0;
	const blend = reduced ? 1 : 1 - Math.exp(-dt / 180);
	const wave = previous.wave + ((mode === 'wave' ? 1 : 0) - previous.wave) * blend;
	const processing = previous.processing + ((mode === 'vortex' ? 1 : 0) - previous.processing) * blend;
	const reveal = (mode === 'idle' || mode === 'error') && wave < 0.08 && processing < 0.08;
	const head = previous.head + ((reveal ? 1 : 0) - previous.head) * blend;
	const t = Math.max(0, Math.min(1, (processing - 0.12) / 0.53));
	const speed = t * t * (3 - 2 * t);
	const spin = reduced ? previous.spin : (previous.spin + dt * 0.00052 * speed) % (Math.PI * 2);
	return { wave, processing, head, spin };
}
