<script lang="ts">
	import { createMutation, createQuery } from '@tanstack/svelte-query';
	import { Button } from '@epicenter/ui/button';
	import { meetingRecovery } from '$lib/query/meeting-recovery';
	import { formatMeetingOffset } from '$lib/services/meetings/library';
	import RevealMeetingFolder from './RevealMeetingFolder.svelte';
	let { id }: { id: string } = $props();
	const status = createQuery(() => ({
		...meetingRecovery.status.options,
		refetchInterval: 1000,
	}));
	const prepare = createMutation(() => meetingRecovery.prepare.options);
	const execute = createMutation(() => meetingRecovery.execute.options);
	const cancel = createMutation(() => meetingRecovery.cancel.options);
	const own = $derived(status.data?.sourceId === id ? status.data : null);
	const busy = $derived(
		prepare.isPending ||
			execute.isPending ||
			cancel.isPending ||
			status.data?.phase === 'preparing' ||
			status.data?.phase === 'running',
	);
	let confirmed = $state(false);
	const reviewToken = $derived(own?.token);
	$effect(() => {
		const identity = reviewToken;
		const source = id;
		confirmed = false;
	});
</script>

<section
	class="space-y-2 border-border border rounded-md p-3"
	aria-label="Recover unfinished audio"
>
	<h3 class="font-semibold">Recover unfinished audio</h3>
	<p>
		Creates a separate incomplete audio copy. Originals and screenshots remain
		in the original folder. Stop dictation/VAD first. Review does not record or
		upload anything; closing this page does not cancel running work.
	</p>
	<Button
		variant="outline"
		disabled={busy || status.isError || status.isPending}
		onclick={() => prepare.mutate(id)}>Review recovery</Button
	>
	{#if status.error}<p role="alert">{status.error.description}</p>{/if}
	{#each [prepare.error, execute.error, cancel.error].filter(Boolean) as error}
		<p role="alert">
			{error?.description} No automatic retry is made. Check native status below.
		</p>
	{/each}
	{#if status.data && !own && ['preparing', 'running'].includes(status.data.phase)}
		<p role="status">
			Recovery for {status.data.sourceId} is {status.data.phase}.
		</p>
	{/if}
	{#if own}
		<p role="status">Native recovery: {own.phase}</p>
		{#if own.error}<p role="alert">
				{own.error} Original and any partial files remain preserved. A new review
				is required to try again.
			</p>{/if}
		{#if own.preview}
			<p>
				{formatMeetingOffset(own.preview.durationMs)} · {own.preview
					.microphoneChunks} microphone chunks · {own.preview.systemChunks} system
				chunks. Approximately {(
					own.preview.additionalBytesEstimate / 1e9
				).toFixed(2)} GB additional space.
			</p>
			<p>
				{own.preview.unregisteredChunks} orphan chunks · {own.preview
					.emptyChunks} empty chunks · {own.preview.normalizedHeaders} headers to
				normalize ·
				{own.preview.discardedPartialSampleBytes} incomplete sample bytes · {own
					.preview.durationMismatches} duration mismatches.
			</p>
			<p class="text-sm text-muted-foreground">
				Supported: native mono 48kHz PCM16 minute chunks, up to 24 hours /
				10,000 audio-folder entries. Missing or ambiguous data requires manual
				review. Recovery is not proof that all meeting audio was captured.
			</p>
		{/if}
		{#if own.phase === 'ready'}
			<label class="flex items-start gap-2"
				><input type="checkbox" bind:checked={confirmed} disabled={busy} />
				I confirm creating this incomplete audio-only copy with the space estimate
				above. Keep all original files and screenshots.</label
			>
			<Button
				disabled={!confirmed || busy || status.isError}
				onclick={() =>
					execute.mutate(
						{ token: own!.token, confirmed },
						{
							onSuccess: () => {
								confirmed = false;
							},
						},
					)}>Create recovery copy</Button
			>
			<Button
				variant="outline"
				disabled={busy}
				onclick={() => cancel.mutate(own!.token)}>Cancel review</Button
			>
		{/if}
		{#if own.phase === 'completed' && own.derivedId}
			<p>
				Recovery copy created: {own.derivedId}. It remains marked incomplete.
				Select it in the library to play or explicitly transcribe audio.
			</p>
			<RevealMeetingFolder id={own.derivedId} />
		{/if}
	{/if}
</section>
