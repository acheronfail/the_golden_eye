<script lang="ts">
	import type { MonitoringSessionDetail, MonitoringSessionSummary, RunStatus } from '$lib/api';
	import Chart from '$lib/ui/Chart/Chart.svelte';
	import SectionTitle from '$lib/ui/SectionTitle.svelte';
	import Select from '$lib/ui/Select.svelte';
	import StatisticsSummaryPanel from './StatisticsSummaryPanel.svelte';
	import {
		formatDuration,
		sessionAttemptsData,
		STATUS_LABELS,
		ALL_STATUSES
	} from '$lib/features/statistics/statisticsView';

	let {
		sessions,
		selectedSessionId = $bindable(),
		detail,
		loading = false,
		sessionStatuses = $bindable([...ALL_STATUSES])
	}: {
		sessions: MonitoringSessionSummary[];
		selectedSessionId: string;
		detail: MonitoringSessionDetail | null;
		loading?: boolean;
		sessionStatuses?: RunStatus[];
	} = $props();

	const options = $derived(
		sessions.map((session) => ({
			value: session.sessionId,
			label: `${new Date(session.startedAt).toLocaleString()} · ${session.counts.total} ${session.counts.total === 1 ? 'attempt' : 'attempts'}`
		}))
	);

	const outcomes = [{ key: 'complete' }, { key: 'failed' }, { key: 'abort' }, { key: 'kia' }] as const;

	function sessionDuration(session: MonitoringSessionSummary): string {
		if (!session.endedAt) return session.endReason === 'interrupted' ? 'Unknown duration' : 'In progress';
		return formatDuration((new Date(session.endedAt).getTime() - new Date(session.startedAt).getTime()) / 1000);
	}
</script>

{#if sessions.length === 0 && !loading}
	<div class="rounded-sm obs-empty-state p-6 text-center">
		<p class="font-semibold">No monitoring sessions in this range</p>
		<p class="mt-1 text-sm obs-muted">New sessions are recorded when monitoring starts.</p>
	</div>
{:else}
	<SectionTitle title="Session" class="mb-3" />
	<label for="statistics-session">
		<span class="sr-only">Session</span>
		<Select
			id="statistics-session"
			value={selectedSessionId}
			{options}
			onChange={(value) => (selectedSessionId = value)}
			class="w-full px-3 py-2 text-left text-sm"
			disabled={loading}
		/>
	</label>

	{#if loading || !detail}
		<div class="mt-4 rounded-sm obs-panel p-6 text-center text-sm obs-muted">Loading session…</div>
	{:else}
		<div class="mt-4">
			<StatisticsSummaryPanel
				title={`Session statistics`}
				subtitle={`Total session time: ${sessionDuration(detail)}`}
				summaryValue={String(detail.counts.total)}
				summaryDetail="total runs"
				items={outcomes.map((outcome) => ({
					label: STATUS_LABELS[outcome.key],
					value: String(detail.counts[outcome.key])
				}))}
			/>
		</div>

		<section class="mt-4 rounded-sm obs-panel p-4">
			<SectionTitle title="Attempts through the session" />
			<div class="mt-3">
				<Chart
					data={sessionAttemptsData(detail)}
					title="Session run times"
					description="Game-reported run time plotted against elapsed monitoring-session time"
					xLabel="Elapsed session time"
					yLabel="Game time"
					formatXValue={(value) => formatDuration(Number(value))}
					formatValue={formatDuration}
					interactiveLegend
					visibleSeriesIds={sessionStatuses}
					onVisibleSeriesChange={(ids) => (sessionStatuses = ids as RunStatus[])}
				/>
			</div>
		</section>
	{/if}
{/if}
