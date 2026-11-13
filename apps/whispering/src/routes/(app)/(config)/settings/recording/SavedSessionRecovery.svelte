<script lang="ts">
	import { createMutation, createQuery } from '@tanstack/svelte-query';
	import * as Alert from '@epicenter/ui/alert';
	import { Button } from '@epicenter/ui/button';
	import { captureRecovery } from '$lib/query/capture-recovery';

	const status = createQuery(() => captureRecovery.getStatus.options);
	const recover = createMutation(() => captureRecovery.recover.options);
</script>

{#if status.data?.blocked}
	<Alert.Root>
		<Alert.Title>Recording startup needs attention</Alert.Title>
		<Alert.Description class="space-y-3">
			<p>{status.data.reason}</p>
			<p>
				Stop the earlier recorder normally and independently verify it is no longer
				recording. If you cannot verify that, do not recover or start another capture.
				Recovery never signals a saved PID, deletes audio, or changes your recording settings.
			</p>
			{#if status.data.hasSavedSession}
				<p>
					Review recovery to archive the saved session metadata locally. Confirmation
					does not prove process exit. Capture stays blocked until you fully Quit and
					relaunch Whispering Intel; hiding or closing its window is not enough.
				</p>
				<Button
					variant="outline"
					disabled={recover.isPending}
					onclick={() => recover.mutate()}
				>
					{recover.isPending ? 'Waiting for confirmation…' : 'Review saved-session recovery…'}
				</Button>
			{:else}
				<p>
					No saved session metadata remains. Capture is still blocked in this process.
					After verifying the earlier recorder has stopped, use the menu bar’s Quit
					command, then relaunch Whispering Intel.
				</p>
			{/if}
			{#if recover.data?.status === 'restart-required'}
				<p class="break-all">Metadata archived locally: {recover.data.archivePath}</p>
			{/if}
			{#if recover.error}
				<p role="alert">{recover.error.title}: {recover.error.description}</p>
			{/if}
		</Alert.Description>
	</Alert.Root>
{:else if status.error}
	<p role="alert">{status.error.title}: {status.error.description}</p>
{/if}
