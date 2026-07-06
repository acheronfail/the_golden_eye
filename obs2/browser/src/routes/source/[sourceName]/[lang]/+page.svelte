<script lang="ts">
	import { afterNavigate, goto } from '$app/navigation';
	import { startMonitor as apiStartMonitor, stopMonitor as apiStopMonitor } from '$lib/api';
	import { settings } from '$lib';
	import {
		monitor,
		monitorPhaseStyle,
		refreshMonitor,
		setMonitorRunning,
		setMonitorStopped
	} from '$lib/monitor.svelte';
	import { refreshReplayBuffer } from '$lib/replayBuffer.svelte';
	import { obsSources } from '$lib/sources.svelte';
	import WizardFrame from '$lib/wizard/WizardFrame.svelte';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();

	let monitoring = $state(false);
	let transition = $state<'starting' | 'stopping' | null>(null);
	const obsTransitionStyle = {
		title: 'waiting for OBS',
		border: 'obs-phase-neutral-border',
		heading: 'obs-phase-neutral-text',
		tag: 'obs-phase-neutral-text',
		button: 'obs-phase-neutral-button',
		dot: 'obs-phase-neutral-dot'
	};

	const waitingForObs = $derived(transition !== null);
	const currentMatch = $derived(monitor.match);
	const currentTimes = $derived(monitor.match?.times ?? null);
	const style = $derived(waitingForObs ? obsTransitionStyle : monitorPhaseStyle(monitor.recordingState));
	const statusLabel = $derived(
		transition === 'starting' ? 'Starting monitor' : transition === 'stopping' ? 'Stopping monitor' : 'Monitoring'
	);
	const title = $derived(waitingForObs ? 'waiting for OBS' : style.title);
	const detail = $derived(
		transition === 'starting'
			? 'replay buffer is stopping or starting'
			: transition === 'stopping'
				? 'stopping monitor'
				: (currentMatch?.screen ?? '...')
	);
	const showDetail = $derived(waitingForObs || detail.trim().toLowerCase() !== 'unknown');

	// Format a level time (whole seconds) as m:ss for the stats overlay readout.
	const formatTime = (secs: number): string => {
		const m = Math.floor(secs / 60);
		const s = secs % 60;
		return `${m}:${s.toString().padStart(2, '0')}`;
	};

	// The source name comes from the URL, so it may be stale if the user navigated
	// here from browser history / a manual URL, or renamed the source in OBS. Verify
	// it still exists before letting them monitor; the start button stays disabled
	// until this completes, and a failed check routes back home to restart the flow.
	let verified = $state(false);
	let statusChecked = $state(false);
	const sourceExists = $derived((obsSources.items ?? []).some((source) => source.name === params.sourceName));

	// Runs on initial load and after every navigation. A redirect below targets
	// this same route, so the component instance is reused and `onMount` wouldn't
	// fire again -- `afterNavigate` re-runs the restore each time the params change.
	afterNavigate(async () => {
		verified = false;
		statusChecked = false;
		try {
			// If a monitor is already running, restore that state rather than
			// defaulting to idle. When it's running for a different source/lang than
			// this URL describes, redirect to the page that matches reality.
			const status = await refreshMonitor();
			if (status.enabled) {
				if (status.sourceName !== params.sourceName || status.lang !== params.lang) {
					goto(`/source/${encodeURIComponent(status.sourceName)}/${status.lang}`);
					return;
				}
				monitoring = true;
			} else {
				monitoring = false;
			}
			statusChecked = true;
		} catch {
			goto('/');
		}
	});

	$effect(() => {
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
			goto('/');
		}
	});

	$effect(() => {
		if (!transition && monitor.status?.enabled === false) {
			monitoring = false;
		}
	});

	// Guards against a double start: the window Space handler and the OptionList
	// button's own keyboard activation can both fire for one keypress.
	const startMonitor = async () => {
		if (monitoring || transition) return;
		transition = 'starting';
		try {
			await settings.saveNow();
			await apiStartMonitor(params.sourceName, params.lang);
			setMonitorRunning(params.sourceName, params.lang);
			void refreshReplayBuffer();
			monitoring = true;
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
		} finally {
			transition = null;
		}
	};

	const stopMonitor = async () => {
		if (transition) return;
		transition = 'stopping';
		try {
			await apiStopMonitor();
			setMonitorStopped();
			void refreshReplayBuffer();
			monitoring = false;
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
		} finally {
			transition = null;
		}
	};

	const option: Option = {
		title: 'start monitor',
		detail: 'or press space to start monitoring'
	};

	// Space toggles monitoring from anywhere on the page (without relying on the
	// OptionList button holding focus); Escape also stops it. `startMonitor` is
	// idempotent, so it's harmless if the button's native activation also fires.
	const onkeydown = (event: KeyboardEvent) => {
		if (transition) return;
		if (monitoring) {
			if (event.key === ' ' || event.key === 'Escape') {
				event.preventDefault();
				stopMonitor();
			}
		} else if (event.key === ' ' && verified) {
			event.preventDefault();
			startMonitor();
		}
	};
</script>

<svelte:head>
	<title>Monitor | {params.lang} | {params.sourceName}</title>
</svelte:head>

<svelte:window {onkeydown} />

{#if monitoring || transition}
	<main
		class="relative flex h-full min-h-0 flex-col items-center justify-center overflow-hidden px-6 py-12 text-center"
		aria-busy={waitingForObs}
		aria-live="polite"
	>
		<!-- A thick border frames the page area (below the header) to signal
		     monitoring is live; its colour tracks the recorder state so the state
		     reads even from peripheral vision. -->
		<div class="pointer-events-none absolute inset-0 z-10 border-8 {style.border}"></div>

		{#if monitoring}
			<div class="absolute top-6 left-1/2 z-20 flex -translate-x-1/2 flex-col items-center">
				<button
					type="button"
					class="obs-button obs-button-danger min-h-11 px-5 py-2 text-sm shadow-lg shadow-black/25"
					disabled={transition === 'stopping'}
					aria-label="Stop monitoring"
					onclick={stopMonitor}
				>
					{transition === 'stopping' ? 'stopping monitor' : 'stop monitor'}
				</button>
				<p class="obs-subtitle mt-2 text-xs whitespace-nowrap">
					{waitingForObs ? 'OBS is finishing the replay buffer transition' : 'press escape or space to stop monitoring'}
				</p>
			</div>
		{/if}

		<p class="font-mono text-xs tracking-widest {style.tag} uppercase">
			{statusLabel}
			<span>({params.lang})</span>
		</p>
		<!-- The big centered title is the recorder state, in words, colour-matched
		     to the border. -->
		<h1 class="mt-4 text-6xl font-semibold wrap-break-word {style.heading}">
			{title}
		</h1>
		{#if showDetail}
			<!-- The raw matched screen, for detail beneath the plain-language title. -->
			<p class="obs-dim mt-3 font-mono text-xs tracking-widest uppercase">
				{detail}
			</p>
		{/if}

		{#if currentTimes && !waitingForObs}
			<!-- Stats overlay is on screen (e.g. while a save is pending): surface the
			     matched times so they're visible on the page. The run time is always
			     present; the target and best times only appear when the game shows
			     them (see the `times` docs in api.ts). -->
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
	</main>
{:else}
	<WizardFrame
		step={3}
		title="Ready to monitor"
		subtitle="{params.lang} | {params.sourceName}"
		hrefs={['/', `/source/${encodeURIComponent(params.sourceName)}`]}
	>
		{#if verified}
			<OptionList options={[option]} onSelect={startMonitor} />
		{:else}
			<p class="obs-dim font-mono text-sm">Verifying source…</p>
		{/if}
	</WizardFrame>
{/if}
