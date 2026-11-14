<script lang="ts">
	import { createMutation } from '@tanstack/svelte-query';
	import { Button } from '@epicenter/ui/button';
	import { Ok, tryAsync } from 'wellcrafted/result';
	import { tick } from 'svelte';
	import { meetingLibrary } from '$lib/query/meeting-library';
	import {
		createLocalPreviewUrl,
		formatMeetingOffset,
	} from '$lib/services/meetings/library';
	let {
		id,
		chunks,
	}: { id: string; chunks: { index: number; offsetMs: number }[] } = $props();
	const load = createMutation(() => meetingLibrary.preview.options);
	let audio = $state<HTMLAudioElement>();
	let url = $state<string | null>(null);
	let position = $state(0);
	let continuing = $state(false);
	let loading = $state(false);
	function ownAudio(node: HTMLAudioElement) {
		return {
			destroy() {
				node.pause();
			},
		};
	}
	let notice = $state('');
	let disposed = false;
	let generation = 0;
	$effect(() => () => {
		disposed = true;
		generation++;
		audio?.pause();
	});
	$effect(() => {
		const data = load.data;
		if (!data || data.id !== id || data.kind !== 'audio') {
			url = null;
			return;
		}
		const resource = createLocalPreviewUrl(data);
		url = resource.url;
		return () => {
			audio?.pause();
			resource.close();
		};
	});
	function playAt(next: number) {
		const chunk = chunks[next];
		if (!chunk) return;
		audio?.pause();
		loading = true;
		const current = ++generation;
		position = next;
		continuing = true;
		notice = '';
		load.mutate(
			{ id, index: chunk.index, kind: 'audio' },
			{
				onSuccess: async (data) => {
					await tick();
					if (
						disposed ||
						current !== generation ||
						data.id !== id ||
						data.index !== chunk.index
					)
						return;
					loading = false;
					await tryAsync({
						try: async () => {
							await audio?.play();
						},
						catch: () => {
							notice =
								'Browser playback needs a gesture. Press Play in the audio controls to continue.';
							return Ok(undefined);
						},
					});
				},
				onError: () => {
					loading = false;
					continuing = false;
				},
			},
		);
	}
</script>

<section class="space-y-2" aria-label="Continuous local audio playback">
	<Button
		variant="outline"
		disabled={!chunks.length || load.isPending}
		onclick={() => playAt(0)}>Play meeting audio</Button
	>
	<Button
		variant="ghost"
		disabled={!continuing}
		onclick={() => {
			continuing = false;
			generation++;
			audio?.pause();
		}}>Stop playback</Button
	>
	<p class="text-sm text-muted-foreground">
		User-started playback continues through finalized mixed chunks. It never
		starts on browsing; screenshots are not part of playback.
	</p>
	{#if load.error}<p role="alert">{load.error.description}</p>{/if}
	{#if notice}<p role="status">{notice}</p>{/if}
	{#if url}
		<p>
			Chunk {position + 1}/{chunks.length} at {formatMeetingOffset(
				chunks[position]?.offsetMs ?? 0,
			)}
		</p>
		<audio
			bind:this={audio}
			use:ownAudio
			controls
			src={url}
			onended={() => {
				if (continuing && !loading && position + 1 < chunks.length)
					playAt(position + 1);
				else continuing = false;
			}}><track kind="captions" /></audio
		>
	{/if}
</section>
