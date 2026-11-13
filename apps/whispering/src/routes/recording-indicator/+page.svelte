<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';
	import Companion from '$lib/components/Companion.svelte';
	import { waveformLevel } from '$lib/components/companion-motion';

	type Snapshot = {
		phase:
			| 'idle'
			| 'preparing'
			| 'recording'
			| 'transcribing'
			| 'cleaning'
			| 'error';
		level: number | null;
		inputActive: boolean;
		gaze?: number[];
	};
	let snapshot = $state<Snapshot>({
		phase: 'idle',
		level: null,
		inputActive: false,
	});
	let levels = $state<number[]>(Array(24).fill(0));
	let disconnected = $state(false);
	let dragFailed = $state(false);
	function startDrag() {
		dragFailed = false;
		invoke<void>('start_indicator_drag').catch(() => {
			dragFailed = true;
		});
	}
	const recording = $derived(snapshot.phase === 'recording');
	const label = $derived(
		dragFailed
			? 'Could not move companion'
			: disconnected
				? 'Companion disconnected'
				: snapshot.phase === 'error'
					? 'Something went wrong'
					: snapshot.phase === 'cleaning'
						? 'Cleaning up'
						: snapshot.phase === 'transcribing'
							? 'Transcribing'
							: snapshot.phase === 'preparing'
								? 'Preparing microphone'
								: recording && snapshot.level === null
									? 'Recording · meter unavailable'
									: recording && !snapshot.inputActive
										? 'No microphone signal'
										: recording && (snapshot.level ?? 0) < 0.003
											? 'Listening · quiet'
											: recording
												? 'Listening'
												: 'Ready',
	);

	onMount(() => {
		document.documentElement.classList.add('indicator-window');
		const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
		let stopped = false;
		let timer: ReturnType<typeof setTimeout>;
		async function poll() {
			await invoke<Snapshot>('get_indicator_state')
				.then((value) => {
					if (stopped) return;
					disconnected = false;
					if (value.phase !== snapshot.phase) levels = Array(24).fill(0);
					snapshot = value;
					if (value.phase === 'recording') {
						levels = [
							...levels.slice(1),
							waveformLevel(value.level, value.inputActive),
						];
					}
				})
				.catch(() => {
					if (!stopped) {
						disconnected = true;
						levels = Array(24).fill(0);
					}
				});
			if (!stopped)
				timer = setTimeout(
					poll,
					disconnected
						? 250
						: snapshot.phase === 'idle'
							? reducedMotion.matches
								? 100
								: 40
							: 40,
				);
		}
		poll();
		return () => {
			stopped = true;
			clearTimeout(timer);
			document.documentElement.classList.remove('indicator-window');
		};
	});
</script>

<div class="indicator-shell">
	<Companion
		phase={snapshot.phase}
		{levels}
		{label}
		{disconnected}
		gaze={snapshot.gaze}
		warning={dragFailed ||
			(recording && (snapshot.level === null || !snapshot.inputActive))}
		onDrag={startDrag}
	/>
</div>

<style>
	:global(html.indicator-window),
	:global(html.indicator-window body) {
		background: transparent !important;
		border: 0 !important;
		outline: 0 !important;
		box-shadow: none !important;
		margin: 0;
		overflow: hidden;
	}
	.indicator-shell {
		height: 100vh;
		display: grid;
		place-items: center;
		pointer-events: none;
	}
</style>
