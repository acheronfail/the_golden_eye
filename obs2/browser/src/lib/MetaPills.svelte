<script lang="ts">
	import type { MetaPill } from './metaPills';

	let {
		chips,
		containerClass = '',
		pillClass = ''
	}: {
		chips: MetaPill[];
		containerClass?: string;
		pillClass?: string;
	} = $props();
</script>

<span class="flex min-w-0 flex-wrap gap-1 {containerClass}">
	{#each chips as chip}
		<span
			class="{chip.class ?? 'obs-token'} rounded border px-1.5 py-0.5 font-mono text-[10px] leading-tight {pillClass}"
			title={chip.title ?? chip.label}
		>
			{chip.label}
		</span>
	{/each}
</span>
