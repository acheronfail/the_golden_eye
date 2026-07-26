<script lang="ts">
	import type { StatisticsResponse } from '$lib/api';
	import { difficultyLabel, formatDuration } from '$lib/features/statistics/statisticsView';
	import StatisticsSummaryPanel from './StatisticsSummaryPanel.svelte';

	let { value }: { value: StatisticsResponse['summary']['combinedBestTimes'] } = $props();

	const items = $derived(
		value.byDifficulty.map((difficulty) => ({
			label: difficultyLabel(difficulty.difficultyNumber),
			value: formatDuration(difficulty.totalSeconds),
			detail: `${difficulty.recordedLevels}/${difficulty.totalLevels} levels`
		}))
	);
</script>

<StatisticsSummaryPanel
	title="Combined best"
	subtitle="All-time best completed run for every level"
	summaryValue={formatDuration(value.overallSeconds)}
	summaryDetail={`${value.recordedCells}/${value.totalCells} times recorded`}
	{items}
	columns={3}
/>
