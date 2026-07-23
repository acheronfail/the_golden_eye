<script lang="ts">
	import ModalDialog from '$lib/components/ModalDialog.svelte';

	let {
		busy = false,
		error = null,
		choose
	}: {
		busy?: boolean;
		error?: string | null;
		choose: (stopReplayBuffer: boolean) => void | Promise<void>;
	} = $props();
	let keepRunningButton = $state<HTMLButtonElement>();

	$effect(() => {
		queueMicrotask(() => keepRunningButton?.focus());
	});
</script>

<ModalDialog id="replay-buffer-stop-dialog" title="Stop the replay buffer too?">
	<p>Would you like OBS's replay buffer to stop whenever you stop monitoring?</p>
	<p class="obs-dim">You can change this later in the plugin's Options.</p>
	{#if error}
		<p class="text-xs text-(--obs-danger)">{error}</p>
	{/if}

	{#snippet actions()}
		<button
			bind:this={keepRunningButton}
			type="button"
			class="obs-button px-4 py-2"
			disabled={busy}
			onclick={() => choose(false)}
		>
			Keep it running
		</button>
		<button type="button" class="obs-button obs-button-gold px-4 py-2" disabled={busy} onclick={() => choose(true)}>
			Stop replay buffer
		</button>
	{/snippet}
</ModalDialog>
