<script lang="ts">
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import { Button } from '@epicenter/ui/button';
	import { meetingTranscription as api } from '$lib/query/meeting-transcription';
	import {
		meetingProviderConsentKey,
		meetingProviderLabel,
	} from '$lib/query/meeting-provider';
	import { formatMeetingOffset } from '$lib/services/meetings/library';
	let { id }: { id: string } = $props();
	const latest = createQuery(() => ({
		...api.latest(id).options,
		refetchInterval: 3000,
	}));
	const progress = createQuery(() => ({
		...api.progress.options,
		refetchInterval: 500,
	}));
	const run = createMutation(() => api.run.options);
	const pause = createMutation(() => api.pause.options);
	const save = createMutation(() => api.save.options);
	let confirmed = $state(false);
	const consentKey = $derived(meetingProviderConsentKey());
	$effect(() => {
		const key = consentKey;
		const source = id;
		confirmed = false;
	});
	const active = $derived(progress.data?.active === true);
	const busy = $derived(active || run.isPending);
	const pendingSave = $derived(progress.data?.pendingText != null);
</script>

<section
	class="space-y-3 border-border rounded-md border p-3"
	aria-label="Audio-only meeting transcription"
>
	<h3 class="font-semibold">Audio-only transcription</h3>
	<p>
		Uses your currently selected provider ({meetingProviderLabel()}) and
		model/language/prompt settings, pinned for this operation. Cloud/self-hosted
		providers receive only one minute of mixed WAV audio at a time (at most 5.76
		MB). Screenshots, meeting directories and manifests are never sent. No
		clipboard, paste, Enter or LLM cleanup.
	</p>
	<p>
		Local providers require an already configured/downloaded model. Failed
		requests retain all audio and saved text. Pausing waits for the current
		request and local save; it does not abort a provider request. Keep the app
		running; after a reload/restart explicitly resume saved chunks. A response
		lost before local saving may require retranscription of that minute.
	</p>
	<label class="flex gap-2"
		><input type="checkbox" bind:checked={confirmed} disabled={busy} />
		I explicitly authorize this audio-only transcription using the selected provider.
		This meeting may be incomplete; participant/privacy requirements are my responsibility.</label
	>
	<Button
		disabled={!confirmed ||
			busy ||
			latest.isPending ||
			latest.isError ||
			pendingSave}
		onclick={() =>
			run.mutate(
				{ id, resume: false, confirmed },
				{
					onSuccess: () => {
						confirmed = false;
					},
				},
			)}>Start new transcript</Button
	>
	<Button
		variant="outline"
		disabled={!confirmed || busy || !latest.data || pendingSave}
		onclick={() =>
			run.mutate(
				{ id, resume: true, confirmed },
				{
					onSuccess: () => {
						confirmed = false;
					},
				},
			)}>Resume saved transcript</Button
	>
	{#if active}<Button variant="outline" onclick={() => pause.mutate()}
			>Pause after current chunk</Button
		>
		<p role="status">
			Transcribing {progress.data?.meetingId}: {progress.data
				?.completed}/{progress.data?.total} chunks saved.
		</p>{/if}
	{#each [latest.error, run.error, save.error].filter(Boolean) as error}<p
			role="alert"
		>
			{error?.title}: {error?.description}
		</p>{/each}
	{#if progress.data?.pendingText !== null && progress.data?.pendingText !== undefined}
		<p role="alert">
			Provider text for {progress.data?.meetingId} is waiting for local save. Retry
			saving without another provider request; this text exists in memory only until
			saved.
		</p>
		<Button disabled={active || save.isPending} onclick={() => save.mutate()}
			>Retry local save only</Button
		>
		<pre class="whitespace-pre-wrap">{progress.data.pendingText}</pre>
	{/if}
	{#if latest.data}
		<h4 class="font-semibold">
			Saved transcript · {latest.data.providerLabel} · {latest.data.segments
				.length}/{latest.data.audio.length} chunks
		</h4>
		{#each latest.data.audio as audio}
			{@const segment = latest.data.segments.find(
				(s) => s.index === audio.index,
			)}
			{#if segment}<p class="text-sm text-muted-foreground">
					{formatMeetingOffset(audio.offsetMs)}
				</p>
				<pre class="whitespace-pre-wrap">{segment.text}</pre>{/if}
		{/each}
	{/if}
</section>
