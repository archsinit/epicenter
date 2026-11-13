import { expect, test } from 'bun:test';
import { gazeAxis, waveformLevel } from './companion-motion';

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

test('gaze remains bounded with a neutral fallback', () => {
	expect(gazeAxis(0.4)).toBe(0.4);
	expect(gazeAxis(5)).toBe(1);
	expect(gazeAxis(-5)).toBe(-1);
	for (const value of [undefined, NaN, Infinity, -Infinity]) {
		expect(gazeAxis(value)).toBe(0);
	}
});
