<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { backend } from '$lib/api';
	import DateRangeSelect from '$lib/features/statistics/DateRangeSelect.svelte';
	import SectionTitle from '$lib/ui/SectionTitle.svelte';
	import StatisticsDashboard from '$lib/features/statistics/StatisticsDashboard.svelte';
	import { StatisticsPageController } from '$lib/features/statistics/statisticsPageController.svelte';
	import { onDestroy, onMount } from 'svelte';

	const controller = new StatisticsPageController(
		backend,
		{
			currentUrl: () => new URL(page.url),
			goto: (href, options) => void goto(href, options)
		},
		new URL(page.url)
	);

	onMount(() => controller.initialize(localStorage));
	onDestroy(() => controller.destroy());
</script>

<svelte:head><title>Statistics</title></svelte:head>

<main class="mx-auto w-full max-w-3xl px-4 obs-page-top pb-4 sm:px-6 sm:pb-6">
	<header class="mb-4">
		<h1 class="text-2xl font-semibold obs-heading">Statistics</h1>
	</header>

	<StatisticsDashboard
		data={controller.data}
		loading={controller.loading}
		error={controller.error}
		bind:levelNumber={controller.levelNumber}
		bind:difficultyNumber={controller.difficultyNumber}
		bind:tab={controller.tab}
		bucket={controller.bucket}
		sessions={controller.sessions}
		bind:selectedSessionId={controller.selectedSessionId}
		sessionDetail={controller.sessionDetail}
		sessionLoading={controller.sessionLoading}
		bind:levelDifficulties={controller.levelDifficulties}
		bind:attemptsOverTimeStatuses={controller.attemptsOverTimeStatuses}
		bind:improvementSeries={controller.improvementSeries}
		bind:outcomeStatuses={controller.outcomeStatuses}
		bind:sessionStatuses={controller.sessionStatuses}
		bind:outcomeMeasure={controller.outcomeMeasure}
		bind:levelMeasure={controller.levelMeasure}
		bind:levelOrder={controller.levelOrder}
	>
		{#snippet controls()}
			<section>
				<SectionTitle title="Filters" class="mb-3" />
				<DateRangeSelect
					bind:value={controller.range}
					bind:bucket={controller.bucket}
					showGroupBy={controller.tab !== 'improvement'}
					error={controller.resolvedRange.error ?? null}
				/>
			</section>
		{/snippet}
	</StatisticsDashboard>
</main>
