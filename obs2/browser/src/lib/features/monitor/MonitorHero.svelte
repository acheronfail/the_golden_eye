<script lang="ts">
	import type { LevelMatch } from '$lib/api';
	import { formatMonitorTime, type MonitorPresentation } from './monitorView';

	let {
		verified,
		presentation,
		match = null,
		statsPosition,
		statusLabel,
		statusAvailable = true,
		panelClass = '',
		statusClass = '',
		titleClass = '',
		detailClass = '',
		hiddenDetailClass = '',
		metricsClass = '',
		unavailableMetricClass = ''
	}: {
		verified: boolean;
		presentation: MonitorPresentation;
		match?: LevelMatch | null;
		statsPosition: 'inside' | 'before';
		statusLabel: string;
		statusAvailable?: boolean;
		panelClass?: string;
		statusClass?: string;
		titleClass?: string;
		detailClass?: string;
		hiddenDetailClass?: string;
		metricsClass?: string;
		unavailableMetricClass?: string;
	} = $props();
</script>

{#snippet statusText()}
	<p class="{statusClass} {statusAvailable ? '' : 'opacity-45'}" data-available={statusAvailable}>
		{statusLabel}
	</p>
	<h1 class={titleClass}>
		{verified ? presentation.title : 'checking source'}
	</h1>
	<p
		class="{detailClass} {presentation.showDetail && verified ? '' : hiddenDetailClass}"
		aria-hidden={!presentation.showDetail || !verified}
	>
		{verified ? presentation.detail : '...'}
	</p>
{/snippet}

{#snippet runTimes()}
	<div class={metricsClass} aria-label="Run times">
		<span class={match?.times?.time == null ? unavailableMetricClass : ''} data-available={match?.times?.time != null}>
			<small>time</small>
			<strong>{match?.times?.time == null ? '--:--' : formatMonitorTime(match.times.time)}</strong>
		</span>
		<span
			class={match?.times?.target_time == null ? unavailableMetricClass : ''}
			data-available={match?.times?.target_time != null}
		>
			<small>target</small>
			<strong>{match?.times?.target_time == null ? '--:--' : formatMonitorTime(match.times.target_time)}</strong>
		</span>
		<span
			class={match?.times?.best_time == null ? unavailableMetricClass : ''}
			data-available={match?.times?.best_time != null}
		>
			<small>best</small>
			<strong>{match?.times?.best_time == null ? '--:--' : formatMonitorTime(match.times.best_time)}</strong>
		</span>
	</div>
{/snippet}

{#if statsPosition === 'inside'}
	{#key presentation.animationKey}
		<section class={panelClass}>
			{@render statusText()}
			{@render runTimes()}
		</section>
	{/key}
{:else}
	{@render runTimes()}
	{#key presentation.animationKey}
		<section class={panelClass}>
			{@render statusText()}
		</section>
	{/key}
{/if}
