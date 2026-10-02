<script lang="ts">
	import type { RunClip } from '$lib/api';
	import { clipTimeSeconds, romVersionLabel } from '$lib/features/runs/runsView';
	import { formatDuration, LEVEL_NAMES } from './statisticsView';
	import { bestTime, TIME_DIFFICULTIES, timesForLevel, timesHistoryHref } from './timesView';

	let {
		runs,
		loading = false,
		error = null,
		retry,
		level = null,
		difficulty = null
	}: {
		runs: RunClip[];
		loading?: boolean;
		error?: string | null;
		retry?: () => void;
		level?: number | null;
		difficulty?: string | null;
	} = $props();

	const selected = $derived(
		level !== null &&
			level >= 1 &&
			level <= 20 &&
			Number.isInteger(level) &&
			difficulty !== null &&
			TIME_DIFFICULTIES.includes(difficulty)
	);
	const history = $derived(selected ? timesForLevel(runs, level!, difficulty!) : []);
	const rows = $derived(
		LEVEL_NAMES.map((name, index) => ({
			name,
			level: index + 1,
			times: TIME_DIFFICULTIES.map((difficulty) => ({
				difficulty,
				best: bestTime(timesForLevel(runs, index + 1, difficulty))
			}))
		}))
	);
	const hasTimes = $derived(
		selected ? history.length > 0 : rows.some((row) => row.times.some((time) => time.best !== null))
	);
	const dateLabel = (timestamp: string) => {
		const date = new Date(timestamp);
		return Number.isNaN(date.getTime()) ? 'Unknown date' : date.toLocaleString();
	};
</script>

<section class="mt-4 rounded-sm obs-panel p-4" aria-label="Run times">
	{#if selected}
		<a class="obs-text-button inline-block px-2 py-1 text-sm" href="/statistics?tab=times">← All best times</a>
		<h2 class="mt-4 text-lg font-semibold obs-heading">{LEVEL_NAMES[level! - 1]} · {difficulty}</h2>
		{#if hasTimes && !loading && !error}
			<p class="mt-1 text-sm obs-muted">All completed times, newest first. Select a time to open its run details.</p>
		{/if}
	{:else}
		<h2 class="text-lg font-semibold obs-heading">Best times</h2>
		{#if hasTimes && !loading && !error}
			<p class="mt-1 text-sm obs-muted">All-time best completed runs. Select a time to view its history.</p>
		{/if}
	{/if}

	{#if loading}
		<p class="py-8 text-center text-sm obs-muted" role="status">Loading times…</p>
	{:else if error}
		<div class="mt-4 rounded obs-alert-error p-3" role="alert">
			<p class="text-sm">Could not load times: {error}</p>
			{#if retry}<button type="button" class="mt-2 obs-text-button px-2 py-1 text-sm" onclick={retry}>Retry</button
				>{/if}
		</div>
	{:else if !hasTimes}
		<div class="mt-4 rounded obs-empty-state px-4 py-6 text-center">
			<p class="text-sm obs-muted">
				{selected ? 'No completed times for this level and difficulty yet.' : 'No completed times yet.'}
			</p>
			<p class="mt-1 font-mono text-xs obs-dim">Recorded and imported times will appear here.</p>
		</div>
	{:else if selected}
		<div class="mt-4 overflow-x-auto">
			<table class="w-full text-left text-sm">
				<caption class="sr-only">{LEVEL_NAMES[level! - 1]} {difficulty} time history</caption>
				<thead class="border-b border-(--obs-border-muted) obs-dim"
					><tr
						><th scope="col" class="p-2">Date</th><th scope="col" class="p-2">Time</th><th scope="col" class="p-2"
							>System</th
						></tr
					></thead
				>
				<tbody>
					{#each history as run (run.runId)}
						<tr class="border-b border-(--obs-border-muted)">
							<td class="p-2 obs-muted">{dateLabel(run.metadata.timestamp)}</td>
							<td class="p-2"
								><a
									class="obs-text-button inline-block px-2 py-1 font-mono text-sm"
									href={`/runs?runId=${encodeURIComponent(run.runId)}`}
									aria-label={`Open run ${formatDuration(clipTimeSeconds(run)!)} from ${dateLabel(run.metadata.timestamp)}`}
									>{formatDuration(clipTimeSeconds(run)!)}</a
								></td
							>
							<td class="p-2 font-mono text-xs obs-muted">{romVersionLabel(run.metadata.romVersion) ?? 'Unknown'}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{:else}
		<div class="mt-4 overflow-x-auto">
			<table class="w-full text-left text-sm">
				<caption class="sr-only">Best time for every level and difficulty</caption>
				<thead class="border-b border-(--obs-border-muted) obs-dim"
					><tr
						><th scope="col" class="p-2">Level</th>{#each TIME_DIFFICULTIES as difficulty}<th
								scope="col"
								class="p-2 text-center">{difficulty}</th
							>{/each}</tr
					></thead
				>
				<tbody>
					{#each rows as row}
						<tr class="border-b border-(--obs-border-muted)">
							<th scope="row" class="p-2 font-medium whitespace-nowrap obs-heading">{row.name}</th>
							{#each row.times as time}
								<td class="p-2 text-center font-mono">
									{#if time.best !== null}<a
											class="obs-text-button inline-block px-2 py-1 text-sm"
											href={timesHistoryHref(row.level, time.difficulty)}
											aria-label={`${row.name} ${time.difficulty} ${formatDuration(time.best)} history`}
											>{formatDuration(time.best)}</a
										>{:else}<span class="obs-dim" aria-label="No completed time">—</span>{/if}
								</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</section>
