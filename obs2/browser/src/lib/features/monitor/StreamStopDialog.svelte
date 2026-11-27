<script lang="ts">
	import ModalDialog from '$lib/ui/ModalDialog.svelte';

	let { onClose, stopStream }: { onClose: () => void; stopStream: () => Promise<void> } = $props();
	let keepStreamingButton = $state<HTMLButtonElement>();
	let busy = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		keepStreamingButton?.focus();
	});

	async function confirmStop() {
		if (busy) return;
		busy = true;
		error = null;
		try {
			await stopStream();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			busy = false;
			return;
		}
		onClose();
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && !busy) {
			event.preventDefault();
			onClose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<ModalDialog id="stream-stop-dialog" title="Stop streaming too?">
	<p>Monitoring has stopped. Do you want to end the active OBS stream?</p>
	{#if error}
		<p role="alert" class="text-xs text-(--obs-danger)">{error}</p>
	{/if}

	{#snippet actions()}
		<button
			bind:this={keepStreamingButton}
			type="button"
			class="obs-button px-4 py-2"
			disabled={busy}
			onclick={onClose}
		>
			Keep streaming
		</button>
		<button type="button" class="obs-button obs-button-danger px-4 py-2" disabled={busy} onclick={confirmStop}>
			{busy ? 'Stopping…' : 'Stop streaming'}
		</button>
	{/snippet}
</ModalDialog>
