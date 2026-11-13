<script lang="ts">
    import { createMutation } from '@tanstack/svelte-query';
    import { Button } from '@epicenter/ui/button';
    import { meetingLibrary } from '$lib/query/meeting-library';
    import { isLocalMeetingId } from '$lib/services/meetings/library';

    let { id }: { id: string } = $props();
    const reveal = createMutation(() => meetingLibrary.revealFolder.options);
</script>

<Button
    variant="outline"
    disabled={!isLocalMeetingId(id) || reveal.isPending}
    onclick={() => reveal.mutate(id)}
>{reveal.isPending ? 'Requesting Finder…' : 'Reveal in Finder'}</Button>
{#if reveal.error && reveal.variables === id}
    <p role="alert">{reveal.error.title}: {reveal.error.description}</p>
{:else if reveal.data?.id === id}
    <p class="text-sm text-muted-foreground" role="status">Folder reveal requested.</p>
{/if}
