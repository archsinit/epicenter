<script lang="ts">
	import { onMount } from 'svelte';
	import { createMutation } from '@tanstack/svelte-query';
	import { Button } from '@epicenter/ui/button';
	import { commands } from '$lib/commands';
	import { rpc } from '$lib/query';
	import { mouseShortcuts, type MouseBinding } from '$lib/services/mouse-shortcuts';
	import { settings } from '$lib/stores/settings.svelte';

	let draft = $state<MouseBinding[]>([]);
	let active = $state(false);
	let statusKnown = $state(false);
	const save = createMutation(() => rpc.mouseShortcuts.configure.options);
	const buttons = Array.from({ length: 30 }, (_, index) => index + 2);
	const duplicates = $derived(new Set(draft.map((binding) => binding.button)).size !== draft.length);

	function buttonLabel(button: number) {
		return button === 2 ? 'Middle button (3)'
			: button === 3 ? 'Button 4 (usually Back)'
			: button === 4 ? 'Button 5 (usually Forward)' : `Button ${button + 1}`;
	}

	onMount(() => {
		draft = settings.value['shortcuts.mouse.bindings'].map((binding) => ({ ...binding }));
		let stopped = false;
		let timer: ReturnType<typeof setTimeout>;
		async function poll() {
			if (!window.__TAURI_INTERNALS__) return;
			const result = await mouseShortcuts.active();
			if (stopped) return;
			statusKnown = !result.error;
			active = result.data ?? false;
			timer = setTimeout(poll, 1000);
		}
		poll();
		return () => { stopped = true; clearTimeout(timer); };
	});
</script>

<section class="space-y-4">
	<h2 class="text-xl font-semibold">Mouse buttons</h2>
	<p class="text-sm text-muted-foreground">
		Optional system-wide shortcuts, alongside your existing keyboard shortcuts.
		Nothing is assigned by default. An assigned button's normal click, navigation
		and drag behaviour is suppressed while the app handles it. Primary and secondary
		clicks are reserved so your mouse remains usable.
	</p>
	<p class="text-sm text-muted-foreground">
		macOS may require Accessibility and Input Monitoring permission for Whispering Intel.
		After granting access, click Save and apply again. Vendor mouse software can remap
		buttons into keyboard events; use the keyboard shortcut settings for those.
	</p>

	{#if !window.__TAURI_INTERNALS__}
		<p>Mouse shortcuts require the macOS desktop app.</p>
	{:else}
		<p class="text-sm" role="status">
			{settings.value['shortcuts.mouse.bindings'].length === 0 ? 'No mouse buttons assigned.'
				: !statusKnown ? 'Mouse shortcut status unavailable.'
				: active ? 'Mouse button interception is active.'
				: 'Mouse shortcuts are inactive. Check permissions and retry.'}
		</p>
		{#each draft as binding, index (index)}
			<div class="flex flex-wrap items-center gap-3">
				<label class="space-y-1">
					<span class="block text-sm">Mouse button</span>
					<select class="rounded-md border bg-background p-2" bind:value={binding.button} disabled={save.isPending}>
						{#each buttons as button (button)}
							<option value={button}>{buttonLabel(button)}</option>
						{/each}
					</select>
				</label>
				<label class="space-y-1">
					<span class="block text-sm">Action</span>
					<select class="rounded-md border bg-background p-2" bind:value={binding.commandId} disabled={save.isPending}>
						{#each commands as command (command.id)}
							<option value={command.id}>{command.title}</option>
						{/each}
					</select>
				</label>
				<Button variant="outline" disabled={save.isPending} onclick={() => { draft = draft.filter((_, i) => i !== index); }}>Remove</Button>
			</div>
		{/each}
		{#if duplicates}<p class="text-sm text-destructive">Choose a different button for each action.</p>{/if}
		<div class="flex flex-wrap gap-3">
			<Button variant="outline" disabled={save.isPending || draft.length === buttons.length} onclick={() => {
				const available = buttons.find((button) => !draft.some((binding) => binding.button === button));
				if (available !== undefined) draft = [...draft, { button: available, commandId: 'toggleManualRecording' }];
			}}>Add mouse button</Button>
			<Button disabled={save.isPending || duplicates} onclick={() => save.mutate(draft.map((binding) => ({ ...binding })), {
				onSuccess: () => rpc.notify.success.execute({ title: 'Mouse shortcuts applied', description: 'Keyboard shortcuts are unchanged.' }),
				onError: (error) => rpc.notify.error.execute({ title: 'Mouse shortcuts could not be applied', description: String(error) }),
			})}>{save.isPending ? 'Applying…' : 'Save and apply'}</Button>
		</div>
	{/if}
</section>
