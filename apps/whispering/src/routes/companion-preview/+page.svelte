<script lang="ts">
	import { dev } from '$app/environment';
	import { onMount } from 'svelte';
	import Companion from '$lib/components/Companion.svelte';

	type Phase =
		| 'idle'
		| 'preparing'
		| 'recording'
		| 'transcribing'
		| 'cleaning'
		| 'error';
	const phases: Phase[] = [
		'idle',
		'recording',
		'transcribing',
		'cleaning',
		'preparing',
		'error',
	];
	let phase = $state<Phase>('idle');
	let levels = $state<number[]>(Array(24).fill(0));
	let gaze = $state([0, 0]);
	let light = $state(false);
	let signal = $state(true);
	onMount(() => {
		if (!dev) return;
		const pointer = (event: PointerEvent) => {
			gaze = [
				(event.clientX - innerWidth / 2) / 400,
				(event.clientY - innerHeight / 2) / 400,
			];
		};
		window.addEventListener('pointermove', pointer);
		// Explicit visual-demo data only: never used by the production recording overlay.
		const timer = setInterval(() => {
			const t = performance.now() / 240;
			levels = [
				...levels.slice(1),
				signal && phase === 'recording'
					? Math.abs(Math.sin(t * 2.1) * Math.sin(t * 0.83)) * 0.95
					: 0,
			];
		}, 40);
		return () => {
			clearInterval(timer);
			window.removeEventListener('pointermove', pointer);
		};
	});
</script>

{#if dev}
	<main class:light>
		<header>
			<h1>Gastly motion prototype</h1>
			<p>
				Design review · simulated audio · move your pointer to turn its head
			</p>
		</header>
		<Companion
			{phase}
			{levels}
			{gaze}
			label={phase === 'idle'
				? 'Ready'
				: phase === 'recording'
					? signal
						? 'Listening · demo audio'
						: 'Listening · quiet'
					: phase === 'cleaning'
						? 'Cleaning up'
						: phase === 'transcribing'
							? 'Transcribing'
							: phase === 'error'
								? 'Something went wrong'
								: 'Preparing microphone'}
		/>
		<nav aria-label="Preview state">
			{#each phases as option}
				<button
					class:selected={phase === option}
					onclick={() => {
						phase = option;
						levels = Array(24).fill(0);
					}}>{option}</button
				>
			{/each}
		</nav>
		<footer>
			<label
				><input type="checkbox" bind:checked={light} /> Light background</label
			>
			<label
				><input type="checkbox" bind:checked={signal} /> Demo voice activity</label
			>
		</footer>
		<p class="note">
			Procedural evolving smoke and a turning textured sphere, not a fluid
			simulation. Success animation is not wired yet.
		</p>
	</main>
{:else}
	<p>This visual prototype is available only in development.</p>
{/if}

<style>
	main {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 24px;
		background: #100d19;
		color: #eee5fa;
		font:
			14px -apple-system,
			BlinkMacSystemFont,
			sans-serif;
	}
	main.light {
		background: #eeeaf3;
		color: #292031;
	}
	header {
		text-align: center;
	}
	h1 {
		font-size: 22px;
		margin-bottom: 8px;
	}
	p {
		opacity: 0.65;
	}
	nav,
	footer {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		justify-content: center;
	}
	button {
		border: 1px solid #9764bb66;
		padding: 8px 12px;
		border-radius: 8px;
		cursor: pointer;
	}
	button.selected {
		background: #8751b8;
		color: white;
	}
	label {
		display: flex;
		align-items: center;
		gap: 7px;
	}
	.note {
		max-width: 500px;
		text-align: center;
		font-size: 12px;
	}
</style>
