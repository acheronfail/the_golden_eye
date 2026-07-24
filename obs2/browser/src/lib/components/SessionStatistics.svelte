<script lang="ts">
	import type { MonitoringSessionDetail, MonitoringSessionSummary } from '$lib/api';
	import Chart from './Chart.svelte';
	import SectionTitle from './SectionTitle.svelte';
	import Select from './Select.svelte';
	import StatisticsSummaryPanel from './StatisticsSummaryPanel.svelte';
	import {
		difficultyLabel,
		formatDuration,
		levelLabel,
		sessionAttemptsData,
		STATUS_LABELS
	} from '$lib/utils/statisticsView';

	let {
		sessions,
		selectedSessionId = $bindable(),
		detail,
		loading = false
	}: {
		sessions: MonitoringSessionSummary[];
		selectedSessionId: string;
		detail: MonitoringSessionDetail | null;
		loading?: boolean;
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
				/>
			</div>
		</section>

		<section class="mt-4 overflow-x-auto rounded-sm obs-panel p-4">
			<SectionTitle title="Level and difficulty cohorts" />
			<table class="mt-3 w-full min-w-120 table-fixed border-collapse text-left text-sm">
				<colgroup>
					<col class="w-[24%]" />
					<col class="w-[11%]" />
					<col class="w-[27%]" />
					<col class="w-[22%]" />
					<col class="w-[16%]" />
				</colgroup>
				<thead class="text-xs obs-dim">
					<tr>
						<th class="border-b border-(--obs-border-muted) py-2 pr-2">Cohort</th>
						<th class="border-b border-(--obs-border-muted) py-2 pr-2">Runs</th>
						<th class="border-b border-(--obs-border-muted) py-2 pr-2">Outcomes</th>
						<th class="border-b border-(--obs-border-muted) py-2 pr-2">Average</th>
						<th class="border-b border-(--obs-border-muted) py-2">Best</th>
					</tr>
				</thead>
				<tbody>
					{#each detail.levels as level}
						<tr>
							<td class="border-b border-(--obs-border-muted) py-2 pr-2">
								<span class="block">{levelLabel(level.levelNumber)}</span>
								<span class="block text-xs obs-muted">{difficultyLabel(level.difficultyNumber)}</span>
							</td>
							<td class="border-b border-(--obs-border-muted) py-2 pr-2">{level.counts.total}</td>
							<td class="border-b border-(--obs-border-muted) py-2 pr-2 text-xs obs-muted">
								<ul class="m-0 grid list-none gap-0.5 p-0">
									{#each outcomes as outcome}
										{#if level.counts[outcome.key] > 0}
											<li><span class="font-mono">{level.counts[outcome.key]}</span> {STATUS_LABELS[outcome.key]}</li>
										{/if}
									{/each}
								</ul>
							</td>
							<td class="border-b border-(--obs-border-muted) py-2 pr-2">
								{level.averageTimeSeconds == null ? '—' : formatDuration(level.averageTimeSeconds)}
								<span class="block text-xs obs-dim">of {level.timedRuns} timed</span>
							</td>
							<td class="border-b border-(--obs-border-muted) py-2">
								{level.bestCompletedTimeSeconds == null ? '—' : formatDuration(level.bestCompletedTimeSeconds)}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</section>
	{/if}
{/if}
