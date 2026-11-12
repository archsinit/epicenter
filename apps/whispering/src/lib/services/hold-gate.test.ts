import { expect, test } from 'bun:test';
import { HoldGate } from './hold-gate';

test('keyboard and mouse stay active until both are released', () => {
	const gate = new HoldGate();
	expect(gate.update('keyboard', true)).toBe('start');
	expect(gate.update('mouse-3', true)).toBeNull();
	expect(gate.update('keyboard', false)).toBeNull();
	expect(gate.update('mouse-3', false)).toBe('stop');
});
test('multiple mouse buttons work alongside keyboard shortcuts', () => {
	const gate = new HoldGate();
	expect(gate.update('mouse-3', true)).toBe('start');
	expect(gate.update('mouse-4', true)).toBeNull();
	expect(gate.update('mouse-3', false)).toBeNull();
	expect(gate.update('mouse-4', false)).toBe('stop');
});
test('repeated presses and unmatched releases do not trigger extra actions', () => {
	const gate = new HoldGate();
	expect(gate.update('keyboard', false)).toBeNull();
	expect(gate.update('keyboard', true)).toBe('start');
	expect(gate.update('keyboard', true)).toBeNull();
	expect(gate.update('keyboard', false)).toBe('stop');
	expect(gate.update('keyboard', false)).toBeNull();
});
