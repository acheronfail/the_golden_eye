<script lang="ts">
	import ModalDialog from '$lib/components/ModalDialog.svelte';

	let {
		busy = false,
		error = null,
		cancel,
		reset
	}: {
		busy?: boolean;
		error?: string | null;
		cancel: () => void;
		reset: () => void | Promise<void>;
	} = $props();
	let cancelButton = $state<HTMLButtonElement>();

	$effect(() => {
		queueMicrotask(() => cancelButton?.focus());

		const onKeydown = (event: KeyboardEvent) => {
			if (event.key === 'Escape' && !busy) cancel();
		};
		window.addEventListener('keydown', onKeydown);
		return () => window.removeEventListener('keydown', onKeydown);
	});
</script>

<ModalDialog id="reset-settings-dialog" title="Reset settings?">
	<p>
		This will permanently remove your changes, including saved secrets such as your Discord webhook URL. This action
		cannot be undone.
	</p>
	{#if error}
		<p class="text-xs text-(--obs-danger)">{error}</p>
	{/if}

	{#snippet actions()}
		<button bind:this={cancelButton} type="button" class="obs-button px-4 py-2" disabled={busy} onclick={cancel}>
			Cancel
		</button>
		<button type="button" class="obs-button obs-button-danger px-4 py-2" disabled={busy} onclick={reset}>
			{busy ? 'Resetting...' : 'Reset to defaults'}
		</button>
	{/snippet}
</ModalDialog>
