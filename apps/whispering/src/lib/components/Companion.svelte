<script lang="ts">
	import { gazeAxis } from '$lib/components/companion-motion';
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
	}: {
		phase: Phase;
		levels: number[];
		label: string;
		disconnected?: boolean;
		gaze?: number[];
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
	const x = $derived(gazeAxis(gaze[0]));
	const y = $derived(gazeAxis(gaze[1]));
</script>

<div
	class="companion"
	data-mode={mode}
	class:cleaning={phase === 'cleaning'}
	style:--look-x={x}
	style:--look-y={y}
>
	<div class="creature" aria-hidden="true">
		<div class="aura">
			<img
				class="smoke smoke-primary"
				src="/companion/idle-smoke.png"
				alt=""
				draggable="false"
			/>
			<img
				class="smoke smoke-secondary"
				src="/companion/idle-smoke.png"
				alt=""
				draggable="false"
			/>
		</div>
		<div class="vortex">
			<img src="/companion/vortex.png" alt="" draggable="false" />
		</div>
		<div class="head-position">
			<div class="head-bob">
				<div class="head-look">
					<img
						class="face"
						src="/companion/idle-head.png"
						alt=""
						draggable="false"
					/>
					<span class="eyelid left"></span><span class="eyelid right"></span>
				</div>
			</div>
		</div>
		<div class="waveform">
			<div class="wave-baseline"></div>
			{#each levels as level, index (index)}
				<span style:height="{2 + Math.max(0, Math.min(1, level)) * 46}px"
				></span>
			{/each}
		</div>
	</div>
	<div
		class="status"
		class:resting={phase === 'idle' && !disconnected}
		class:failure={mode === 'error'}
		role="status"
		aria-live="polite"
		aria-atomic="true"
	>
		{label}
	</div>
</div>

<style>
	.companion {
		width: 360px;
		height: 240px;
		position: relative;
		pointer-events: none;
		font:
			11px/1.3 -apple-system,
			BlinkMacSystemFont,
			sans-serif;
		color: #faf5ff;
		isolation: isolate;
	}
	.creature {
		position: absolute;
		inset: 0 0 28px;
	}
	.aura,
	.vortex {
		position: absolute;
		width: 240px;
		height: 214px;
		left: 60px;
		top: 0;
		transition:
			transform 650ms ease-in-out,
			opacity 500ms ease,
			filter 400ms ease;
	}
	.smoke,
	.vortex img {
		width: 100%;
		height: 100%;
		object-fit: contain;
	}
	.smoke {
		position: absolute;
		inset: 0;
	}
	.smoke-primary {
		animation: vapor 11s ease-in-out infinite alternate;
	}
	.smoke-secondary {
		opacity: 0.32;
		animation: wisps 17s ease-in-out infinite alternate;
	}
	.head-position {
		position: absolute;
		left: 145px;
		top: 70px;
		width: 70px;
		height: 70px;
		transition:
			opacity 250ms ease,
			transform 650ms ease-in-out;
	}
	.head-bob {
		width: 100%;
		height: 100%;
		animation: float 5.5s ease-in-out infinite;
	}
	.head-look {
		position: relative;
		width: 100%;
		height: 100%;
		transform: translate(calc(var(--look-x) * 3px), calc(var(--look-y) * 2px))
			rotate(calc(var(--look-x) * 5deg));
		transition: transform 260ms ease-out;
	}
	.face {
		display: block;
		width: 100%;
		height: 100%;
	}
	.eyelid {
		position: absolute;
		top: 34%;
		width: 34%;
		height: 34%;
		background: linear-gradient(#32233f, #211829);
		transform-origin: top;
		animation: blink 8.7s linear infinite;
		transform: scaleY(0);
	}
	.left {
		left: 12%;
		clip-path: polygon(0 0, 100% 52%, 95% 85%, 70% 100%, 28% 95%, 6% 68%);
	}
	.right {
		right: 12%;
		clip-path: polygon(100% 0, 0 52%, 5% 85%, 30% 100%, 72% 95%, 94% 68%);
	}
	.vortex {
		opacity: 0;
		transform: scale(0.3);
	}
	.vortex img {
		animation: swirl 8s linear infinite;
		animation-play-state: paused;
	}
	.waveform {
		position: absolute;
		left: 65px;
		top: 75px;
		width: 230px;
		height: 60px;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 5px;
		opacity: 0;
		transform: scaleX(0.25);
		transition:
			opacity 350ms ease,
			transform 650ms ease-in-out;
	}
	.waveform > span {
		z-index: 1;
		width: 3px;
		border-radius: 4px;
		background: #e9c5ff;
		box-shadow:
			0 0 5px #c676ff,
			0 0 12px #913cff;
		transition: height 60ms linear;
	}
	.wave-baseline {
		position: absolute;
		inset: 29px 5px auto;
		height: 1px;
		background: linear-gradient(90deg, transparent, #d4a5fa, transparent);
		box-shadow: 0 0 12px #b34dff;
	}
	.status {
		position: absolute;
		bottom: 13px;
		left: 14px;
		right: 14px;
		width: fit-content;
		max-width: 332px;
		margin: auto;
		padding: 5px 10px;
		border-radius: 12px;
		background: #191020e8;
		text-align: center;
		text-shadow: 0 1px 2px #000;
		transition: opacity 200ms ease;
	}
	.resting {
		opacity: 0;
	}
	.failure {
		color: #ffd2df;
	}
	[data-mode='wave'] .aura {
		transform: scale(1.4, 0.52);
	}
	[data-mode='wave'] .head-position {
		opacity: 0;
		transform: scale(1.8, 0.08);
	}
	[data-mode='wave'] .waveform {
		opacity: 1;
		transform: scaleX(1);
	}
	[data-mode='vortex'] .head-position {
		opacity: 0;
		transform: scale(0.1) rotate(100deg);
	}
	[data-mode='vortex'] .aura {
		opacity: 0.22;
		transform: rotate(45deg) scale(0.92);
	}
	[data-mode='vortex'] .vortex {
		opacity: 1;
		transform: scale(1);
	}
	[data-mode='vortex'] .vortex img {
		animation-play-state: running;
	}
	.cleaning .vortex {
		filter: hue-rotate(18deg) brightness(1.13);
	}
	[data-mode='error'] .aura {
		filter: hue-rotate(35deg);
	}
	@keyframes float {
		0%,
		100% {
			transform: translateY(0) rotate(-2deg);
		}
		50% {
			transform: translateY(-5px) rotate(2deg);
		}
	}
	@keyframes vapor {
		from {
			transform: rotate(-7deg) scale(0.97);
		}
		to {
			transform: rotate(9deg) scale(1.04);
		}
	}
	@keyframes wisps {
		from {
			transform: rotate(12deg) scale(1.04, 0.96);
		}
		to {
			transform: rotate(-14deg) scale(0.96, 1.04);
		}
	}
	@keyframes swirl {
		to {
			transform: rotate(360deg);
		}
	}
	@keyframes blink {
		0%,
		44%,
		47%,
		100% {
			transform: scaleY(0);
		}
		45%,
		46% {
			transform: scaleY(1);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		*,
		*::before,
		*::after {
			animation: none !important;
			transition: none !important;
		}
		.head-look {
			transform: none;
		}
	}
</style>
