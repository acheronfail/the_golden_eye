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
		<button type="button" class="obs-text-button cursor-pointer rounded-full obs-button-xs" aria-label="Section help">
			?
		</button>
	{/if}
{/snippet}

<main class="mx-auto w-full max-w-2xl px-6 py-8">
	<SectionTitle {title} {detail} {actions} />
	<div class="mt-3 h-24 rounded obs-panel"></div>
</main>
