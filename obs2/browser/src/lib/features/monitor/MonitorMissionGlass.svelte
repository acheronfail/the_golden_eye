<script lang="ts">
	import MonitorHero from './MonitorHero.svelte';
	import MonitorWallClockTimers from './MonitorWallClockTimers.svelte';
	import type { MonitorDesignProps } from './monitorView';
	import { monitorPresentation } from './monitorView';
	import RecentRuns from './RecentRuns.svelte';

	let {
		verified,
		monitoring,
		transition = null,
		recordingState = null,
		match = null,
		fps = null,
		showMonitorFps = false,
		recentRuns = [],
		recentRunsBusyId = null,
		recentRunsError = null,
		onKeepRun = () => {},
		onStop,
		wallClocks
	}: MonitorDesignProps = $props();

	const presentation = $derived(
		monitorPresentation({ verified, monitoring, transition, recordingState, match, fps, showMonitorFps, onStop })
	);
</script>

<main
	class="@container [container-type:size] relative isolate flex h-full min-h-0 items-center justify-center overflow-hidden bg-(--obs-bg) text-(--obs-text) [--monitor-accent:var(--obs-monitor-waiting)] [--monitor-surface:var(--obs-monitor-waiting-surface)] data-[phase=complete]:[--monitor-accent:var(--obs-gold-hover)] data-[phase=complete]:[--monitor-surface:var(--obs-gold-surface)] data-[phase=danger]:[--monitor-accent:var(--obs-danger)] data-[phase=danger]:[--monitor-surface:var(--obs-danger-surface)] data-[phase=neutral]:[--monitor-accent:var(--obs-text-muted)] data-[phase=neutral]:[--monitor-surface:rgb(182_186_196_/_11%)] data-[phase=recording]:[--monitor-accent:var(--obs-success)] data-[phase=recording]:[--monitor-surface:var(--obs-success-surface)] motion-reduce:duration-[1ms] motion-reduce:[&_*]:duration-[1ms]"
	data-phase={presentation.phase}
	aria-busy={presentation.waitingForObs || !verified}
	aria-live="polite"
>
	<div
		class="absolute inset-0 z-[-4] bg-[radial-gradient(circle_at_50%_48%,var(--monitor-surface),transparent_42%),linear-gradient(145deg,var(--monitor-surface),transparent_38%)] [transition:background_240ms_ease] after:absolute after:inset-0 after:bg-[linear-gradient(rgb(255_255_255_/_2%)_1px,transparent_1px),linear-gradient(90deg,rgb(255_255_255_/_2%)_1px,transparent_1px)] after:[mask-image:radial-gradient(circle,#000,transparent_72%)] after:bg-[size:3rem_3rem] after:content-['']"
		aria-hidden="true"
	></div>
	<div
		class="absolute inset-4 -z-1 rounded-[0.8rem] border border-[color-mix(in_srgb,var(--monitor-accent)_50%,transparent)] shadow-[inset_0_0_2.5rem_var(--monitor-surface)] transition-[border-color,box-shadow] duration-240"
		aria-hidden="true"
	></div>

	<header
		class="absolute top-6 right-8 left-8 z-5 flex items-center justify-between gap-4 font-mono text-[0.7rem] tracking-[0.12em] uppercase"
	>
		<span>
			<span
				class="mr-[0.3rem] inline-block h-[0.55rem] w-[0.55rem] rounded-full bg-(--monitor-accent) align-[-0.02rem] shadow-[0_0_0.9rem_var(--monitor-accent)] transition-[background-color,box-shadow] duration-240"
				aria-hidden="true"
			></span>
			LIVE MONITOR
		</span>
		<button
			type="button"
			class="obs-button min-h-10 obs-button-danger px-4 py-2 text-xs shadow-lg shadow-black/25"
			disabled={!monitoring || transition === 'stopping'}
			aria-label="Stop monitoring"
			onclick={onStop}
		>
			<span class="mr-[0.45rem] h-2 w-2 rounded-[1px] bg-current" aria-hidden="true"></span>
			{transition === 'stopping' ? 'stopping' : 'stop'}
		</button>
	</header>

	{#key presentation.animationKey}
		<div
			class="absolute -z-2 aspect-square w-[min(78cqw,34rem)] animate-reticle motion-reduce:[animation-duration:1ms] @max-[520px]:w-[92cqw]"
			aria-hidden="true"
		>
			<div
				class="absolute inset-0 rounded-full border border-[color-mix(in_srgb,var(--monitor-accent)_58%,transparent)] shadow-[0_0_5rem_var(--monitor-surface)] transition-[border-color,box-shadow] duration-240 before:absolute before:top-1/2 before:right-[-13%] before:left-[-13%] before:h-px before:bg-[color-mix(in_srgb,var(--monitor-accent)_36%,transparent)] before:content-[''] after:absolute after:top-[-13%] after:bottom-[-13%] after:left-1/2 after:w-px after:bg-[color-mix(in_srgb,var(--monitor-accent)_36%,transparent)] after:content-['']"
			>
				<span
					class="absolute inset-[14%] rounded-full border border-dashed border-[color-mix(in_srgb,var(--monitor-accent)_27%,transparent)]"
				></span>
			</div>
		</div>
	{/key}

	<div
		class="glass-layout relative z-2 grid h-[calc(100cqh-9rem)] min-h-0 w-[min(82cqw,42rem)] grid-rows-[auto_auto_minmax(0,1fr)] gap-[clamp(0.65rem,2cqh,1rem)] @max-[520px]:w-[calc(100cqw-3rem)]"
	>
		<MonitorWallClockTimers variant="mission-glass" {wallClocks} />
		<MonitorHero variant="mission-glass" {verified} {presentation} {match} />

		<RecentRuns
			variant="mission-glass"
			runs={recentRuns}
			busyRunId={recentRunsBusyId}
			error={recentRunsError}
			onKeep={onKeepRun}
		/>
	</div>

	<footer
		class="absolute right-6 bottom-6 left-8 z-5 flex items-center justify-between gap-4 font-mono text-[0.7rem] tracking-[0.12em] text-(--obs-text-dim) uppercase @max-[520px]:right-[1.1rem] @max-[520px]:left-8"
	>
		<span>{presentation.phase}</span>
		{#if monitoring}
			<span class="@max-[520px]:hidden">escape or space to stop</span>
		{/if}
		{#if monitoring && showMonitorFps && presentation.fpsText}
			<span class:text-amber-400={presentation.fpsWarning} class:text-(--obs-danger)={presentation.fpsLagging}>
				{presentation.fpsText}
			</span>
		{/if}
	</footer>
</main>
