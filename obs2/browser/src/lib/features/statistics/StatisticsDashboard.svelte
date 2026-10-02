<script lang="ts">
	import type { Snippet } from 'svelte';
	import type {
		DifficultyNumber,
		MonitoringSessionDetail,
		MonitoringSessionSummary,
		RunStatus,
		StatisticsBucket,
		StatisticsResponse
	} from '$lib/api';
	import Chart from '$lib/ui/Chart/Chart.svelte';
	import CombinedBestTimes from './CombinedBestTimes.svelte';
	import SectionTitle from '$lib/ui/SectionTitle.svelte';
	import SegmentedControl from '$lib/ui/SegmentedControl.svelte';
	import SessionStatistics from './SessionStatistics.svelte';
	import StatisticsSummaryPanel from './StatisticsSummaryPanel.svelte';
	import StatisticsFilters from './StatisticsFilters.svelte';
	import {
		ALL_STATUSES,
		attemptsByLevelData,
		formatDuration,
		mostPlayedLevel,
		outcomeData,
		overallAttemptsData,
		runTimeData
	} from '$lib/features/statistics/statisticsView';
	import type {
		StatisticsImprovementSeries,
		StatisticsLevelDifficulty,
		StatisticsLevelMeasure,
		StatisticsLevelOrder,
		StatisticsOutcomeMeasure,
		StatisticsTab
	} from '$lib/features/statistics/statisticsPreferences';

	let {
		data,
		loading,
		error,
		levelNumber = $bindable(),
		difficultyNumber = $bindable(),
		tab = $bindable(),
		bucket = 'week',
		sessions,
		selectedSessionId = $bindable(),
		sessionDetail,
		sessionLoading = false,
		levelDifficulties = $bindable([0, 1, 2]),
		attemptsOverTimeStatuses = $bindable([...ALL_STATUSES]),
		improvementSeries = $bindable(['running-best', 'complete']),
		outcomeStatuses = $bindable([...ALL_STATUSES]),
		sessionStatuses = $bindable([...ALL_STATUSES]),
		outcomeMeasure = $bindable('share'),
		levelMeasure = $bindable('attempts'),
		levelOrder = $bindable('attempts'),
		controls,
		times
	}: {
		data: StatisticsResponse | null;
		loading: boolean;
		error: string | null;
		levelNumber: number;
		difficultyNumber: DifficultyNumber;
		tab: StatisticsTab;
		bucket?: StatisticsBucket;
		sessions: MonitoringSessionSummary[];
		selectedSessionId: string;
		sessionDetail: MonitoringSessionDetail | null;
		sessionLoading?: boolean;
		levelDifficulties?: StatisticsLevelDifficulty[];
		attemptsOverTimeStatuses?: RunStatus[];
		improvementSeries?: StatisticsImprovementSeries[];
		outcomeStatuses?: RunStatus[];
		sessionStatuses?: RunStatus[];
		outcomeMeasure?: StatisticsOutcomeMeasure;
		levelMeasure?: StatisticsLevelMeasure;
		levelOrder?: StatisticsLevelOrder;
		controls?: Snippet;
		times?: Snippet;
	} = $props();

	let tabList = $state<HTMLDivElement>();

	function chooseTab(index: number) {
		const item = tabs[index];
		if (!item) return;
		tab = item.id;
		tabList?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[index]?.focus();
	}

	function onTabKeydown(event: KeyboardEvent, index: number) {
		let next: number | undefined;
		if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') next = (index - 1 + tabs.length) % tabs.length;
		else if (event.key === 'ArrowRight' || event.key === 'ArrowDown') next = (index + 1) % tabs.length;
		else if (event.key === 'Home') next = 0;
		else if (event.key === 'End') next = tabs.length - 1;
		if (next == null) return;
		event.preventDefault();
		chooseTab(next);
	}

	const tabs: Array<{ id: StatisticsTab; label: string }> = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'times', label: 'Times' },
		{ id: 'improvement', label: 'Improvement' },
		{ id: 'outcomes', label: 'Outcomes' },
		{ id: 'sessions', label: 'Sessions' }
	];
</script>

{#snippet levelOrderActions()}
	<div class="flex w-full flex-wrap justify-between gap-2">
		<SegmentedControl
			value={levelMeasure}
			options={[
				{ value: 'attempts', label: 'Attempts' },
				{ value: 'time', label: 'Time spent' }
			]}
			ariaLabel="Level measurement"
			onChange={(value) => (levelMeasure = value as StatisticsLevelMeasure)}
		/>
		<SegmentedControl
			value={levelOrder}
			options={[
				{ value: 'attempts', label: levelMeasure === 'attempts' ? 'Most attempted' : 'Most time' },
				{ value: 'mission', label: 'Mission order' }
			]}
			ariaLabel="Level chart order"
			onChange={(value) => (levelOrder = value as StatisticsLevelOrder)}
		/>
	</div>
{/snippet}

{#snippet outcomeMeasureActions()}
	<SegmentedControl
		value={outcomeMeasure}
		options={[
			{ value: 'share', label: 'Share' },
			{ value: 'count', label: 'Count' }
		]}
		ariaLabel="Outcome measurement"
		onChange={(value) => (outcomeMeasure = value as StatisticsOutcomeMeasure)}
	/>
{/snippet}

<div class="sticky top-0 z-20 -mx-4 bg-(--obs-bg) px-4 py-2 sm:-mx-6 sm:px-6">
	<div
		bind:this={tabList}
		class="flex w-full overflow-hidden rounded border border-(--obs-border)"
		role="tablist"
		aria-label="Statistics views"
	>
		{#each tabs as item, index}
			<button
				type="button"
				role="tab"
				aria-label={item.label}
				aria-selected={tab === item.id}
				tabindex={tab === item.id ? 0 : -1}
				title={item.label}
				class="flex min-w-0 items-center justify-center gap-2 px-3 py-2.5 text-sm font-semibold transition-colors hover:bg-(--obs-control-hover) focus-visible:z-10 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-(--obs-accent)"
				class:w-12={tab !== item.id}
				class:flex-1={tab === item.id}
				class:border-l={index > 0}
				class:border-(--obs-border)={index > 0}
				class:bg-(--obs-control-active)={tab === item.id}
				onclick={() => chooseTab(index)}
				onkeydown={(event) => onTabKeydown(event, index)}
			>
				<svg
					class="size-4 shrink-0"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="1.8"
					stroke-linecap="round"
					stroke-linejoin="round"
					aria-hidden="true"
				>
					{#if item.id === 'overview'}
						<path d="M4 20V10h4v10M10 20V4h4v16M16 20v-7h4v7M3 20h18" />
					{:else if item.id === 'times'}
						<circle cx="12" cy="14" r="8" />
						<path d="M12 10v4l3 2M9 2h6M12 2v4M18 7l2-2" />
					{:else if item.id === 'improvement'}
						<path d="m3 17 6-6 4 4 8-10M15 5h6v6" />
					{:else if item.id === 'outcomes'}
						<circle cx="12" cy="12" r="9" />
						<path d="m8 12 3 3 5-6" />
					{:else}
						<rect x="3" y="5" width="18" height="16" rx="2" />
						<path d="M16 3v4M8 3v4M3 11h18M7 15h3M14 15h3" />
					{/if}
				</svg>
				{#if tab === item.id}<span aria-hidden="true" class="truncate">{item.label}</span>{/if}
			</button>
		{/each}
	</div>
</div>

{#if controls && tab !== 'times'}
	<div class="mt-4">
		{@render controls()}
	</div>
{/if}

{#if error && tab !== 'times'}
	<div class="mt-4 rounded-sm border border-(--obs-danger) bg-(--obs-danger-surface) p-3 text-sm" role="alert">
		{error}
	</div>
{/if}

{#if tab === 'times'}
	{#if times}{@render times()}{/if}
{:else if loading && !data}
	<div class="mt-4 rounded-sm obs-panel p-8 text-center text-sm obs-muted">Loading statistics…</div>
{:else if data}
	{#if tab === 'overview'}
		<div class="mt-4">
			<StatisticsSummaryPanel
				title="General statistics"
				items={[
					{ label: 'Attempts', value: String(data.summary.counts.total) },
					{ label: 'Total session time', value: formatDuration(data.summary.totalSessionSeconds) },
					{ label: 'Most played', ...mostPlayedLevel(data) },
					{
						label: 'Overall combined time',
						value: formatDuration(data.summary.combinedBestTimes.overallSeconds)
					}
				]}
			/>
		</div>
		<section class="mt-4 rounded-sm obs-panel p-4">
			<SectionTitle title="Attempts over time" />
			<div class="mt-3">
				<Chart
					data={overallAttemptsData(data.overallBuckets)}
					title="Attempts over time"
					description="All attempts grouped into calendar buckets and split by outcome"
					xLabel="Date"
					yLabel="Attempts"
					formatXValue={bucket === 'year' ? (value) => String(new Date(Number(value)).getFullYear()) : undefined}
					interactiveLegend
					visibleSeriesIds={attemptsOverTimeStatuses}
					onVisibleSeriesChange={(ids) => (attemptsOverTimeStatuses = ids as RunStatus[])}
				/>
			</div>
		</section>
		<section class="mt-4 rounded-sm obs-panel p-4">
			<SectionTitle
				title={levelMeasure === 'attempts' ? 'Attempts by level' : 'Time spent by level'}
				actions={levelOrderActions}
			/>
			<div class="mt-3">
				<Chart
					data={attemptsByLevelData(data, levelOrder, levelMeasure)}
					title={levelMeasure === 'attempts' ? 'Attempts by level' : 'Time spent by level'}
					description="Three bars per level compare Agent, Secret Agent, and 00 Agent"
					xLabel={levelMeasure === 'attempts' ? 'Attempts' : 'Time spent'}
					formatValue={levelMeasure === 'time' ? formatDuration : undefined}
					interactiveLegend
					visibleSeriesIds={levelDifficulties.map(String)}
					onVisibleSeriesChange={(ids) => (levelDifficulties = ids.map(Number) as StatisticsLevelDifficulty[])}
				/>
			</div>
		</section>
		<div class="mt-4"><CombinedBestTimes value={data.summary.combinedBestTimes} /></div>
	{:else if tab === 'improvement'}
		<div class="mt-4">
			<StatisticsFilters bind:levelNumber bind:difficultyNumber />
		</div>
		<section class="mt-4 rounded-sm obs-panel p-4">
			<SectionTitle title="Run time improvement" />
			<p class="mt-1 text-xs obs-dim">The gold step line tracks personal-best progression.</p>
			<div class="mt-3">
				<Chart
					data={runTimeData(data)}
					title="Game time over chronological attempts"
					description="Individual game times for the selected level and difficulty"
					xLabel="Attempt date"
					yLabel="Game time"
					formatValue={formatDuration}
					includeZero={false}
					interactiveLegend
					visibleSeriesIds={improvementSeries}
					onVisibleSeriesChange={(ids) => (improvementSeries = ids as StatisticsImprovementSeries[])}
				/>
			</div>
		</section>
	{:else if tab === 'outcomes'}
		<section class="mt-4 rounded-sm obs-panel p-4">
			<SectionTitle title="Outcome mix" actions={outcomeMeasureActions} />
			<p class="mt-1 text-xs obs-dim">
				{outcomeMeasure === 'share'
					? 'Selected outcomes are stacked to show each calendar bucket as a whole.'
					: 'Selected outcomes are side-by-side for each calendar bucket.'}
			</p>
			<div class="mt-3">
				<Chart
					data={outcomeData(data.overallBuckets, outcomeStatuses, outcomeMeasure)}
					title="Outcome mix over time"
					description={outcomeMeasure === 'share'
						? 'Stacked bars show the selected run outcome share in each period'
						: 'Grouped bars compare the selected run outcomes in each period'}
					xLabel="Date"
					yLabel={outcomeMeasure === 'share' ? 'Share' : 'Attempts'}
					formatXValue={bucket === 'year' ? (value) => String(new Date(Number(value)).getFullYear()) : undefined}
					formatValue={outcomeMeasure === 'share' ? (value) => `${Math.round(value)}%` : undefined}
					interactiveLegend
					visibleSeriesIds={outcomeStatuses}
					onVisibleSeriesChange={(ids) => (outcomeStatuses = ids as RunStatus[])}
				/>
			</div>
		</section>
	{:else if tab === 'sessions'}
		<div class="mt-4">
			<SessionStatistics
				{sessions}
				bind:selectedSessionId
				detail={sessionDetail}
				loading={sessionLoading}
				bind:sessionStatuses
			/>
		</div>
	{/if}
{/if}
