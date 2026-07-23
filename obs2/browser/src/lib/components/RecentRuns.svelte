<script lang="ts">
	import { tick } from 'svelte';
	import type { RunClip } from '$lib/api';
	import { formatMonitorTime } from './monitorView';

	let {
		runs,
		variant = 'mission-glass',
		busyRunId = null,
		error = null,
		onKeep
	}: {
		runs: RunClip[];
		variant?: 'mission-glass' | 'signal-band' | 'debug';
		busyRunId?: string | null;
		error?: string | null;
		onKeep: (runId: string) => void;
	} = $props();

	const runTime = (run: RunClip): string =>
		run.metadata.time ?? (run.metadata.timeSeconds == null ? '—' : formatMonitorTime(run.metadata.timeSeconds));

	let runScroll = $state<HTMLDivElement | null>(null);
	let firstRunKey: string | null = null;

	$effect(() => {
		const nextFirstRunKey = runs[0]?.runId ?? runs[0]?.metadata.timestamp ?? null;
		if (nextFirstRunKey === firstRunKey) return;
		firstRunKey = nextFirstRunKey;
		if (!nextFirstRunKey) return;
		void tick().then(() => {
			if (runScroll) runScroll.scrollTop = 0;
		});
	});
</script>

<section class={`recent-runs recent-runs--${variant}`} aria-label="Recent runs">
	<header><span>Recent runs</span><small>{runs.length}</small></header>
	{#if error}<p class="error">{error}</p>{/if}
	{#if runs.length === 0}
		<p class="empty">Finished runs will appear here.</p>
	{:else}
		<div class="run-scroll" bind:this={runScroll}>
			{#each runs as run, index (run.runId ?? run.metadata.timestamp)}
				<article class:hidden-run={index > 0}>
					<div class="run-copy">
						{#if run.runId}
							<a class="run-name" href={`/runs?runId=${encodeURIComponent(run.runId)}`}>{run.metadata.level}</a>
						{:else}
							<strong>{run.metadata.level}</strong>
						{/if}
						<span class="run-summary">
							<span>{run.metadata.difficulty ?? 'Unknown'}</span>
							<span aria-hidden="true">-</span>
							<span class:personal-best={run.retentionReason === 'personalBest'}>{runTime(run)}</span>
							<span aria-hidden="true">-</span>
							<span
								class:status-complete={run.metadata.status === 'complete'}
								class:status-failed={run.metadata.status !== 'complete' && run.metadata.status !== 'pending'}
								>{run.metadata.status}</span
							>
						</span>
					</div>
					{#if run.path && run.retentionState === 'pending'}
						<button
							class="row-action"
							disabled={busyRunId === run.runId}
							onclick={() => run.runId && onKeep(run.runId)}
						>
							{busyRunId === run.runId ? 'Keeping…' : 'Keep'}
						</button>
					{:else}
						<span class:kept={run.retentionState === 'kept'} class="row-action state">
							{run.retentionReason === 'personalBest'
								? 'PB'
								: run.retentionState === 'pending' && !run.path
									? 'Saving…'
									: run.retentionState}
						</span>
					{/if}
				</article>
			{/each}
		</div>
	{/if}
</section>

<style>
	.recent-runs {
		width: 100%;
		min-height: 0;
		max-height: 11rem;
		display: grid;
		grid-template-rows: auto minmax(0, 1fr);
		overflow: hidden;
		color: var(--obs-text);
		font-family: var(--font-mono, ui-monospace, monospace);
	}
	.recent-runs--mission-glass {
		border: 1px solid color-mix(in srgb, var(--monitor-accent) 30%, var(--obs-border-soft));
		border-radius: 0.75rem;
		background: rgb(37 41 52 / 90%);
		box-shadow:
			inset 0 1px 0 rgb(255 255 255 / 8%),
			0 0 2rem var(--monitor-surface);
	}
	.recent-runs--signal-band {
		border-top: 1px solid color-mix(in srgb, var(--monitor-accent) 45%, var(--obs-border));
		border-bottom: 1px solid color-mix(in srgb, var(--monitor-accent) 25%, var(--obs-border));
		background: color-mix(in srgb, var(--monitor-surface) 35%, var(--obs-bg-elevated));
		box-shadow: 0 0 2rem var(--monitor-surface);
	}
	.recent-runs--debug {
		border-top: 1px solid var(--obs-border-muted);
		border-left: 1px solid var(--obs-border-muted);
		background: var(--obs-bg-elevated);
	}
	@container (min-height: 48rem) {
		.recent-runs--mission-glass,
		.recent-runs--signal-band,
		.recent-runs--debug {
			max-height: min(34cqh, 24rem);
		}
	}
	header {
		display: flex;
		justify-content: space-between;
		padding: 0.45rem 0.65rem;
		border-bottom: 1px solid var(--obs-border-muted);
		color: var(--obs-text-dim);
		font-size: 0.65rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
	}
	.recent-runs--signal-band header {
		color: var(--monitor-accent);
	}
	.recent-runs--debug header {
		border-right: 1px solid var(--obs-border-muted);
		color: var(--debug-accent);
	}
	.run-scroll {
		min-height: 0;
		overflow-y: auto;
	}
	article {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.65rem;
		padding: 0.48rem 0.65rem;
		border-bottom: 1px solid var(--obs-border-muted);
		font-size: 0.72rem;
	}
	.recent-runs--debug article {
		border-right: 1px solid var(--obs-border-muted);
		background: var(--obs-bg-elevated);
	}
	.run-copy {
		min-width: 0;
		display: flex;
		align-items: baseline;
	}
	.run-copy strong,
	.run-copy .run-name,
	.run-summary {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.run-summary,
	.empty {
		color: var(--obs-text-dim);
	}
	.run-summary {
		min-width: 0;
		display: flex;
		gap: 0.4em;
		margin-left: 0.55em;
	}
	.personal-best {
		color: var(--obs-gold);
	}
	.status-complete {
		color: var(--obs-success);
	}
	.status-failed {
		color: var(--obs-danger);
	}
	.run-name {
		flex: 0 1 auto;
		min-width: 0;
		font-weight: 700;
		cursor: pointer;
		text-decoration: underline transparent;
		text-underline-offset: 0.18em;
		transition:
			color 120ms ease,
			text-decoration-color 120ms ease;
	}
	.run-name:hover {
		color: var(--obs-gold);
		text-decoration-color: currentColor;
	}
	.run-name:active {
		color: var(--obs-gold-hover);
	}
	.row-action {
		box-sizing: border-box;
		min-height: 1.75rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border: 1px solid transparent;
		border-radius: 0.3rem;
		padding: 0.25rem 0.55rem;
		font: inherit;
		line-height: 1;
		white-space: nowrap;
	}
	button.row-action {
		border-color: var(--obs-gold);
		color: var(--obs-gold);
		cursor: pointer;
		transition:
			color 120ms ease,
			background-color 120ms ease,
			transform 80ms ease;
	}
	button:hover:not(:disabled) {
		background: var(--obs-gold);
		color: var(--obs-bg);
	}
	button:active:not(:disabled) {
		transform: translateY(1px);
		background: var(--obs-gold-hover);
		color: var(--obs-bg);
	}
	button:disabled {
		opacity: 0.55;
		cursor: default;
	}
	.state {
		border-color: var(--obs-border);
		background: color-mix(in srgb, var(--obs-bg-elevated) 72%, transparent);
		color: var(--obs-text-dim);
		text-transform: uppercase;
	}
	.state.kept {
		border-color: color-mix(in srgb, var(--obs-success) 55%, var(--obs-border));
		background: color-mix(in srgb, var(--obs-success) 10%, transparent);
		color: var(--obs-success);
	}
	.empty,
	.error {
		padding: 0.65rem;
		font-size: 0.7rem;
	}
	.error {
		color: var(--obs-danger);
	}
	@container (max-height: 42rem) {
		.recent-runs--signal-band {
			max-height: 3.25rem;
		}
		.recent-runs--signal-band article.hidden-run {
			display: none;
		}
		.recent-runs--signal-band header {
			display: none;
		}
	}
	@container (max-height: 28rem) {
		.recent-runs:not(.recent-runs--debug) {
			max-height: 4.2rem;
		}
		.recent-runs:not(.recent-runs--debug) article.hidden-run {
			display: none;
		}
		.recent-runs:not(.recent-runs--debug) header {
			display: none;
		}
	}
</style>
