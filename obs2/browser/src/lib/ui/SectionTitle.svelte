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

<div class="flex flex-wrap items-start justify-between gap-3 border-b border-(--obs-gold-muted) pb-2 {className}">
	<div class="flex min-w-32 flex-1 items-baseline gap-2">
		<h3 class="font-mono text-xs leading-relaxed font-semibold tracking-[0.2em] obs-heading uppercase">{title}</h3>
		{#if detail}
			<span class="shrink-0 font-mono text-[10px] text-(--obs-text-dim)">{detail}</span>
		{/if}
	</div>
	{#if actions}
		{@render actions()}
	{/if}
</div>
