<script lang="ts">
	import type { LevelMatch, MonitorFps, RecordingStatus } from '$lib/api';
	import { monitorPhaseStyle } from '$lib/stores/monitor.svelte';

	export type MonitorTransition = 'starting' | 'stopping' | null;

	let {
		verified,
		monitoring,
		transition = null,
		recordingState = null,
		match = null,
		fps = null,
		showMonitorFps = false,
		onStop
	}: {
		verified: boolean;
		monitoring: boolean;
		transition?: MonitorTransition;
		recordingState?: RecordingStatus | null;
		match?: LevelMatch | null;
		fps?: MonitorFps | null;
		showMonitorFps?: boolean;
		onStop: () => void;
	} = $props();

	const obsTransitionStyle = {
		title: 'waiting for OBS',
		border: 'obs-phase-neutral-border',
		heading: 'obs-phase-neutral-text',
		tag: 'obs-phase-neutral-text',
		button: 'obs-phase-neutral-button',
		dot: 'obs-phase-neutral-dot'
	};

	const waitingForObs = $derived(transition !== null);
	const style = $derived(waitingForObs ? obsTransitionStyle : monitorPhaseStyle(recordingState));
	const statusLabel = $derived(
		transition === 'starting' ? 'Starting monitor' : transition === 'stopping' ? 'Stopping monitor' : 'Monitoring'
	);
	const title = $derived(waitingForObs ? 'waiting for OBS' : style.title);
	const detail = $derived(
		transition === 'starting'
			? 'replay buffer is stopping or starting'
			: transition === 'stopping'
				? 'stopping monitor'
				: (match?.screen ?? '...')
	);
	const showDetail = $derived(waitingForObs || detail.trim().toLowerCase() !== 'unknown');
	const monitorFpsText = $derived(
		fps
			? fps.sourceFps > 0
				? `${fps.processedFps.toFixed(1)} / ${fps.sourceFps.toFixed(1)} FPS`
				: `${fps.processedFps.toFixed(1)} FPS`
			: null
	);
	const monitorFpsLagging = $derived(Boolean(fps && fps.sourceFps > 0 && fps.processedFps + 0.5 < fps.sourceFps));

	const formatTime = (secs: number): string => {
		const m = Math.floor(secs / 60);
		const s = secs % 60;
		return `${m}:${s.toString().padStart(2, '0')}`;
	};
</script>

<main
	class="relative flex h-full min-h-0 flex-col items-center justify-center overflow-hidden px-6 py-12 text-center"
	aria-busy={waitingForObs || !verified}
	aria-live="polite"
>
	<div class="pointer-events-none absolute inset-0 z-10 border-8 {style.border}"></div>

	{#if monitoring}
		<div class="absolute top-6 left-1/2 z-20 flex -translate-x-1/2 flex-col items-center">
			<button
				type="button"
				class="obs-button obs-button-danger min-h-11 px-5 py-2 text-sm shadow-lg shadow-black/25"
				disabled={transition === 'stopping'}
				aria-label="Stop monitoring"
				onclick={onStop}
			>
				{transition === 'stopping' ? 'stopping monitor' : 'stop monitor'}
			</button>
			<p class="obs-subtitle mt-2 text-xs whitespace-nowrap">
				{waitingForObs ? 'OBS is finishing the replay buffer transition' : 'press escape or space to stop monitoring'}
			</p>
		</div>
	{/if}

	{#if monitoring && showMonitorFps && monitorFpsText}
		<div
			class="absolute right-6 bottom-6 z-20 font-mono text-xs whitespace-nowrap tabular-nums {monitorFpsLagging
				? 'text-(--obs-danger)'
				: 'obs-dim'}"
			aria-label={monitorFpsLagging ? 'Monitor FPS is below OBS FPS' : 'Monitor FPS'}
		>
			{monitorFpsText}
		</div>
	{/if}

	<p class="font-mono text-xs tracking-widest {style.tag} uppercase">
		{verified ? statusLabel : 'Verifying source'}
	</p>
	<h1 class="mt-4 text-6xl font-semibold wrap-break-word {style.heading}">
		{verified ? title : 'checking source'}
	</h1>
	{#if showDetail && verified}
		<p class="obs-dim mt-3 font-mono text-xs tracking-widest uppercase">
			{detail}
		</p>
	{/if}

	{#if match?.times && !waitingForObs}
		<div class="mt-6 flex flex-wrap justify-center gap-6 font-mono">
			<span class="flex flex-col items-center">
				<span class="obs-dim text-xs tracking-widest uppercase">time</span>
				<span class="text-4xl">{formatTime(match.times.time)}</span>
			</span>
			{#if match.times.target_time != null}
				<span class="flex flex-col items-center">
					<span class="obs-dim text-xs tracking-widest uppercase">target</span>
					<span class="text-4xl">{formatTime(match.times.target_time)}</span>
				</span>
			{/if}
			{#if match.times.best_time != null}
				<span class="flex flex-col items-center">
					<span class="obs-dim text-xs tracking-widest uppercase">best</span>
					<span class="text-4xl">{formatTime(match.times.best_time)}</span>
				</span>
			{/if}
		</div>
	{/if}
</main>
