<script lang="ts">
	import { onMount } from 'svelte';
	import { gazeAxis } from '$lib/components/companion-motion';
	import { createCompanionRenderer } from '$lib/components/companion-renderer';

	type Phase =
		| 'idle'
		| 'preparing'
		| 'recording'
		| 'transcribing'
		| 'cleaning'
		| 'error';
	let {
		phase,
		levels,
		label,
		disconnected = false,
		gaze = [0, 0],
		warning = false,
		onDrag,
	}: {
		phase: Phase;
		levels: number[];
		label: string;
		disconnected?: boolean;
		gaze?: number[];
		warning?: boolean;
		onDrag?: () => void;
	} = $props();
	const mode = $derived(
		disconnected || phase === 'error'
			? 'error'
			: phase === 'recording'
				? 'wave'
				: phase === 'transcribing' || phase === 'cleaning'
					? 'vortex'
					: 'idle',
	);
	let canvas: HTMLCanvasElement;
	let ready = $state(false);
	let reduced = $state(false);

	onMount(() => {
		const renderer = createCompanionRenderer(canvas);
		if (!renderer) return;
		const media = window.matchMedia('(prefers-reduced-motion: reduce)');
		const updateMotion = () => {
			reduced = media.matches;
		};
		updateMotion();
		media.addEventListener('change', updateMotion);
		let frame = 0,
			previous = 0,
			start = 0;
		let wave = 0,
			processing = 0,
			yaw = 0,
			pitch = 0;
		let previousSignature = '';
		function animate(now: number) {
			frame = requestAnimationFrame(animate);
			if (document.hidden || now - previous < 1000 / 24) return;
			const dt = Math.min(100, now - previous);
			previous = now;
			if (!start) start = now;
			const blend = reduced ? 1 : 1 - Math.exp(-dt / 180);
			wave += ((mode === 'wave' ? 1 : 0) - wave) * blend;
			processing += ((mode === 'vortex' ? 1 : 0) - processing) * blend;
			yaw += ((reduced ? 0 : gazeAxis(gaze[0]) * 0.65) - yaw) * blend;
			pitch += ((reduced ? 0 : gazeAxis(gaze[1]) * 0.45) - pitch) * blend;
			const time = reduced ? 0 : (now - start) / 1000;
			const cycle = time % 7.8;
			const blink =
				!reduced && cycle > 4.2 && cycle < 4.42
					? Math.sin(((cycle - 4.2) / 0.22) * Math.PI)
					: 0;
			const signature = `${mode}:${warning}:${disconnected}`;
			if (reduced && ready && signature === previousSignature) return;
			ready =
				renderer?.draw({
					time,
					wave,
					processing,
					yaw,
					pitch,
					blink,
					failure: mode === 'error' ? 1 : 0,
				}) ?? false;
			previousSignature = signature;
		}
		frame = requestAnimationFrame(animate);
		return () => {
			cancelAnimationFrame(frame);
			media.removeEventListener('change', updateMotion);
			renderer.dispose();
		};
	});
</script>

<div
	class="companion"
	data-mode={mode}
	data-renderer={ready ? 'webgl' : 'fallback'}
	data-motion={reduced ? 'reduced' : 'full'}
>
	<canvas
		bind:this={canvas}
		class:ready
		role="img"
		aria-label={ready
			? 'Gastly with evolving smoke'
			: 'Gastly fallback graphic'}
	></canvas>
	{#if !ready}
		<div class="fallback" aria-hidden="true">
			{#each Array(12) as _, index}
				<span style:--i={index}></span>
			{/each}
			{#if mode === 'idle' || mode === 'error'}<img
					src="/companion/idle-head.png"
					alt=""
				/>{/if}
		</div>
	{/if}
	<div class="waveform" class:visible={mode === 'wave'} aria-hidden="true">
		{#each levels as level, index (index)}
			<span style:height="{1 + Math.max(0, Math.min(1, level)) * 23}px"></span>
		{/each}
	</div>
	<button
		class="drag-target"
		class:wide={mode === 'wave'}
		tabindex="-1"
		aria-label="Drag Gastly"
		onpointerdown={(event) => {
			if (event.button === 0) {
				event.preventDefault();
				onDrag?.();
			}
		}}
	></button>
	{#if mode === 'error' || warning}<span
			class="warning"
			class:error={mode === 'error'}
			aria-hidden="true"
		></span>{/if}
	<span class="status" role="status" aria-live="polite" aria-atomic="true"
		>{label}</span
	>
</div>

<style>
	.companion {
		position: relative;
		width: 180px;
		height: 120px;
		pointer-events: none;
		isolation: isolate;
		background: transparent;
		border: 0;
		outline: 0;
		box-shadow: none;
	}
	canvas {
		display: block;
		width: 180px;
		height: 120px;
		opacity: 0;
	}
	canvas.ready {
		opacity: 1;
	}
	.waveform {
		position: absolute;
		top: 39px;
		left: 32px;
		width: 116px;
		height: 27px;
		display: flex;
		justify-content: center;
		align-items: center;
		gap: 2.5px;
		opacity: 0;
		transition: opacity 350ms ease;
	}
	.waveform.visible {
		opacity: 1;
	}
	.waveform span {
		width: 1.5px;
		border-radius: 2px;
		background: #eedaff;
		box-shadow: 0 0 4px #c779ff;
		transition: height 60ms linear;
	}
	.drag-target {
		position: absolute;
		left: 35px;
		top: 3px;
		width: 110px;
		height: 100px;
		clip-path: ellipse(50% 50%);
		pointer-events: auto;
		cursor: grab;
		touch-action: none;
		padding: 0;
		margin: 0;
		border: 0;
		outline: 0;
		box-shadow: none;
		background: transparent;
		-webkit-app-region: no-drag;
	}
	.drag-target.wide {
		left: 8px;
		top: 29px;
		width: 164px;
		height: 48px;
	}
	.drag-target:active {
		cursor: grabbing;
	}
	.status {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
		border: 0;
	}
	.warning {
		position: absolute;
		left: 87px;
		top: 98px;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #e7b667;
		box-shadow: 0 0 5px #e7b66760;
	}
	.warning.error {
		background: #fb7185;
		box-shadow: 0 0 5px #fb718560;
	}
	.fallback {
		position: absolute;
		inset: 0;
	}
	.fallback img {
		position: absolute;
		width: 35px;
		height: 35px;
		left: 72.5px;
		top: 35px;
	}
	.fallback span {
		position: absolute;
		left: 50px;
		top: 17px;
		width: 80px;
		height: 80px;
		border-radius: 50%;
		background: radial-gradient(
			ellipse,
			#9351c92a,
			#a166d918 35%,
			transparent 68%
		);
		animation: billow calc(4s + var(--i) * 0.17s) ease-in-out infinite alternate;
		animation-delay: calc(var(--i) * -0.6s);
	}
	@keyframes billow {
		from {
			transform: translate(
					calc(sin(var(--i)) * 18px),
					calc(cos(var(--i)) * 16px)
				)
				scale(0.55);
			opacity: 0.25;
		}
		to {
			transform: translate(
					calc(cos(var(--i)) * 25px),
					calc(sin(var(--i)) * 24px)
				)
				scale(1.2);
			opacity: 0.8;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		*,
		*::before,
		*::after {
			animation: none !important;
			transition: none !important;
		}
	}
</style>
