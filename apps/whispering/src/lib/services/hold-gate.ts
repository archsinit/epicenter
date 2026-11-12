/** Coordinate overlapping keyboard/mouse push-to-talk without stopping another held input. */
export class HoldGate {
	private held = new Set<string>();

	isHeld() {
		return this.held.size > 0;
	}

	update(source: string, pressed: boolean): 'start' | 'stop' | null {
		const wasHeld = this.held.size > 0;
		if (pressed) this.held.add(source);
		else this.held.delete(source);
		if (!wasHeld && this.held.size > 0) return 'start';
		if (wasHeld && this.held.size === 0) return 'stop';
		return null;
	}
}
