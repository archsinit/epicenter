import { expect, test } from 'bun:test';
import { advanceFormMotion, buildBillows, gazeAxis, initialFormMotion, waveformLevel } from './companion-motion';

test('only fresh real samples produce a waveform', () => {
	expect(waveformLevel(0.0625, true)).toBe(0.5);
	expect(waveformLevel(1, true)).toBe(1);
	expect(waveformLevel(0, true)).toBe(0);
	expect(waveformLevel(1, false)).toBe(0);
	expect(waveformLevel(null, true)).toBe(0);
});

test('invalid sample amplitudes cannot generate CSS heights', () => {
	for (const level of [NaN, Infinity, -Infinity, -1]) {
		expect(waveformLevel(level, true)).toBe(0);
	}
});

test('billows grow outward independently with finite soft lifetimes', () => {
	const early = buildBillows(1), later = buildBillows(3);
	expect(Math.hypot(later[0] ?? 0, later[1] ?? 0)).toBeGreaterThan(Math.hypot(early[0] ?? 0, early[1] ?? 0));
	for (let i = 0; i < 8; i++) {
		expect(Number.isFinite(later[i * 4])).toBe(true);
		expect(Number.isFinite(later[i * 4 + 1])).toBe(true);
		expect(later[i * 4 + 2] ?? 0).toBeGreaterThan(0.1);
		expect(later[i * 4 + 3] ?? -1).toBeGreaterThanOrEqual(0);
		expect(later[i * 4 + 3] ?? 2).toBeLessThanOrEqual(1);
	}
	expect(later).not.toEqual(early);
	expect(buildBillows(3)).toEqual(later);
	expect(buildBillows(NaN)).toEqual(buildBillows(0));
});

test('entering processing retains a gradual face-to-gas morph', () => {
	const next = advanceFormMotion(initialFormMotion(), 'vortex', 40, false);
	expect(next.processing).toBeGreaterThan(0);
	expect(next.processing).toBeLessThan(1);
	expect(next.head).toBeGreaterThan(0);
	expect(next.head).toBeLessThan(1);
});

test('processing phase runs in the reversed direction and wraps continuously', () => {
	const state = { wave: 0, processing: 1, head: 0, spin: 3 };
	const next = advanceFormMotion(state, 'vortex', 40, false);
	expect(next.spin).toBeLessThan(state.spin);
	expect(next.spin).toBeGreaterThan(2.9);
	const wrapped = advanceFormMotion({ ...state, spin: 0.001 }, 'vortex', 40, false);
	expect(wrapped.spin).toBeGreaterThan(6);
	expect(wrapped.spin).toBeLessThan(Math.PI * 2);
	expect(advanceFormMotion(state, 'vortex', 0, false).spin).toBe(state.spin);
});

test('returning face is held back until the process swirl has stopped', () => {
	let state = { wave: 0, processing: 1, head: 0, spin: 1.2 };
	let revealed = false;
	for (let i = 0; i < 60; i++) {
		const previous = state;
		state = advanceFormMotion(state, 'idle', 40, false);
		if (state.head > 0.001) {
			revealed = true;
			expect(state.processing).toBeLessThan(0.08);
			expect(state.spin).toBe(previous.spin);
		}
	}
	expect(revealed).toBe(true);
	expect(state.head).toBeGreaterThan(0.99);
});

test('recording-to-processing keeps one continuous gas form without a face flash', () => {
	let state = { wave: 1, processing: 0, head: 0, spin: 0 };
	for (let i = 0; i < 30; i++) {
		state = advanceFormMotion(state, 'vortex', 40, false);
		expect(state.wave + state.processing).toBeCloseTo(1, 8);
		expect(state.head).toBe(0);
	}
});

test('return travel expands the waveform into gas without a face or processing spin', () => {
	let state = { wave: 1, processing: 0, head: 0, spin: 2 };
	for (let i = 0; i < 15; i++) {
		state = advanceFormMotion(state, 'cloud', 40, false);
		expect(state.head).toBe(0);
		expect(state.processing).toBe(0);
		expect(state.spin).toBe(2);
	}
	expect(state.wave).toBeLessThan(0.05);
	// A short take may interrupt an older processing morph; hold even its residual spin.
	expect(advanceFormMotion({ ...state, processing: 0.5 }, 'cloud', 40, false).spin).toBe(state.spin);
	const arrived = advanceFormMotion(state, 'vortex', 40, false);
	expect(arrived.processing).toBeGreaterThan(0);
	expect(arrived.head).toBe(0);
});

test('a job finishing during return reveals the face only after arrival, without fake processing', () => {
	let state = { wave: 1, processing: 0, head: 0, spin: 0 };
	for (let i = 0; i < 20; i++) state = advanceFormMotion(state, 'cloud', 40, false);
	expect(state.head).toBe(0);
	const arrived = advanceFormMotion(state, 'idle', 40, false);
	expect(arrived.head).toBeGreaterThan(0);
	expect(arrived.processing).toBe(0);
	expect(arrived.spin).toBe(0);
	expect(advanceFormMotion(initialFormMotion(), 'cloud', 40, true)).toEqual({ wave: 0, processing: 0, head: 0, spin: 0 });
});

test('reduced motion changes form immediately without spinning', () => {
	const process = advanceFormMotion(initialFormMotion(), 'vortex', 40, true);
	expect(process).toEqual({ wave: 0, processing: 1, head: 0, spin: 0 });
	expect(advanceFormMotion(process, 'idle', 40, true)).toEqual(initialFormMotion());
});

test('gaze remains bounded with a neutral fallback', () => {
	expect(gazeAxis(0.4)).toBe(0.4);
	expect(gazeAxis(5)).toBe(1);
	expect(gazeAxis(-5)).toBe(-1);
	for (const value of [undefined, NaN, Infinity, -Infinity]) {
		expect(gazeAxis(value)).toBe(0);
	}
});
