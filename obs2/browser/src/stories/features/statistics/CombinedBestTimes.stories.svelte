<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import CombinedBestTimes from '$lib/features/statistics/CombinedBestTimes.svelte';
	import { statisticsFixture } from './statisticsFixtures';
	const { Story } = defineMeta({
		title: 'Statistics/Combined best',
		component: CombinedBestTimes,
		args: { value: statisticsFixture.summary.combinedBestTimes }
	});
</script>

<Story name="Partial coverage" />
<Story
	name="Empty catalog"
	args={{
		value: {
			overallSeconds: 61380,
			recordedCells: 0,
			totalCells: 60,
			byDifficulty: [
				{ difficultyNumber: 0, totalSeconds: 20460, recordedLevels: 0, totalLevels: 20 },
				{ difficultyNumber: 1, totalSeconds: 20460, recordedLevels: 0, totalLevels: 20 },
				{ difficultyNumber: 2, totalSeconds: 20460, recordedLevels: 0, totalLevels: 20 }
			]
		}
	}}
/>
