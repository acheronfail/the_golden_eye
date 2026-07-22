<script lang="ts">
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

<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
	<div
		class="obs-dialog w-full max-w-md overflow-hidden rounded"
		role="dialog"
		aria-modal="true"
		aria-labelledby="reset-settings-dialog-title"
		aria-describedby="reset-settings-dialog-body"
	>
		<div class="obs-dialog-header px-4 py-3">
			<h2 id="reset-settings-dialog-title" class="obs-heading text-lg font-semibold">Reset settings?</h2>
		</div>
		<div id="reset-settings-dialog-body" class="grid gap-3 px-4 py-4 text-sm leading-6">
			<p>
				This will permanently remove your changes, including saved secrets such as your Discord webhook URL. This action
				cannot be undone.
			</p>
			{#if error}
				<p class="text-xs text-(--obs-danger)">{error}</p>
			{/if}
		</div>
		<div class="flex justify-end gap-2 px-4 pb-4">
			<button bind:this={cancelButton} type="button" class="obs-button px-4 py-2" disabled={busy} onclick={cancel}>
				Cancel
			</button>
			<button type="button" class="obs-button obs-button-danger px-4 py-2" disabled={busy} onclick={reset}>
				{busy ? 'Resetting...' : 'Reset to defaults'}
			</button>
		</div>
	</div>
</div>
