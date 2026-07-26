<script lang="ts">
	import ModalDialog from '$lib/ui/ModalDialog.svelte';

	let { cancel, read }: { cancel: () => void; read: () => void } = $props();
	let cancelButton = $state<HTMLButtonElement>();

	$effect(() => {
		queueMicrotask(() => cancelButton?.focus());

		const onKeydown = (event: KeyboardEvent) => {
			if (event.key === 'Escape') cancel();
		};
		window.addEventListener('keydown', onKeydown);
		return () => window.removeEventListener('keydown', onKeydown);
	});
</script>

<ModalDialog id="read-clips-dialog" title="Read clips?">
	<p>
		This scans your configured clips folder and reads GoldenEye metadata from its videos. New tagged clips are added to
		Runs, and clips that are no longer present remain as history-only entries.
	</p>
	<p class="obs-dim">The scan will not delete any video files. It may take a while for a large clips folder.</p>

	{#snippet actions()}
		<button bind:this={cancelButton} type="button" class="obs-button px-4 py-2" onclick={cancel}>Cancel</button>
		<button type="button" class="obs-button obs-button-gold px-4 py-2" onclick={read}>Read clips</button>
	{/snippet}
</ModalDialog>
