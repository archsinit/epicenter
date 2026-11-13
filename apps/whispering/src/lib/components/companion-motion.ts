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

/** Independently born billows: slow outward growth, uneven directions and small flutter. */
export function buildBillows(seconds: number, target = new Float32Array(32)): Float32Array {
	const time = Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
	const random = (seed: number) => { const n = Math.sin(seed * 127.1 + 311.7) * 43758.5453; return n - Math.floor(n); };
	const smooth = (n: number) => { const t = Math.max(0, Math.min(1, n)); return t * t * (3 - 2 * t); };
	for (let i = 0; i < 8; i++) {
		const cycle = time * (0.10 + random(i + 1) * 0.045) + i / 8;
		const age = cycle - Math.floor(cycle);
		const seed = i + Math.floor(cycle) * 7.13;
		const angle = random(seed + 2.7) * Math.PI * 2;
		const distance = 0.08 + age * 0.96;
		const curl = Math.sin(age * 4.2 + random(seed + 4) * Math.PI * 2) * age * 0.15;
		const width = (0.20 + age * 0.26) * (1 + Math.sin(time * (2.2 + i * 0.31) + i) * 0.10);
		target[i * 4] = Math.cos(angle) * distance - Math.sin(angle) * curl;
		target[i * 4 + 1] = Math.sin(angle) * distance + Math.cos(angle) * curl;
		target[i * 4 + 2] = width;
		target[i * 4 + 3] = smooth(age / 0.12) * (1 - smooth((age - 0.64) / 0.36));
	}
	return target;
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
