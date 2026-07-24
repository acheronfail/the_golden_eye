<script module lang="ts">
	export interface StatisticsSummaryItem {
		label: string;
		value: string;
		detail?: string;
	}
</script>

<script lang="ts">
	import SectionTitle from './SectionTitle.svelte';

	let {
		title,
		subtitle,
		summaryValue,
		summaryDetail,
		items,
		columns = 4
	}: {
		title: string;
		subtitle?: string;
		summaryValue?: string;
		summaryDetail?: string;
		items: StatisticsSummaryItem[];
		columns?: 3 | 4;
	} = $props();

	const gridClass = $derived(columns === 3 ? 'grid-cols-1 sm:grid-cols-3' : 'grid-cols-2 sm:grid-cols-4');
</script>

{#snippet summary()}
	<div class="text-right">
		<p class="font-mono text-xl font-bold text-(--obs-text)">{summaryValue}</p>
		{#if summaryDetail}
			<p class="text-xs obs-muted">{summaryDetail}</p>
		{/if}
	</div>
{/snippet}

<section class="rounded-sm obs-panel p-4" aria-label={title}>
	{#if summaryValue}
		<SectionTitle {title} actions={summary} />
	{:else}
		<SectionTitle {title} />
	{/if}
	{#if subtitle}
		<p class="mt-2 text-xs obs-dim">{subtitle}</p>
	{/if}
	<div class="mt-4 grid gap-2 {gridClass}">
		{#each items as item}
			<div class="rounded-sm border border-(--obs-border-soft) bg-(--obs-bg) p-3 text-center sm:text-left">
				<p class="text-xs font-semibold obs-muted">{item.label}</p>
				<p class="mt-1 font-mono text-base font-bold">{item.value}</p>
				{#if item.detail}
					<p class="mt-1 text-xs obs-dim">{item.detail}</p>
				{/if}
			</div>
		{/each}
	</div>
</section>
