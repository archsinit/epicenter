<script lang="ts">
	import { createMutation, createQuery } from '@tanstack/svelte-query';
	import { Button } from '@epicenter/ui/button';
	import { meetingLibrary } from '$lib/query/meeting-library';
	import { createLocalPreviewUrl, formatMeetingOffset } from '$lib/services/meetings/library';

	import RevealMeetingFolder from './RevealMeetingFolder.svelte';
	import RecoverMeeting from './RecoverMeeting.svelte';
	import TranscribeMeeting from './TranscribeMeeting.svelte';
	import MeetingPlayback from './MeetingPlayback.svelte';

	let selectedId = $state<string | null>(null);
	let cursor=$state<string|null>(null);
	let previousPages=$state<(string|null)[]>([]);
	const library = createQuery(() => meetingLibrary.page(cursor).options);
	const detail = createQuery(() => ({
		...meetingLibrary.detail(selectedId ?? '').options,
		enabled: selectedId !== null,
	}));
	const preview = createMutation(() => meetingLibrary.preview.options);
	let previewUrl = $state<string | null>(null);
	$effect(() => {
		const data = preview.data;
		if (!data || data.id !== selectedId) { previewUrl = null; return; }
		const resource = createLocalPreviewUrl(data);
		previewUrl = resource.url;
		return () => resource.close();
	});
</script>

<section class="space-y-4" aria-label="Local meeting library">
	<header class="space-y-2">
		<h1 class="text-2xl font-semibold">Meetings</h1>
		<p class="text-muted-foreground">
			Saved local meetings with explicit incomplete-audio recovery. Audio-only transcription is separate. Browsing this library does not start recording.
			Screenshots stay local and are never sent to a transcription provider or LLM.
		</p>
		<p class="text-sm text-muted-foreground">Retained source plus mixed WAVs use approximately 1 GB/hour for dual-source meetings or 0.7 GB/hour for microphone-only, excluding screenshots.</p>
		<p class="text-sm text-muted-foreground">Reveal selects a local folder in Finder, including folders with damaged metadata. Do not move, edit or delete files while recording or finalizing.</p>
		<Button variant="outline" disabled={library.isFetching} onclick={() => library.refetch()}>
			{library.isFetching ? 'Refreshing…' : 'Refresh local meetings'}
		</Button>
	</header>

	{#if library.error}
		<p role="alert">{library.error.title}: {library.error.description}</p>
	{:else if library.isPending}
		<p>Reading local metadata…</p>
	{:else if library.data}
		{#if library.data.truncated}
			<p role="status">More folders are available on the next page (500 per page). Pages use stable folder-ID order; dates are sorted within each page. Refresh from the first page to see newly added folders.</p>
		{/if}
		{#if library.data.entries.length === 0}
			<p>No saved meetings yet. No recording or permission request was started.</p>
		{/if}
		<Button variant="outline" disabled={!previousPages.length||library.isFetching} onclick={()=>{cursor=previousPages[previousPages.length-1]??null;previousPages=previousPages.slice(0,-1);selectedId=null;preview.reset();}}>Previous page</Button>
		<Button variant="outline" disabled={!library.data.nextCursor||library.isFetching} onclick={()=>{previousPages=[...previousPages,cursor];cursor=library.data?.nextCursor??null;selectedId=null;preview.reset();}}>Next page</Button>
		<Button variant="ghost" disabled={cursor===null} onclick={()=>{cursor=null;previousPages=[];selectedId=null;preview.reset();}}>First page</Button>
		<ul class="space-y-3">
			{#each library.data.entries as entry}
				<li class="border-border rounded-md border p-3 space-y-1">
					<Button
						variant="ghost"
						disabled={entry.state === 'unavailable'}
						onclick={() => { selectedId = entry.id; preview.reset(); }}
					>{entry.title || entry.id}</Button>
					<p class="text-sm text-muted-foreground">
						{entry.state} · {entry.audioChunks} registered audio chunks · {entry.screenshots} screenshots
						{#if entry.durationMs !== null} · {formatMeetingOffset(entry.durationMs)}{/if}
					</p>
					{#if entry.error}<p role="status">{entry.error}</p>{/if}
					{#key entry.id}
						<RevealMeetingFolder id={entry.id} />
					{/key}
				</li>
			{/each}
		</ul>
	{/if}

	{#if selectedId}
		<section class="space-y-3 border-border border-t pt-4" aria-label="Selected meeting">
			{#if detail.isPending}<p>Reading meeting details…</p>
			{:else if detail.error}<p role="alert">{detail.error.title}: {detail.error.description}</p>
			{:else if detail.data}
				<h2 class="text-xl font-semibold">{detail.data.manifest.title || selectedId}</h2>
				<p class="text-sm text-muted-foreground">
					{detail.data.state} · {new Date(detail.data.manifest.startedAtUnixMs).toLocaleString()}
					· {detail.data.manifest.mode}
				</p>
				{#if detail.data.state === 'unfinished' || detail.data.state === 'interrupted'}
					<p>Local files are retained. This session is not silently resumed or uploaded.
						Finalized mixed chunks may be previewed. Unfinished or unregistered audio still needs an explicit recovery step.</p>
				{/if}
				{#if detail.data.unregisteredAssets}
					<p role="status">{detail.data.unregisteredAssets} unregistered audio/image files remain preserved for recovery.</p>
				{/if}
				{#if detail.data.state === 'unfinished' || detail.data.state === 'interrupted'}
					{#key selectedId}<RecoverMeeting id={selectedId} />{/key}
				{/if}
				{#if (detail.data.state === 'completed' || detail.data.state === 'interrupted') && detail.data.manifest.audio.some(a => a.track === 'mixed')}
					{#key selectedId}<TranscribeMeeting id={selectedId} />{/key}
				{/if}
				{#if detail.data.state === 'completed' || detail.data.state === 'interrupted'}
					{#key selectedId}<MeetingPlayback id={selectedId} chunks={detail.data.manifest.audio.flatMap((a,index)=>a.track==='mixed'?[{index,offsetMs:a.offsetMs}]:[])} />{/key}
				{/if}
				<h3 class="font-semibold">Mixed audio chunks</h3>
				<p class="text-sm text-muted-foreground">Raw microphone/system tracks remain local for recovery. Only finalized mixed chunks are available here.</p>
				<ul class="space-y-2">
					{#each detail.data.manifest.audio as chunk, index}
						{#if chunk.track === 'mixed'}
							<li>
								<Button
									variant="outline"
									disabled={preview.isPending || detail.data.state === 'active' || detail.data.state === 'unfinished'}
									onclick={() => preview.mutate({ id: selectedId!, index, kind: 'audio' })}
								>Load audio at {formatMeetingOffset(chunk.offsetMs)} ({formatMeetingOffset(chunk.durationMs)})</Button>
							</li>
						{/if}
					{/each}
				</ul>
				<h3 class="font-semibold">Local screenshot references</h3>
				<ul class="space-y-2">
					{#each detail.data.manifest.screenshots as image, index}
						<li>
							<Button
								variant="outline" disabled={preview.isPending}
								onclick={() => preview.mutate({ id: selectedId!, index, kind: 'local-screenshot' })}
							>View {image.origin} screenshot at {formatMeetingOffset(image.offsetMs)}</Button>
						</li>
					{/each}
				</ul>
			{/if}
			{#if preview.error}<p role="alert">{preview.error.title}: {preview.error.description}</p>{/if}
			{#if previewUrl && preview.data?.id === selectedId}
				<Button variant="ghost" onclick={() => preview.reset()}>Close local preview</Button>
				{#if preview.data.kind === 'audio'}
					<audio controls src={previewUrl}><track kind="captions" /></audio>
				{:else}
					<img class="max-h-[70vh] max-w-full object-contain" src={previewUrl} alt="Local meeting screenshot reference" />
				{/if}
			{/if}
		</section>
	{/if}
</section>
