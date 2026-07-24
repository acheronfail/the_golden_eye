<script lang="ts">
	import type { RunCatalogSync } from '$lib/api';
	import ModalDialog from '$lib/components/ModalDialog.svelte';

	let { sync }: { sync: RunCatalogSync } = $props();

	const initial = $derived(sync === 'initial');
</script>

<ModalDialog
	id="run-catalog-sync-dialog"
	title={initial ? 'Building your runs library' : 'Resyncing your runs library'}
	bodyClass="grid gap-4 px-4 py-5 text-sm leading-6"
>
	<div class="flex items-center gap-3" role="status" aria-live="polite">
		<div
			class="size-5 shrink-0 animate-spin rounded-full border-2 border-(--obs-border-muted) border-t-(--obs-gold)"
			aria-hidden="true"
		></div>
		<p>
			{initial
				? 'Scanning your clips folders and reading clip details to build the run catalog.'
				: 'Scanning your clips folders and updating the run catalog.'}
		</p>
	</div>
	<p class="obs-dim">
		{initial
			? 'This is normally only needed once. It may take a while if you already have many clips.'
			: 'This can take a while if your clips folders contain many videos.'}
	</p>

	{#snippet actions()}
		<p class="mr-auto font-mono text-xs obs-dim">Please keep OBS open</p>
	{/snippet}
</ModalDialog>
