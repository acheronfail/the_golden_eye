<script lang="ts">
	import type { LevelMatch } from '$lib/api';
	import { formatMonitorTime, type MonitorPresentation } from './monitorView';

	let {
		variant,
		verified,
		presentation,
		match = null
	}: {
		variant: 'mission-glass' | 'signal-band';
		verified: boolean;
		presentation: MonitorPresentation;
		match?: LevelMatch | null;
	} = $props();
</script>

{#snippet statusText()}
	<p
		class="font-mono text-[clamp(0.65rem,2.8cqw,0.82rem)] tracking-[0.15em] text-(--monitor-accent) uppercase transition-colors duration-240 [@container(max-height:42rem)]:text-[clamp(0.58rem,2cqw,0.72rem)]"
	>
		{verified ? presentation.statusLabel : 'Verifying source'}{variant === 'signal-band' ? ' / ACTIVE' : ''}
	</p>
	<h1
		class="max-w-full leading-[0.9] font-semibold tracking-[-0.065em] [overflow-wrap:anywhere] text-[color-mix(in_srgb,var(--monitor-accent)_12%,var(--obs-text))] transition-colors duration-240 [@container(max-height:42rem)]:mt-[0.3rem] [@container(max-height:42rem)]:mb-[0.4rem] [@container(max-height:42rem)]:text-[clamp(2rem,8cqw,3.5rem)] {variant ===
		'signal-band'
			? 'mt-2 mb-[0.65rem] text-[clamp(2.4rem,11cqw,5.25rem)]'
			: 'mt-[0.55rem] mb-[0.7rem] text-[clamp(2.25rem,11cqw,5rem)]'}"
	>
		{verified ? presentation.title : 'checking source'}
	</h1>
	<p
		class="monitor-detail font-mono text-[clamp(0.65rem,2.8cqw,0.82rem)] tracking-[0.15em] text-(--obs-text-dim) uppercase [@container(max-height:42rem)]:text-[clamp(0.58rem,2cqw,0.72rem)]"
		class:glass-detail={variant === 'mission-glass'}
		class:signal-detail={variant === 'signal-band'}
		class:invisible={!presentation.showDetail || !verified}
		aria-hidden={!presentation.showDetail || !verified}
	>
		{verified ? presentation.detail : '...'}
	</p>
{/snippet}

{#snippet runTimes()}
	<div
		class="monitor-metrics grid grid-cols-3 font-mono [&_small]:text-(--obs-text-dim) [&_small]:uppercase [&_strong]:font-medium [&_strong]:[font-variant-numeric:tabular-nums] [&>span]:grid [&>span]:min-w-0 {variant ===
		'mission-glass'
			? 'glass-metrics mt-[clamp(1.5rem,5cqw,2.5rem)] gap-[clamp(0.7rem,4cqw,2rem)] border-t border-[color-mix(in_srgb,var(--monitor-accent)_25%,var(--obs-border-muted))] pt-[clamp(1.25rem,4cqw,2rem)] [&_small]:text-[0.65rem] [&_small]:tracking-[0.12em] [&_strong]:text-[clamp(1.25rem,6cqw,2.6rem)] [&>span]:gap-[0.2rem] [@container(max-height:42rem)]:mt-[clamp(0.45rem,2cqw,0.7rem)] [@container(max-height:42rem)]:pt-[clamp(0.4rem,2cqw,0.65rem)] [@container(max-height:42rem)]:[&_strong]:text-[clamp(0.75rem,3cqw,1rem)] [@container(max-height:58rem)]:mt-[clamp(0.65rem,2.5cqw,1rem)] [@container(max-height:58rem)]:gap-[clamp(0.5rem,2cqw,1rem)] [@container(max-height:58rem)]:pt-[clamp(0.55rem,2.5cqw,0.9rem)] [@container(max-height:58rem)]:[&_small]:text-[0.55rem] [@container(max-height:58rem)]:[&_strong]:text-[clamp(0.85rem,3.2cqw,1.35rem)]'
			: 'signal-metrics mb-[clamp(1.25rem,4cqh,2.5rem)] animate-signal-metrics gap-[clamp(0.75rem,4cqw,2.5rem)] motion-reduce:[animation-delay:0ms] motion-reduce:[animation-duration:1ms] @max-[520px]:mb-4 [&_small]:text-[0.65rem] [&_small]:tracking-[0.14em] [&_strong]:text-[clamp(1.35rem,7cqw,3rem)] [&>span]:gap-1 [@container(max-height:42rem)]:mb-[0.7rem] [@container(max-height:42rem)]:gap-[clamp(0.6rem,2.5cqw,1.5rem)] [@container(max-height:42rem)]:[&_small]:text-[0.58rem] [@container(max-height:42rem)]:[&_strong]:text-[clamp(1.1rem,4.5cqw,2rem)]'}"
		aria-label="Run times"
	>
		<span class:text-(--obs-text-dim)={match?.times?.time == null} data-available={match?.times?.time != null}>
			<small>time</small>
			<strong>{match?.times?.time == null ? '--:--' : formatMonitorTime(match.times.time)}</strong>
		</span>
		<span
			class:text-(--obs-text-dim)={match?.times?.target_time == null}
			data-available={match?.times?.target_time != null}
		>
			<small>target</small>
			<strong>{match?.times?.target_time == null ? '--:--' : formatMonitorTime(match.times.target_time)}</strong>
		</span>
		<span
			class:text-(--obs-text-dim)={match?.times?.best_time == null}
			data-available={match?.times?.best_time != null}
		>
			<small>best</small>
			<strong>{match?.times?.best_time == null ? '--:--' : formatMonitorTime(match.times.best_time)}</strong>
		</span>
	</div>
{/snippet}

{#if variant === 'mission-glass'}
	{#key presentation.animationKey}
		<section
			class="glass-panel relative w-full animate-glass-panel rounded-[clamp(1rem,4cqw,1.6rem)] border border-[color-mix(in_srgb,var(--monitor-accent)_38%,var(--obs-border-soft))] bg-[rgb(37_41_52_/_90%)] p-[clamp(1.25rem,4.5cqw,2.5rem)] text-center shadow-[0_1.5rem_5rem_rgb(0_0_0_/_35%),0_0_4rem_var(--monitor-surface),inset_0_1px_0_rgb(255_255_255_/_11%)] transition-[border-color,box-shadow] duration-240 motion-reduce:[animation-duration:1ms] @max-[520px]:p-[1.35rem] [@container(max-height:42rem)]:p-[clamp(0.65rem,2.5cqw,0.95rem)] [@container(max-height:58rem)]:p-[clamp(0.85rem,3cqw,1.25rem)]"
		>
			{@render statusText()}
			{@render runTimes()}
		</section>
	{/key}
{:else}
	{@render runTimes()}
	{#key presentation.animationKey}
		<section
			class="signal-content animate-signal-title motion-reduce:[animation-delay:0ms] motion-reduce:[animation-duration:1ms]"
		>
			{@render statusText()}
		</section>
	{/key}
{/if}
