<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import StatisticsSummaryPanel from '$lib/features/statistics/StatisticsSummaryPanel.svelte';

	const { Story } = defineMeta({
		title: 'Statistics/Summary panel',
		component: StatisticsSummaryPanel,
		args: {
			title: 'General statistics',
			items: [
				{ label: 'Attempts', value: '76' },
				{ label: 'Total session time', value: '2:14:09' },
				{ label: 'Most played', value: 'Dam' },
				{ label: 'Overall combined time', value: '16:16:05' }
			]
		}
	});
</script>

<Story name="Four metrics" />
<Story
	name="Three metrics with summary"
	args={{
		title: 'Combined best',
		subtitle: 'All-time best completed run for every level',
		summaryValue: '16:16:05',
		summaryDetail: '3/60 times recorded',
		columns: 3,
		items: [
			{ label: 'Agent', value: '5:24:25', detail: '1/20 levels' },
			{ label: 'Secret Agent', value: '5:10:40', detail: '2/20 levels' },
			{ label: '00 Agent', value: '5:41:00', detail: '0/20 levels' }
		]
	}}
/>
