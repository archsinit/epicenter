<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';

	type Snapshot = {
		phase: 'idle' | 'preparing' | 'recording' | 'transcribing' | 'cleaning' | 'error';
		level: number | null;
		inputActive: boolean;
	};
	let snapshot = $state<Snapshot>({ phase: 'idle', level: null, inputActive: false });
	let levels = $state<number[]>(Array(24).fill(0));
	let disconnected = $state(false);
	const recording = $derived(snapshot.phase === 'recording');
	const processing = $derived(snapshot.phase === 'transcribing' || snapshot.phase === 'cleaning');
	const label = $derived(disconnected ? 'Indicator disconnected'
		: snapshot.phase === 'error' ? 'Something went wrong'
		: snapshot.phase === 'cleaning' ? 'Cleaning up'
		: snapshot.phase === 'transcribing' ? 'Transcribing'
		: snapshot.phase === 'preparing' ? 'Preparing microphone'
		: recording && snapshot.level === null ? 'Recording · meter unavailable'
		: recording && !snapshot.inputActive ? 'No microphone signal'
		: recording && (snapshot.level ?? 0) < 0.003 ? 'Listening · quiet'
		: recording ? 'Listening' : 'Ready');

	onMount(() => {
		document.documentElement.classList.add('indicator-window');
		let stopped = false;
		let timer: ReturnType<typeof setTimeout>;
		async function poll() {
			await invoke<Snapshot>('get_indicator_state').then((value) => {
				if (stopped) return;
				disconnected = false;
				if (value.phase !== snapshot.phase) levels = Array(24).fill(0);
				snapshot = value;
				if (value.phase === 'recording') {
					levels = [...levels.slice(1), Math.min(1, Math.sqrt(value.level ?? 0) * 2)];
				}
			}).catch(() => { if (!stopped) disconnected = true; });
			if (!stopped) timer = setTimeout(poll, snapshot.phase === 'idle' || disconnected ? 250 : 40);
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
	<div class="recording-pill" class:error={snapshot.phase === 'error' || disconnected} role="status" aria-live="polite">
		{#if processing}
			<span class="spinner" aria-hidden="true"></span>
		{:else}
			<div class="waveform" aria-hidden="true">
				{#each levels as level, index (index)}
					<span style:height="{2 + (recording ? level : 0) * 22}px"></span>
				{/each}
			</div>
		{/if}
		<span class="label">{label}</span>
	</div>
</div>

<style>
	:global(html.indicator-window), :global(html.indicator-window body) {
		background: transparent !important;
		margin: 0;
		overflow: hidden;
	}
	.indicator-shell { height: 100vh; display: grid; place-items: center; }
	.recording-pill {
		display: flex; align-items: center; gap: 12px; padding: 12px 18px;
		border-radius: 28px; background: #18181bea; color: #f4f4f5;
		border: 1px solid #ffffff26; box-shadow: 0 3px 10px #0004;
		font: 12px -apple-system, BlinkMacSystemFont, sans-serif;
	}
	.error { color: #fca5a5; }
	.label { white-space: nowrap; }
	.waveform { height: 24px; display: flex; align-items: center; gap: 2px; }
	.waveform span { width: 2px; border-radius: 2px; background: #a7f3d0; transition: height 40ms linear; }
	.spinner { width: 16px; height: 16px; border: 2px solid #ffffff40; border-top-color: #a7f3d0; border-radius: 50%; animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
	@media (prefers-reduced-motion: reduce) {
		.spinner { animation: none; border-top-color: #a7f3d0; }
		.waveform span { transition: none; }
	}
</style>
