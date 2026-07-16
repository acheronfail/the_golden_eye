<script lang="ts">
	import { afterNavigate, goto } from '$app/navigation';
	import { page } from '$app/state';
	import { startMonitor as apiStartMonitor, stopMonitor as apiStopMonitor, type MonitorRunMode } from '$lib/api';
	import { settings } from '$lib';
	import { monitor, monitorPhaseStyle, monitorHref } from '$lib/monitor.svelte';
	import { refreshReplayBuffer } from '$lib/replayBuffer.svelte';
	import { obsSources } from '$lib/sources.svelte';
	import StopMonitorDialog from '$lib/StopMonitorDialog.svelte';
	import SingleSegmentSplits from '$lib/SingleSegmentSplits.svelte';

	let {
		sourceName,
		mode = 'clips',
		difficulty,
		title: modeTitle = null
	}: {
		sourceName: string;
		mode?: MonitorRunMode;
		difficulty?: number;
		title?: string | null;
	} = $props();

	let monitoring = $state(false);
	let verified = $state(false);
	let statusChecked = $state(false);
	let transition = $state<'starting' | 'stopping' | null>(null);
	let pendingNavigation = $state<string | null>(null);
	let confirmStop = $state(false);
	const obsTransitionStyle = {
		title: 'waiting for OBS',
		border: 'obs-phase-neutral-border',
		heading: 'obs-phase-neutral-text',
		tag: 'obs-phase-neutral-text',
		button: 'obs-phase-neutral-button',
		dot: 'obs-phase-neutral-dot'
	};

	const isSingleSegment = $derived(mode !== 'clips');
	const expectedPath = $derived(
		mode === 'clips'
			? `/sources/${encodeURIComponent(sourceName)}/monitor`
			: page.url.pathname
	);
	const isCurrentPage = $derived(page.url.pathname === expectedPath);
	const sourceExists = $derived((obsSources.items ?? []).some((source) => source.name === sourceName));
	const waitingForObs = $derived(transition !== null);
	const currentMatch = $derived(monitor.match);
	const currentTimes = $derived(monitor.match?.times ?? null);
	const style = $derived(waitingForObs ? obsTransitionStyle : monitorPhaseStyle(monitor.recordingState));
	const statusLabel = $derived(
		transition === 'starting' ? 'Starting monitor' : transition === 'stopping' ? 'Stopping monitor' : 'Monitoring'
	);
	const displayTitle = $derived(isSingleSegment ? (modeTitle ?? monitor.singleSegment.category?.title ?? 'Single segment') : waitingForObs ? 'waiting for OBS' : style.title);
	const detail = $derived(
		transition === 'starting'
			? 'replay buffer is stopping or starting'
			: transition === 'stopping'
				? 'stopping monitor'
				: (currentMatch?.screen ?? '...')
	);
	const showDetail = $derived(waitingForObs || detail.trim().toLowerCase() !== 'unknown');
	const monitorFpsText = $derived(
		monitor.fps
			? monitor.fps.sourceFps > 0
				? `${monitor.fps.processedFps.toFixed(1)} / ${monitor.fps.sourceFps.toFixed(1)} FPS`
				: `${monitor.fps.processedFps.toFixed(1)} FPS`
			: null
	);
	const monitorFpsLagging = $derived(
		Boolean(monitor.fps && monitor.fps.sourceFps > 0 && monitor.fps.processedFps + 0.5 < monitor.fps.sourceFps)
	);

	const formatTime = (secs: number): string => {
		const m = Math.floor(secs / 60);
		const s = secs % 60;
		return `${m}:${s.toString().padStart(2, '0')}`;
	};

	const navigate = (href: string, options: { replaceState?: boolean } = {}) => {
		if (page.url.pathname === href || pendingNavigation === href) return;
		pendingNavigation = href;
		void goto(href, options);
	};

	afterNavigate(async () => {
		pendingNavigation = null;
		if (!isCurrentPage) return;

		verified = false;
		statusChecked = false;
		const status = monitor.status;
		if (status?.enabled) {
			if (status.sourceName !== sourceName || status.mode !== mode) {
				const href = monitorHref(status);
				if (href) navigate(href, { replaceState: true });
				return;
			}
			monitoring = true;
		} else {
			monitoring = false;
		}
		statusChecked = true;
	});

	$effect(() => {
		if (!isCurrentPage) return;
		if (!statusChecked) return;
		if (monitoring) {
			verified = true;
			return;
		}
		if (!obsSources.loaded) {
			verified = false;
			return;
		}
		if (sourceExists) {
			verified = true;
		} else {
			navigate('/', { replaceState: true });
		}
	});

	$effect(() => {
		if (!isCurrentPage) return;
		if (!statusChecked || monitoring || transition || pendingNavigation || !verified) return;
		void startMonitor();
	});

	$effect(() => {
		if (!isCurrentPage) return;
		if (!statusChecked || transition) return;
		if (monitoring && monitor.status?.enabled === false) {
			monitoring = false;
			navigate('/', { replaceState: true });
		}
	});

	$effect(() => {
		if (!isCurrentPage) return;
		if (!monitor.status?.enabled) return;
		if (monitor.status.sourceName !== sourceName || monitor.status.mode !== mode) {
			const href = monitorHref(monitor.status);
			if (href) navigate(href, { replaceState: true });
		}
	});

	const startMonitor = async () => {
		if (monitoring || transition || pendingNavigation) return;
		transition = 'starting';
		try {
			await settings.saveNow();
			await apiStartMonitor(sourceName, { mode, difficulty });
			void refreshReplayBuffer();
			monitoring = true;
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
			navigate('/', { replaceState: true });
		} finally {
			transition = null;
		}
	};

	const requestStopMonitor = () => {
		if (transition) return;
		if (isSingleSegment) {
			confirmStop = true;
			return;
		}
		void stopMonitor();
	};

	const stopMonitor = async () => {
		if (transition) return;
		transition = 'stopping';
		try {
			await apiStopMonitor();
			void refreshReplayBuffer();
			monitoring = false;
			confirmStop = false;
			navigate('/', { replaceState: true });
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
		} finally {
			transition = null;
		}
	};

	const onkeydown = (event: KeyboardEvent) => {
		if (transition || !monitoring) return;
		if (event.key === ' ' || event.key === 'Escape') {
			event.preventDefault();
			requestStopMonitor();
		}
	};
</script>

<svelte:window {onkeydown} />

<main
	class="relative flex h-full min-h-0 flex-col items-center overflow-hidden px-6 py-12 text-center {isSingleSegment
		? 'justify-between'
		: 'justify-center'}"
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
				onclick={requestStopMonitor}
			>
				{transition === 'stopping' ? 'stopping monitor' : 'stop monitor'}
			</button>
			<p class="obs-subtitle mt-2 text-xs whitespace-nowrap">
				{waitingForObs ? 'OBS is finishing the replay buffer transition' : `press escape or space to ${isSingleSegment ? 'confirm stop' : 'stop monitoring'}`}
			</p>
		</div>
	{/if}

	{#if monitoring && settings.showMonitorFps && monitorFpsText}
		<div
			class="absolute right-6 bottom-6 z-20 font-mono text-xs whitespace-nowrap tabular-nums {monitorFpsLagging
				? 'text-(--obs-danger)'
				: 'obs-dim'}"
			aria-label={monitorFpsLagging ? 'Monitor FPS is below OBS FPS' : 'Monitor FPS'}
		>
			{monitorFpsText}
		</div>
	{/if}

	<div class="mt-20 {isSingleSegment ? 'text-sm' : ''}">
		<p class="font-mono text-xs tracking-widest {style.tag} uppercase">
			{verified ? statusLabel : 'Verifying source'}
		</p>
		<h1 class="mt-4 font-semibold wrap-break-word {style.heading} {isSingleSegment ? 'text-3xl' : 'text-6xl'}">
			{verified ? displayTitle : 'checking source'}
		</h1>
		{#if showDetail && verified}
			<p class="obs-dim mt-3 font-mono text-xs tracking-widest uppercase">
				{detail}
			</p>
		{/if}

		{#if currentTimes && !waitingForObs && !isSingleSegment}
			<div class="mt-6 flex flex-wrap justify-center gap-6 font-mono">
				<span class="flex flex-col items-center">
					<span class="obs-dim text-xs tracking-widest uppercase">time</span>
					<span class="text-4xl">{formatTime(currentTimes.time)}</span>
				</span>
				{#if currentTimes.target_time != null}
					<span class="flex flex-col items-center">
						<span class="obs-dim text-xs tracking-widest uppercase">target</span>
						<span class="text-4xl">{formatTime(currentTimes.target_time)}</span>
					</span>
				{/if}
				{#if currentTimes.best_time != null}
					<span class="flex flex-col items-center">
						<span class="obs-dim text-xs tracking-widest uppercase">best</span>
						<span class="text-4xl">{formatTime(currentTimes.best_time)}</span>
					</span>
				{/if}
			</div>
		{/if}
	</div>

	{#if isSingleSegment}
		<SingleSegmentSplits snapshot={monitor.singleSegment} />
	{/if}
</main>

<StopMonitorDialog
	open={confirmStop}
	busy={transition === 'stopping'}
	modeTitle={modeTitle ?? monitor.singleSegment.category?.title ?? 'single segment run'}
	close={() => (confirmStop = false)}
	confirm={stopMonitor}
/>
