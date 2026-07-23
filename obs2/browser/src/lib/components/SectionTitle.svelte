<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		detail,
		actions,
		class: className = ''
	}: {
		title: string;
		detail?: string;
		actions?: Snippet;
		class?: string;
	} = $props();
</script>

<div class="flex items-center justify-between gap-3 border-b border-(--obs-border-muted) pb-2 {className}">
	<div class="flex min-w-0 items-baseline gap-2">
		<h3 class="truncate font-mono text-xs font-semibold tracking-[0.2em] text-(--obs-text-muted) uppercase">{title}</h3>
		{#if detail}
			<span class="shrink-0 font-mono text-[10px] text-(--obs-text-dim)">{detail}</span>
		{/if}
	</div>
	{#if actions}
		{@render actions()}
	{/if}
</div>
