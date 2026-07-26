<script lang="ts">
	import { formatWallClockTime } from './monitorWallClocks.svelte';
	import type { MonitorDesignProps } from './monitorView';
	import { formatMonitorTime, monitorPresentation } from './monitorView';
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
	const levelTimerHelp =
		"Starts when the start screen disappears. It measures elapsed wall-clock time, not the game's reported time.";
</script>

<main
	class="@container [container-type:size] relative isolate flex h-full min-h-0 overflow-hidden bg-(--obs-bg) text-(--obs-text) transition-colors duration-240 [--monitor-accent:var(--obs-monitor-waiting)] [--monitor-surface:var(--obs-monitor-waiting-surface)] data-[phase=complete]:[--monitor-accent:var(--obs-gold-hover)] data-[phase=complete]:[--monitor-surface:var(--obs-gold-surface)] data-[phase=danger]:[--monitor-accent:var(--obs-danger)] data-[phase=danger]:[--monitor-surface:var(--obs-danger-surface)] data-[phase=neutral]:[--monitor-accent:var(--obs-text-muted)] data-[phase=neutral]:[--monitor-surface:rgb(182_186_196_/_11%)] data-[phase=recording]:[--monitor-accent:var(--obs-success)] data-[phase=recording]:[--monitor-surface:var(--obs-success-surface)] motion-reduce:duration-[1ms] motion-reduce:[&_*]:duration-[1ms]"
	data-phase={presentation.phase}
	aria-busy={presentation.waitingForObs || !verified}
	aria-live="polite"
>
	<div
		class="absolute inset-0 -z-3 bg-[linear-gradient(115deg,var(--monitor-surface),transparent_44%),radial-gradient(circle_at_22%_54%,var(--monitor-surface),transparent_45%)] [transition:background_240ms_ease] after:absolute after:inset-0 after:bg-[linear-gradient(rgb(255_255_255_/_2%)_1px,transparent_1px)] after:[mask-image:linear-gradient(to_bottom,transparent,#000_30%,#000_70%,transparent)] after:bg-[size:100%_4px] after:content-['']"
		aria-hidden="true"
	></div>
	<div
		class="absolute top-4 bottom-4 left-4 -z-1 w-[0.3rem] overflow-hidden rounded-full bg-[color-mix(in_srgb,var(--monitor-accent)_22%,var(--obs-border))] shadow-[0_0_2rem_var(--monitor-surface)] transition-[background-color,box-shadow] duration-240 [&>span]:block [&>span]:h-[42%] [&>span]:w-full [&>span]:bg-(--monitor-accent) [&>span]:shadow-[0_0_1.1rem_var(--monitor-accent)] [&>span]:transition-[background-color,box-shadow] [&>span]:duration-240"
		aria-hidden="true"
	>
		<span></span>
	</div>

	<header
		class="absolute top-6 right-6 left-8 z-4 flex items-center justify-between gap-4 font-mono text-[0.7rem] tracking-[0.12em] uppercase @max-[520px]:right-8 @max-[520px]:left-8"
	>
		<span class="whitespace-nowrap">
			<span
				class="mr-[0.3rem] inline-block h-[0.55rem] w-[0.55rem] rounded-full bg-(--monitor-accent) align-[-0.02rem] shadow-[0_0_0.9rem_var(--monitor-accent)] transition-[background-color,box-shadow] duration-240"
				aria-hidden="true"
			></span>
			<span class="@max-[520px]:hidden">GOLDENEYE MONITOR</span>
			<span class="hidden @max-[520px]:inline">GE MONITOR</span>
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
			class="pointer-events-none absolute inset-0 -z-2 animate-signal-wipe bg-[linear-gradient(90deg,transparent,var(--monitor-surface),transparent)] motion-reduce:[animation-delay:0ms] motion-reduce:[animation-duration:1ms]"
			aria-hidden="true"
		></div>
	{/key}

	<div
		class="signal-layout absolute top-20 right-[clamp(1.75rem,7cqw,5rem)] bottom-15 left-[clamp(2.5rem,9cqw,7rem)] grid grid-cols-[minmax(0,1.35fr)_minmax(15rem,0.65fr)] items-center gap-[clamp(1.5rem,5cqw,4rem)] @max-[760px]:grid-cols-[minmax(0,1fr)] @max-[760px]:grid-rows-[auto_minmax(0,1fr)] @max-[760px]:items-start @max-[760px]:gap-3 [@container(max-height:42rem)]:top-18 [@container(max-height:42rem)]:bottom-13 [@container(max-height:42rem)]:gap-[0.6rem]"
	>
		<div class="min-w-0">
			<div
				class="mb-[clamp(1rem,3cqh,2rem)] grid max-w-[34rem] grid-cols-2 gap-[clamp(1rem,5cqw,3rem)] font-mono [&_small]:text-[clamp(0.54rem,2cqw,0.68rem)] [&_small]:tracking-[0.14em] [&_small]:uppercase [&_strong]:text-[clamp(1.15rem,5cqw,2rem)] [&_strong]:font-medium [&_strong]:tracking-[-0.04em] [&_strong]:[font-variant-numeric:tabular-nums] [&>div]:grid [&>div]:gap-0.5 [@container(max-height:42rem)]:mb-2 [@container(max-height:42rem)]:[&_strong]:text-[clamp(1rem,4cqw,1.45rem)]"
				aria-label="Wall-clock timers"
				aria-live="off"
				role="group"
			>
				<div class:text-(--obs-text-dim)={!wallClocks.sessionRunning} data-running={wallClocks.sessionRunning}>
					<small class:text-(--monitor-accent)={wallClocks.sessionRunning}>Time in session</small>
					<strong>{formatWallClockTime(wallClocks.sessionElapsedMs)}</strong>
				</div>
				<div class:text-(--obs-text-dim)={!wallClocks.levelRunning} data-running={wallClocks.levelRunning}>
					<small
						class="w-fit cursor-help"
						class:text-(--monitor-accent)={wallClocks.levelRunning}
						title={levelTimerHelp}
						aria-label={`Time in level. ${levelTimerHelp}`}>Time in level</small
					>
					<strong>{formatWallClockTime(wallClocks.levelElapsedMs)}</strong>
				</div>
			</div>

			<div
				class="signal-metrics mb-[clamp(1.25rem,4cqh,2.5rem)] grid animate-signal-metrics grid-cols-3 gap-[clamp(0.75rem,4cqw,2.5rem)] font-mono motion-reduce:[animation-delay:0ms] motion-reduce:[animation-duration:1ms] @max-[520px]:mb-4 [&_small]:text-[0.65rem] [&_small]:tracking-[0.14em] [&_small]:text-(--obs-text-dim) [&_small]:uppercase [&_strong]:text-[clamp(1.35rem,7cqw,3rem)] [&_strong]:font-medium [&_strong]:[font-variant-numeric:tabular-nums] [&>span]:grid [&>span]:min-w-0 [&>span]:gap-1 [@container(max-height:42rem)]:mb-[0.7rem] [@container(max-height:42rem)]:gap-[clamp(0.6rem,2.5cqw,1.5rem)] [@container(max-height:42rem)]:[&_small]:text-[0.58rem] [@container(max-height:42rem)]:[&_strong]:text-[clamp(1.1rem,4.5cqw,2rem)]"
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

			{#key presentation.animationKey}
				<section
					class="signal-content animate-signal-title motion-reduce:[animation-delay:0ms] motion-reduce:[animation-duration:1ms]"
				>
					<p
						class="font-mono text-[clamp(0.65rem,2.8cqw,0.82rem)] tracking-[0.15em] text-(--monitor-accent) uppercase transition-colors duration-240 [@container(max-height:42rem)]:text-[clamp(0.58rem,2cqw,0.72rem)]"
					>
						{verified ? presentation.statusLabel : 'Verifying source'} / ACTIVE
					</p>
					<h1
						class="mt-2 mb-[0.65rem] max-w-full text-[clamp(2.4rem,11cqw,5.25rem)] leading-[0.9] font-semibold tracking-[-0.065em] [overflow-wrap:anywhere] text-[color-mix(in_srgb,var(--monitor-accent)_12%,var(--obs-text))] transition-colors duration-240 [@container(max-height:42rem)]:mt-[0.3rem] [@container(max-height:42rem)]:mb-[0.4rem] [@container(max-height:42rem)]:text-[clamp(2rem,8cqw,3.5rem)]"
					>
						{verified ? presentation.title : 'checking source'}
					</h1>
					<p
						class="signal-detail font-mono text-[clamp(0.65rem,2.8cqw,0.82rem)] tracking-[0.15em] text-(--obs-text-dim) uppercase [@container(max-height:42rem)]:text-[clamp(0.58rem,2cqw,0.72rem)]"
						class:invisible={!presentation.showDetail || !verified}
						aria-hidden={!presentation.showDetail || !verified}
					>
						{verified ? presentation.detail : '...'}
					</p>
				</section>
			{/key}
		</div>

		<RecentRuns
			variant="signal-band"
			runs={recentRuns}
			busyRunId={recentRunsBusyId}
			error={recentRunsError}
			onKeep={onKeepRun}
		/>
	</div>

	<footer
		class="absolute right-6 bottom-6 left-8 z-4 flex items-center justify-between gap-4 font-mono text-[0.7rem] tracking-[0.12em] text-(--obs-text-dim) uppercase @max-[520px]:right-4 @max-[520px]:left-7"
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
