<script lang="ts">
	import ModalDialog from '$lib/components/ModalDialog.svelte';

	let { dismiss }: { dismiss: () => void } = $props();
	let dismissButton = $state<HTMLButtonElement>();

	$effect(() => {
		queueMicrotask(() => dismissButton?.focus());
	});
</script>

<ModalDialog
	id="welcome-dialog"
	title="Welcome to The Golden Eye"
	bodyClass="flex flex-col gap-4 px-4 py-4 text-sm leading-6"
>
	<p>
		This plugin helps save clips from GoldenEye 007 speedruns by watching your capture and managing replay-buffer saves
		around runs.
	</p>
	<p class="rounded obs-alert-warning px-3 py-2 obs-alert-warning-body">
		Do not rely completely on this plugin as your only copy. You are strongly recommended to stream or record your
		gameplay somewhere reliable, such as YouTube or Twitch, so a missed detection or local recording issue does not cost
		you the run.
	</p>

	{#snippet actions()}
		<button bind:this={dismissButton} type="button" class="obs-button obs-button-gold px-4 py-2" onclick={dismiss}>
			I understand
		</button>
	{/snippet}
</ModalDialog>
