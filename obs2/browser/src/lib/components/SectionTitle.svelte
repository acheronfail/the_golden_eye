<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		actions,
		class: className = ''
	}: {
		title: string;
		actions?: Snippet;
		class?: string;
	} = $props();
</script>

<div class="flex items-center justify-between gap-3 border-b border-(--obs-border-muted) pb-2 {className}">
	<h3 class="font-mono text-xs font-semibold tracking-[0.2em] text-(--obs-text-muted) uppercase">{title}</h3>
	{#if actions}
		{@render actions()}
	{/if}
</div>
