<script lang="ts">
	import { IS_MACOS } from '$lib/constants/platform';
	import MeetingLibrary from '$routes/(app)/(config)/meetings/MeetingLibrary.svelte';
</script>

<svelte:head><title>Meetings - Whispering Intel</title></svelte:head>

{#if window.__TAURI_INTERNALS__ && IS_MACOS}
	<MeetingLibrary />
{:else}
	<p>The local meeting library is available in Whispering Intel for Mac.</p>
{/if}
