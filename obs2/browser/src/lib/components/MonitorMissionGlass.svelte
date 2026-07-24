<script lang="ts">
	import type { MonitorViewProps } from './monitorView';
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
		onStop
	}: MonitorViewProps = $props();

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
		class="absolute top-6 right-6 left-6 z-5 flex items-center justify-between gap-4 font-mono text-[0.7rem] tracking-[0.12em] uppercase @max-[520px]:right-8 @max-[520px]:left-8"
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
		class="glass-layout relative z-2 grid max-h-[calc(100cqh-9rem)] w-[min(82cqw,42rem)] gap-[clamp(0.65rem,2cqh,1rem)] @max-[520px]:w-[calc(100cqw-3rem)]"
	>
		{#key presentation.animationKey}
			<section
				class="glass-panel relative w-full animate-glass-panel rounded-[clamp(1rem,4cqw,1.6rem)] border border-[color-mix(in_srgb,var(--monitor-accent)_38%,var(--obs-border-soft))] bg-[rgb(37_41_52_/_90%)] p-[clamp(1.25rem,4.5cqw,2.5rem)] text-center shadow-[0_1.5rem_5rem_rgb(0_0_0_/_35%),0_0_4rem_var(--monitor-surface),inset_0_1px_0_rgb(255_255_255_/_11%)] transition-[border-color,box-shadow] duration-240 motion-reduce:[animation-duration:1ms] @max-[520px]:p-[1.35rem]"
			>
				<p
					class="font-mono text-[clamp(0.65rem,2.8cqw,0.82rem)] tracking-[0.15em] text-(--monitor-accent) uppercase transition-colors duration-240"
				>
					{verified ? presentation.statusLabel : 'Verifying source'}
				</p>
				<h1
					class="mt-[0.55rem] mb-[0.7rem] text-[clamp(2.25rem,11cqw,5rem)] leading-[0.92] font-semibold tracking-[-0.065em] [overflow-wrap:anywhere] text-[color-mix(in_srgb,var(--monitor-accent)_12%,var(--obs-text))] transition-colors duration-240"
				>
					{verified ? presentation.title : 'checking source'}
				</h1>
				<p
					class="glass-detail font-mono text-[clamp(0.65rem,2.8cqw,0.82rem)] tracking-[0.15em] text-(--obs-text-dim) uppercase"
					class:invisible={!presentation.showDetail || !verified}
					aria-hidden={!presentation.showDetail || !verified}
				>
					{verified ? presentation.detail : '...'}
				</p>

				{#if match?.times && !presentation.waitingForObs}
					<div
						class="mt-[clamp(1.5rem,5cqw,2.5rem)] grid grid-cols-3 gap-[clamp(0.7rem,4cqw,2rem)] border-t border-[color-mix(in_srgb,var(--monitor-accent)_25%,var(--obs-border-muted))] pt-[clamp(1.25rem,4cqw,2rem)] font-mono [&_small]:text-[0.65rem] [&_small]:tracking-[0.12em] [&_small]:text-(--obs-text-dim) [&_small]:uppercase [&_strong]:text-[clamp(1.25rem,6cqw,2.6rem)] [&_strong]:font-medium [&_strong]:[font-variant-numeric:tabular-nums] [&>span]:grid [&>span]:min-w-0 [&>span]:gap-[0.2rem]"
						aria-label="Run times"
					>
						<span>
							<small>time</small>
							<strong>{formatMonitorTime(match.times.time)}</strong>
						</span>
						{#if match.times.target_time != null}
							<span>
								<small>target</small>
								<strong>{formatMonitorTime(match.times.target_time)}</strong>
							</span>
						{/if}
						{#if match.times.best_time != null}
							<span>
								<small>best</small>
								<strong>{formatMonitorTime(match.times.best_time)}</strong>
							</span>
						{/if}
					</div>
				{/if}
			</section>
		{/key}

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
