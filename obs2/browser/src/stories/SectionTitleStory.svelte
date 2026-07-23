<script lang="ts">
	import SectionTitle from '$lib/components/SectionTitle.svelte';

	let {
		title = 'Metadata',
		detail,
		withAction = false
	}: { title?: string; detail?: string; withAction?: boolean } = $props();
</script>

{#snippet actions()}
	{#if withAction}
		<button type="button" class="obs-text-button obs-button-xs cursor-pointer rounded-full" aria-label="Section help">
			?
		</button>
	{/if}
{/snippet}

<main class="mx-auto w-full max-w-2xl px-6 py-8">
	<SectionTitle {title} {detail} {actions} />
	<div class="obs-panel mt-3 h-24 rounded"></div>
</main>
