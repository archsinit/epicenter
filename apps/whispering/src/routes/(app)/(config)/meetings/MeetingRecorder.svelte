<script lang="ts">
	import { createMutation, createQuery } from '@tanstack/svelte-query';
	import { Button } from '@epicenter/ui/button';
	import { Input } from '@epicenter/ui/input';
	import { meetingControls } from '$lib/query/meeting-controls';
	import { rpc } from '$lib/query';
	import { vadRecorder } from '$lib/query/vad.svelte';
	import { meetingStartAvailability, type StartMeetingParams } from '$lib/services/meetings/controls';
	import { formatMeetingOffset } from '$lib/services/meetings/library';

	const status = createQuery(() => ({
		...meetingControls.status.options,
		refetchInterval: 1000,
		refetchIntervalInBackground: false,
	}));
	const dictation = createQuery(() => rpc.recorder.getRecorderState.options);
	const sources = createMutation(() => meetingControls.loadSources.options);
	const permission = createMutation(() => meetingControls.permission.options);
	const start = createMutation(() => meetingControls.start.options);
	const stop = createMutation(() => meetingControls.stop.options);
	const capture = createMutation(() => meetingControls.captureNow.options);
	const pause = createMutation(() => meetingControls.pauseScreenshots.options);
	let title = $state('Meeting');
	let mode = $state<StartMeetingParams['mode']>('microphone');
	let microphoneId = $state('');
	let displayChoice = $state('');
	let confirmed = $state(false);
	let microphonePermission = $state<boolean | null>(null);
	let screenPermission = $state<boolean | null>(null);
	$effect(() => { title; mode; microphoneId; displayChoice; confirmed = false; });
	const params = $derived<StartMeetingParams>({
		title, mode, microphoneId,
		displayId: displayChoice === '' ? null : Number(displayChoice),
		confirmed,
	});
	const progress = $derived(status.data?.progress);
	const reserved = $derived(progress?.captureReserved ?? false);
	const unknownCleanup = $derived(Boolean(progress?.captureReserved && ['completed', 'interrupted', 'failed'].includes(progress.phase)));
	const pendingStop = $derived(stop.isPending || Boolean(stop.data?.progress?.id === progress?.id && reserved && stop.isSuccess));
	const locked = $derived(reserved || start.isPending || stop.isPending || permission.isPending);
	const ordinaryActive = $derived(vadRecorder.state !== 'IDLE' || dictation.data === 'RECORDING');
	const baseStartReason = $derived(status.isError
		? 'Native status is unavailable. Request Stop or fully Quit/relaunch; do not assume capture is idle.'
		: dictation.isError
			? 'Cannot verify ordinary dictation state. Stop/Cancel its input first.'
			: ordinaryActive
				? 'Stop dictation/VAD and its pending cleanup before starting a meeting. Your settings are not changed.'
				: meetingStartAvailability(params, sources.data, status.data));
	const startReason = $derived(baseStartReason ?? (microphonePermission !== true
		? 'Use the explicit microphone permission check before Start.'
		: params.displayId !== null && screenPermission !== true
			? 'Use the explicit screen permission check for the selected display before Start.' : null));
</script>

<section class="space-y-4 border-border rounded-md border p-4" aria-label="Meeting recorder">
	<h1 class="text-2xl font-semibold">Meeting recorder</h1>
	<p>macOS 14+. A deliberate Start records the selected inputs locally. Closing or hiding this window
		does not stop capture or reset screenshot timing. Use Stop to finalize; Quit finalizes or asks
		before leaving incomplete files. Transcription/recovery are separate and still under development.</p>
	<Button href="/settings/recording" variant="ghost">Ordinary recorder settings / saved-session recovery</Button>
	{#if status.error}
		<p role="alert">{status.error.title}: {status.error.description}</p>
		<Button variant="outline" onclick={() => status.refetch()}>Refresh native status</Button>
	{/if}
	{#if status.data?.requiresRestart}
		<p role="alert">An earlier Start acknowledgement is unknown. Request Stop, then fully Quit and relaunch
			Whispering Intel before another recording. Closing/reloading the window is not proof of cleanup.</p>
	{/if}
	{#if progress}
		<p role="status" class="font-semibold">{progress.title} · {progress.phase} · {formatMeetingOffset(progress.elapsedMs)}</p>
		<p class="text-sm text-muted-foreground">
			{progress.mode} · microphone: {progress.microphoneId || 'System default'}
			· display: {progress.displayId ?? 'None — no screenshots'}
			· {progress.audioChunks} mixed / {progress.sourceChunks} source chunks · {progress.screenshots} saved screenshots
		</p>
		{#if progress.error}<p role="alert">{progress.error}</p>{/if}
		{#if progress.lastScreenshotError}<p role="alert">Screenshot failed; audio status is shown above: {progress.lastScreenshotError}</p>{/if}
	{/if}
	{#if unknownCleanup}
		<p role="alert">Native capture/finalization ownership is still reserved. Cleanup is not fully acknowledged; do not start
			dictation or another meeting. Local files were retained. Stop the original recorder normally
			where possible, then fully Quit/relaunch if cleanup remains unknown.</p>
	{/if}
	{#if reserved || start.isPending || status.isError || status.data?.requiresRestart}
		<Button
			disabled={stop.isPending || (pendingStop && !unknownCleanup)}
			onclick={() => stop.mutate()}
		>{stop.isPending ? 'Requesting Stop…' : 'Stop meeting and finalize'}</Button>
		{#if pendingStop}<p role="status">Stop requested. Waiting for native input closure and finalization; audio is not automatically transcribed.</p>{/if}
	{/if}
	{#if reserved && progress?.displayId !== null && progress?.phase === 'recording'}
		<div class="flex flex-wrap gap-2">
			<Button variant="outline" disabled={capture.isPending || progress.screenshotsPaused || pendingStop} onclick={() => capture.mutate()}>Capture screenshot now</Button>
			<Button variant="outline" disabled={pause.isPending || pendingStop} onclick={() => pause.mutate(!progress.screenshotsPaused)}>
				{progress.screenshotsPaused ? 'Resume screenshots' : 'Pause screenshots'}
			</Button>
		</div>
		<p>Automatic screenshots follow the original five-minute boundaries. Manual captures never reset
			that cadence. {progress.screenshotsPaused ? 'Screenshots are paused, including manual capture.' : 'Screenshot requests update the saved count only after native capture/persistence completes.'}</p>
	{/if}
	<fieldset class="space-y-3" disabled={locked}>
		<legend class="font-semibold">New meeting sources and consent</legend>
		<Button variant="outline" disabled={sources.isPending} onclick={() => sources.mutate()}>{sources.isPending ? 'Loading…' : 'Load / refresh sources (no recording)'}</Button>
		{#if sources.data && !sources.data.screenSupported}<p>Meeting mode requires macOS 14 or newer.</p>{/if}
		<label class="block">Meeting title <Input bind:value={title} maxlength={400} /></label>
		<label class="block">Audio sources
			<select class="border-border bg-background rounded-md border p-2" bind:value={mode}>
				<option value="microphone">In-person — microphone only</option>
				<option value="system-and-microphone">Online — system audio and microphone</option>
			</select>
		</label>
		<label class="block">Microphone
			<select class="border-border bg-background rounded-md border p-2" bind:value={microphoneId}>
				<option value="">System default microphone</option>
				{#each sources.data?.microphones ?? [] as mic}<option value={mic.id}>{mic.name}</option>{/each}
			</select>
		</label>
		<label class="block">Display for screenshots / system audio
			<select class="border-border bg-background rounded-md border p-2" bind:value={displayChoice}>
				<option value="">None — no screenshots (microphone-only mode)</option>
				{#each sources.data?.displays ?? [] as display}<option value={String(display.id)}>{display.main ? 'Main display' : 'Display'} {display.id} — {display.width}×{display.height}</option>{/each}
			</select>
		</label>
		<p class="text-sm text-muted-foreground">A selected display enables full-display PNG screenshots every five minutes, including in-person mode.
			Online mode requires a display for system audio. Use headphones to reduce acoustic duplication.
			Retained source + mixed WAVs use approximately 1 GB/hour online or 0.7 GB/hour microphone-only,
			excluding screenshots. There is no continuous video.</p>
		<div class="flex flex-wrap gap-2">
			<Button variant="outline" disabled={!sources.data?.screenSupported} onclick={() => { microphonePermission = null; permission.mutate('microphone', { onSuccess: (result) => { microphonePermission = result.granted; } }); }}>Grant / check microphone permission…</Button>
			<Button variant="outline" disabled={!sources.data?.screenSupported} onclick={() => { screenPermission = null; permission.mutate('screen', { onSuccess: (result) => { screenPermission = result.granted; } }); }}>Grant / check screen permission…</Button>
		</div>
		<p role="status">Microphone: {microphonePermission === null ? 'not checked' : microphonePermission ? 'granted' : 'not granted'} · Screen: {screenPermission === null ? 'not checked' : screenPermission ? 'granted' : 'not granted'}</p>
		<p>Permission buttons are explicit macOS actions, never triggered by page load or Start.
			Microphone-only without a display does not need screen permission.
			If denied, use normal macOS Settings and restart if requested; no restriction is bypassed.</p>
		<label class="flex items-start gap-2">
			<input type="checkbox" bind:checked={confirmed} />
			<span>I have informed participants and obtained required recording consent. I understand the selected
				inputs will be recorded and, if a display is selected, full-display screenshots will be saved locally.
				Screenshots are never sent to a transcription provider or LLM.</span>
		</label>
		{#if startReason}<p role="status">{startReason}</p>{/if}
		<Button disabled={Boolean(startReason) || start.isPending || stop.isPending || sources.isPending} onclick={() => start.mutate(params, { onSuccess: () => { confirmed = false; } })}>
			{start.isPending ? 'Requesting Start…' : 'Start meeting recording'}
		</Button>
	</fieldset>
	{#if permission.data}<p role="status">{permission.data.kind} permission: {permission.data.granted ? 'granted/available' : 'not granted — no permission was assumed'}. Refresh sources after changing devices/permissions.</p>{/if}
	{#each [sources.error, permission.error, start.error, stop.error, capture.error, pause.error] as error}
		{#if error}<p role="alert">{error.title}: {error.description}</p>{/if}
	{/each}
</section>
