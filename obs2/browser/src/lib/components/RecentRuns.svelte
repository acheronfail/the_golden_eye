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
	const variantClass = $derived(
		variant === 'mission-glass'
			? 'rounded-xl border border-[color-mix(in_srgb,var(--monitor-accent)_30%,var(--obs-border-soft))] bg-[rgb(37_41_52/90%)] shadow-[inset_0_1px_0_rgb(255_255_255/8%),0_0_2rem_var(--monitor-surface)]'
			: variant === 'signal-band'
				? 'border-y border-[color-mix(in_srgb,var(--monitor-accent)_35%,var(--obs-border))] bg-[color-mix(in_srgb,var(--monitor-surface)_35%,var(--obs-bg-elevated))] shadow-[0_0_2rem_var(--monitor-surface)]'
				: 'border-t border-l border-(--obs-border-muted) bg-(--obs-bg-elevated)'
	);

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

<section
	class="recent-runs recent-runs--{variant} grid max-h-[clamp(4.2rem,22cqh,24rem)] min-h-0 w-full grid-rows-[auto_minmax(0,1fr)] overflow-hidden font-mono text-(--obs-text) {variantClass}"
	aria-label="Recent runs"
>
	<header
		class="flex justify-between border-b border-(--obs-border-muted) px-2.5 py-2 text-[0.65rem] tracking-widest text-(--obs-text-dim) uppercase"
		class:text-(--monitor-accent)={variant === 'signal-band'}
		class:border-r={variant === 'debug'}
	>
		<span>Recent runs</span><small>{runs.length}</small>
	</header>
	{#if error}<p class="error p-2.5 text-[0.7rem] text-(--obs-danger)">{error}</p>{/if}
	{#if runs.length === 0}
		<p class="empty p-2.5 text-[0.7rem] text-(--obs-text-dim)">Finished runs will appear here.</p>
	{:else}
		<div class="run-scroll min-h-0 overflow-y-auto" bind:this={runScroll}>
			{#each runs as run, index (run.runId ?? run.metadata.timestamp)}
				<article
					class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-2.5 border-b border-(--obs-border-muted) px-2.5 py-2 text-[0.72rem]"
					class:border-r={variant === 'debug'}
					class:bg-(--obs-bg-elevated)={variant === 'debug'}
					class:hidden-run={index > 0}
				>
					<div class="run-copy flex min-w-0 items-baseline">
						{#if run.runId}
							<a
								class="run-name min-w-0 flex-[0_1_auto] cursor-pointer overflow-hidden font-bold text-ellipsis whitespace-nowrap underline decoration-transparent underline-offset-[0.18em] transition-[color,text-decoration-color] duration-120 hover:text-(--obs-gold) hover:decoration-current active:text-(--obs-gold-hover)"
								href={`/runs?runId=${encodeURIComponent(run.runId)}`}>{run.metadata.level}</a
							>
						{:else}
							<strong class="overflow-hidden text-ellipsis whitespace-nowrap">{run.metadata.level}</strong>
						{/if}
						<span
							class="run-summary ml-[0.55em] flex min-w-0 gap-[0.4em] overflow-hidden text-ellipsis whitespace-nowrap text-(--obs-text-dim)"
						>
							<span>{run.metadata.difficulty ?? 'Unknown'}</span>
							<span aria-hidden="true">-</span>
							<span
								class:personal-best={run.retentionReason === 'personalBest'}
								class:text-(--obs-gold)={run.retentionReason === 'personalBest'}>{runTime(run)}</span
							>
							<span aria-hidden="true">-</span>
							<span
								class:status-complete={run.metadata.status === 'complete'}
								class:text-(--obs-success)={run.metadata.status === 'complete'}
								class:status-failed={run.metadata.status !== 'complete' && run.metadata.status !== 'pending'}
								class:text-(--obs-danger)={run.metadata.status !== 'complete' && run.metadata.status !== 'pending'}
								>{run.metadata.status}</span
							>
						</span>
					</div>
					{#if run.path && run.retentionState === 'pending'}
						<button
							class="row-action inline-flex min-h-7 cursor-pointer items-center justify-center rounded-sm border border-(--obs-gold) px-2 py-1 font-[inherit] leading-none whitespace-nowrap text-(--obs-gold) uppercase transition-[color,background-color,transform] duration-120 enabled:hover:bg-(--obs-gold) enabled:hover:text-(--obs-bg) enabled:active:translate-y-px enabled:active:bg-(--obs-gold-hover) enabled:active:text-(--obs-bg) disabled:cursor-default disabled:opacity-55"
							disabled={busyRunId === run.runId}
							onclick={() => run.runId && onKeep(run.runId)}
						>
							{busyRunId === run.runId ? 'Keeping…' : 'Keep'}
						</button>
					{:else}
						<span
							class="row-action state inline-flex min-h-7 items-center justify-center rounded-sm border border-(--obs-border) bg-[color-mix(in_srgb,var(--obs-bg-elevated)_72%,transparent)] px-2 py-1 font-[inherit] leading-none whitespace-nowrap text-(--obs-text-dim) uppercase"
							class:kept={run.retentionState === 'kept'}
							class:border-[color-mix(in_srgb,var(--obs-success)_55%,var(--obs-border))]={run.retentionState === 'kept'}
							class:bg-[color-mix(in_srgb,var(--obs-success)_10%,transparent)]={run.retentionState === 'kept'}
							class:text-(--obs-success)={run.retentionState === 'kept'}
						>
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
