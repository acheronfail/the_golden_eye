<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import StatisticsFilters from '$lib/features/statistics/StatisticsFilters.svelte';
	const { Story } = defineMeta({
		title: 'Statistics/Filters',
		component: StatisticsFilters,
		args: { levelNumber: 1, difficultyNumber: 0 }
	});
</script>

<Story name="Default" />
<Story name="Selected cohort" args={{ levelNumber: 7, difficultyNumber: 2 }} />
