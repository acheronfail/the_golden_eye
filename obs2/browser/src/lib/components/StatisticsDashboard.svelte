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
	import Chart from './Chart.svelte';
	import CombinedBestTimes from './CombinedBestTimes.svelte';
	import SectionTitle from './SectionTitle.svelte';
	import SegmentedControl from './SegmentedControl.svelte';
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
	} from '$lib/utils/statisticsView';
	import type {
		StatisticsImprovementSeries,
		StatisticsLevelOrder,
		StatisticsOutcomeMeasure,
		StatisticsTab
	} from '$lib/utils/statisticsPreferences';

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
		attemptsByLevelStatuses = $bindable([...ALL_STATUSES]),
		attemptsOverTimeStatuses = $bindable([...ALL_STATUSES]),
		improvementSeries = $bindable(['running-best', 'complete']),
		outcomeStatuses = $bindable([...ALL_STATUSES]),
		sessionStatuses = $bindable([...ALL_STATUSES]),
		outcomeMeasure = $bindable('share'),
		levelOrder = $bindable('attempts'),
		controls
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
		attemptsByLevelStatuses?: RunStatus[];
		attemptsOverTimeStatuses?: RunStatus[];
		improvementSeries?: StatisticsImprovementSeries[];
		outcomeStatuses?: RunStatus[];
		sessionStatuses?: RunStatus[];
		outcomeMeasure?: StatisticsOutcomeMeasure;
		levelOrder?: StatisticsLevelOrder;
		controls?: Snippet;
	} = $props();

	const tabs: Array<{ id: StatisticsTab; label: string }> = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'improvement', label: 'Improvement' },
		{ id: 'outcomes', label: 'Outcomes' },
		{ id: 'sessions', label: 'Sessions' }
	];
</script>

{#snippet levelOrderActions()}
	<SegmentedControl
		value={levelOrder}
		options={[
			{ value: 'attempts', label: 'Most attempted' },
			{ value: 'mission', label: 'Mission order' }
		]}
		ariaLabel="Attempts by level order"
		onChange={(value) => (levelOrder = value as StatisticsLevelOrder)}
	/>
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

<div class="sticky top-0 z-20 -mx-4 border-b border-(--obs-border-muted) bg-(--obs-bg) px-4 sm:-mx-6 sm:px-6">
	<div class="flex gap-1 overflow-x-auto" role="tablist" aria-label="Statistics views">
		{#each tabs as item}
			<button
				type="button"
				role="tab"
				aria-selected={tab === item.id}
				class="border-b-2 px-3 py-2 text-sm font-semibold transition-colors"
				class:border-(--obs-gold)={tab === item.id}
				class:text-(--obs-gold-hover)={tab === item.id}
				class:border-transparent={tab !== item.id}
				class:obs-muted={tab !== item.id}
				onclick={() => (tab = item.id)}
			>
				{item.label}
			</button>
		{/each}
	</div>
</div>

{#if controls}
	<div class="mt-4">
		{@render controls()}
	</div>
{/if}

{#if error}
	<div class="mt-4 rounded-sm border border-(--obs-danger) bg-(--obs-danger-surface) p-3 text-sm" role="alert">
		{error}
	</div>
{/if}

{#if loading && !data}
	<div class="mt-4 rounded-sm obs-panel p-8 text-center text-sm obs-muted">Loading statistics…</div>
{:else if data}
	{#if tab === 'overview'}
		<div class="mt-4">
			<StatisticsSummaryPanel
				title="General statistics"
				items={[
					{ label: 'Attempts', value: String(data.summary.counts.total) },
					{ label: 'Total session time', value: formatDuration(data.summary.totalSessionSeconds) },
					{ label: 'Most played', value: mostPlayedLevel(data) },
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
			<SectionTitle title="Attempts by level" actions={levelOrderActions} />
			<div class="mt-3">
				<Chart
					data={attemptsByLevelData(data, levelOrder)}
					title="Attempts by level"
					description="Horizontal bars show attempts for every level split by Complete, Failed, Aborted, and Killed in Action"
					xLabel="Share of attempts"
					interactiveLegend
					visibleSeriesIds={attemptsByLevelStatuses}
					onVisibleSeriesChange={(ids) => (attemptsByLevelStatuses = ids as RunStatus[])}
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
			<p class="mt-1 text-xs obs-dim">Selected outcomes are side-by-side for each calendar bucket.</p>
			<div class="mt-3">
				<Chart
					data={outcomeData(data.overallBuckets, outcomeStatuses, outcomeMeasure)}
					title="Outcome mix over time"
					description="Grouped bars compare the selected run outcomes in each period"
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
