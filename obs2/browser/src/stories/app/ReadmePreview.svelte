<script lang="ts">
	import AppHeader from '$lib/app/AppHeader.svelte';
	import { monitorPhaseStyleForPhase } from '$lib/stores/monitor.svelte';
	import MonitorView from '$lib/features/monitor/MonitorView.svelte';
	import type { MonitorDesign } from '$lib/features/monitor/monitorView';
	import RunList from '$lib/features/runs/RunList.svelte';
	import RunFilters from '$lib/features/runs/RunFilters.svelte';
	import { createRunListRows, EMPTY_RUN_FILTERS, LEVEL_OPTIONS } from '$lib/features/runs/runsView';
	import StatisticsDashboard from '$lib/features/statistics/StatisticsDashboard.svelte';
	import DateRangeSelect from '$lib/features/statistics/DateRangeSelect.svelte';
	import SectionTitle from '$lib/ui/SectionTitle.svelte';
	import ActionMenu from '$lib/ui/ActionMenu.svelte';
	import { monitorBaseArgs, monitorMatch } from '../features/monitor/monitorStoryFixtures';
	import { readmeRuns, readmeStatistics } from '../readmeFixtures';

	let {
		view = 'monitor',
		design = 'mission-glass'
	}: {
		view?: 'monitor' | 'runs' | 'statistics';
		design?: MonitorDesign;
	} = $props();
	const links = [
		{ href: '/', label: 'Monitor' },
		{ href: '/statistics', label: 'Statistics' },
		{ href: '/runs', label: 'Runs' },
		{ href: '/options', label: 'Options' }
	];
	let filters = $state({ ...EMPTY_RUN_FILTERS });
	const noop = () => {};
</script>

<div
	class="obs-window-focused obs-app-shell flex h-screen min-h-0 min-w-100 flex-col overflow-hidden {monitorPhaseStyleForPhase(
		view === 'monitor' ? 'recording' : 'complete'
	).border}"
>
	<AppHeader
		{links}
		currentPath={view === 'monitor' ? '/sources/N64%20Capture' : `/${view}`}
		pluginVersion="0.0.0"
		activeMonitorHref={view === 'monitor' ? '/sources/N64%20Capture' : null}
		recordingState={view === 'monitor' ? 'started' : null}
		youtubeConnected={true}
	/>
	<div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto obs-content-scroller">
		{#if view === 'monitor'}
			<MonitorView
				{...monitorBaseArgs}
				{design}
				recordingState="started"
				match={{ ...monitorMatch('start'), mission: 1, part: 2, difficulty: 2 }}
				wallClockState={{
					...monitorBaseArgs.wallClockState,
					sessionElapsedMs: 1_345_000,
					levelTimerPhase: 'awaitingInitialBlack'
				}}
				recentRuns={readmeRuns.slice(0, 5)}
			/>
		{:else}
			<main class="mx-auto w-full max-w-3xl px-4 obs-page-top pb-4 sm:px-6 sm:pb-6">
				{#if view === 'runs'}
					<div class="mb-4 flex items-center gap-3">
						<h1 class="text-2xl font-semibold obs-heading">Runs</h1>
						<div class="relative z-30 ml-auto flex">
							<button
								type="button"
								class="obs-button h-8 rounded-r-none border-r-0 obs-button-gold px-3 font-mono text-xs"
								>+ Add times</button
							>
							<ActionMenu
								items={[]}
								label="More run actions"
								title="More run actions"
								triggerClass="h-8 w-8 shrink-0 rounded-l-none px-2 font-mono text-sm"
								triggerGlyph="▾"
							/>
						</div>
					</div>
					<RunFilters
						collapsed={true}
						{filters}
						activeFilters={[]}
						hasActiveFilters={false}
						levelOptions={LEVEL_OPTIONS.map((level) => ({ value: level, label: level }))}
						clearFilter={noop}
						clearFilters={noop}
					/>
					<RunList
						loading={false}
						clips={readmeRuns}
						rows={createRunListRows(readmeRuns, 'newest')}
						scannedDirectoryCount={1}
						directoryCount={1}
						hasActiveFilters={false}
						clearFilters={noop}
						sort="newest"
						onSortChange={noop}
						fileBrowserLabel="Show in file browser"
						open={noop}
						rename={noop}
						reveal={noop}
						remove={noop}
					/>
				{:else}
					<header class="mb-4"><h1 class="text-2xl font-semibold obs-heading">Statistics</h1></header>
					<StatisticsDashboard
						data={readmeStatistics}
						loading={false}
						error={null}
						levelNumber={1}
						difficultyNumber={0}
						tab="improvement"
						sessions={[]}
						selectedSessionId=""
						sessionDetail={null}
					>
						{#snippet controls()}
							<section>
								<SectionTitle title="Filters" class="mb-3" />
								<DateRangeSelect
									value={{ preset: 'all', customFrom: '', customTo: '' }}
									bucket="week"
									showGroupBy={false}
								/>
							</section>
						{/snippet}
					</StatisticsDashboard>
				{/if}
			</main>
		{/if}
	</div>
</div>
