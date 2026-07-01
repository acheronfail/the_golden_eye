<script lang="ts">
	import { afterNavigate, goto } from '$app/navigation';
	import { onDestroy } from 'svelte';
	import {
		connectMonitorSocket,
		getMonitorStatus,
		getSources,
		startMonitor as apiStartMonitor,
		stopMonitor as apiStopMonitor,
		type LevelMatch,
		type RecordingSaved,
		type RecordingStatus
	} from '$lib/api';
	import { settings } from '$lib';
	import WizardFrame from '$lib/wizard/WizardFrame.svelte';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();

	let monitoring = $state(false);
	let matchSocket: WebSocket | null = null;
	let match = $state<LevelMatch | null>(null);
	// The most recent clip the backend saved this session, shown while monitoring.
	let lastSaved = $state<RecordingSaved | null>(null);
	// The recorder's latest per-run state transition (started / cancelled /
	// failed / savePending), tracked to drive the big centered title + border.
	// `null` is the resting "waiting for level start" state between runs.
	let recordingState = $state<RecordingStatus | null>(null);

	// The centered title and thick border are meant to read at a glance (even from
	// peripheral vision), so each recorder state maps to a distinct word + colour.
	// Full literal class strings so Tailwind's scanner keeps them.
	interface PhaseStyle {
		title: string;
		border: string; // thick frame colour
		heading: string; // big title colour
		tag: string; // small "monitoring" label colour
	}
	const phaseStyle = (state: RecordingStatus | null): PhaseStyle => {
		switch (state) {
			case 'started':
				return {
					title: 'recording',
					border: 'border-green-500',
					heading: 'text-green-300',
					tag: 'text-green-500'
				};
			case 'cancelled':
				return {
					title: 'cancelled',
					border: 'border-neutral-500',
					heading: 'text-neutral-300',
					tag: 'text-neutral-500'
				};
			case 'failed':
				return {
					title: 'failed',
					border: 'border-red-500',
					heading: 'text-red-300',
					tag: 'text-red-500'
				};
			case 'aborted':
				return {
					title: 'aborted',
					border: 'border-red-500',
					heading: 'text-red-300',
					tag: 'text-red-500'
				};
			case 'kia':
				return {
					title: 'killed in action',
					border: 'border-red-500',
					heading: 'text-red-300',
					tag: 'text-red-500'
				};
			case 'complete':
				return {
					title: 'complete',
					border: 'border-fuchsia-500',
					heading: 'text-fuchsia-300',
					tag: 'text-fuchsia-500'
				};
			case 'statsSkipped':
				return {
					title: 'skipped stats',
					border: 'border-red-500',
					heading: 'text-red-300',
					tag: 'text-red-500'
				};
			case 'failedDiscarded':
				return {
					title: 'failed run not saved',
					border: 'border-neutral-500',
					heading: 'text-neutral-300',
					tag: 'text-neutral-500'
				};
			case 'savePending':
				return {
					title: 'saving recording',
					border: 'border-cyan-500',
					heading: 'text-cyan-300',
					tag: 'text-cyan-500'
				};
			case null:
			default:
				return {
					title: 'waiting for level start',
					border: 'border-amber-500',
					heading: 'text-amber-300',
					tag: 'text-amber-500'
				};
		}
	};
	const style = $derived(phaseStyle(recordingState));

	// Two recorder states are transient and fall back to the resting title on a
	// timer: `cancelled` lingers briefly, and `savePending`/`statsSkipped` wait for
	// the `recordingSaved` event but time out if it never arrives (e.g. save
	// failed). Any newer transition supersedes a pending revert (see below).
	const CANCELLED_LINGER_MS = 2000;
	const SAVE_TIMEOUT_MS = 30000;
	let revertTimer: ReturnType<typeof setTimeout> | null = null;

	const clearRevertTimer = () => {
		if (revertTimer !== null) {
			clearTimeout(revertTimer);
			revertTimer = null;
		}
	};

	// Format a level time (whole seconds) as m:ss for the stats overlay readout.
	const formatTime = (secs: number): string => {
		const m = Math.floor(secs / 60);
		const s = secs % 60;
		return `${m}:${s.toString().padStart(2, '0')}`;
	};

	const applyRecordingState = (status: RecordingStatus) => {
		// A fresh transition always supersedes a pending revert-to-idle timer --
		// this is what lets a new run that starts within the 2s `cancelled` window
		// jump straight to "recording" instead of blinking back to idle first.
		clearRevertTimer();
		recordingState = status;
		if (status === 'cancelled' || status === 'failedDiscarded') {
			revertTimer = setTimeout(() => {
				recordingState = null;
				revertTimer = null;
			}, CANCELLED_LINGER_MS);
		} else if (status === 'savePending' || status === 'statsSkipped') {
			// Normally `recordingSaved` clears us back to idle; this is the fallback
			// if that event never lands so we don't sit on "saving" forever.
			revertTimer = setTimeout(() => {
				recordingState = null;
				revertTimer = null;
			}, SAVE_TIMEOUT_MS);
		}
	};

	// The source name comes from the URL, so it may be stale if the user navigated
	// here from browser history / a manual URL, or renamed the source in OBS. Verify
	// it still exists before letting them monitor; the start button stays disabled
	// until this completes, and a failed check routes back home to restart the flow.
	let verified = $state(false);

	// Runs on initial load and after every navigation. A redirect below targets
	// this same route, so the component instance is reused and `onMount` wouldn't
	// fire again -- `afterNavigate` re-runs the restore each time the params change.
	afterNavigate(async () => {
		verified = false;
		try {
			// If a monitor is already running, restore that state rather than
			// defaulting to idle. When it's running for a different source/lang than
			// this URL describes, redirect to the page that matches reality.
			const status = await getMonitorStatus();
			if (status.enabled) {
				if (status.sourceName !== params.sourceName || status.lang !== params.lang) {
					goto(`/source/${encodeURIComponent(status.sourceName)}/${status.lang}`);
					return;
				}
				monitoring = true;
				connectMatchSocket();
			} else {
				monitoring = false;
				match = null;
				recordingState = null;
				clearRevertTimer();
				disconnectMatchSocket();
			}

			const sources = await getSources();
			if (sources.some((source) => source.name === params.sourceName)) {
				verified = true;
			} else {
				goto('/');
			}
		} catch {
			goto('/');
		}
	});

	const connectMatchSocket = () => {
		matchSocket?.close();
		const socket = connectMonitorSocket({
			onMatch: (m) => {
				match = m;
			},
			onRecordingState: (status) => {
				applyRecordingState(status);
			},
			onRecordingSaved: (saved) => {
				lastSaved = saved;
				// The clip is written: leave the transient "saving"/"skipped stats"
				// title and settle back to the resting state, ready for the next run.
				if (recordingState === 'savePending' || recordingState === 'statsSkipped') {
					clearRevertTimer();
					recordingState = null;
				}
			},
			onClose: () => {
				if (matchSocket === socket) matchSocket = null;
			}
		});
		matchSocket = socket;
	};
	const disconnectMatchSocket = () => {
		matchSocket?.close();
		matchSocket = null;
	};

	// Guards against a double start: the window Space handler and the OptionList
	// button's own keyboard activation can both fire for one keypress.
	let starting = false;
	const startMonitor = async () => {
		if (monitoring || starting) return;
		starting = true;
		try {
			await apiStartMonitor(params.sourceName, params.lang, settings.recordingOptions);
			monitoring = true;
			connectMatchSocket();
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
		} finally {
			starting = false;
		}
	};

	const stopMonitor = async () => {
		try {
			await apiStopMonitor();
			monitoring = false;
			match = null;
			lastSaved = null;
			recordingState = null;
			clearRevertTimer();
			disconnectMatchSocket();
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
		}
	};

	// Don't leak the revert timer (or the socket) if the page is torn down mid-run.
	onDestroy(() => {
		clearRevertTimer();
		disconnectMatchSocket();
	});

	const option: Option = {
		title: 'start monitor',
		detail: 'or press space to start monitoring'
	};

	// Space toggles monitoring from anywhere on the page (without relying on the
	// OptionList button holding focus); Escape also stops it. `startMonitor` is
	// idempotent, so it's harmless if the button's native activation also fires.
	const onkeydown = (event: KeyboardEvent) => {
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

{#if monitoring}
	<main class="relative flex flex-1 flex-col items-center justify-center px-6 py-12 text-center">
		<!-- A thick border frames the page area (below the header) to signal
		     monitoring is live; its colour tracks the recorder state so the state
		     reads even from peripheral vision. -->
		<div class="pointer-events-none absolute inset-0 z-10 border-8 {style.border}"></div>

		<p class="font-mono text-xs tracking-widest {style.tag} uppercase">
			Monitoring
			<span>({params.lang})</span>
		</p>
		<!-- The big centered title is the recorder state, in words, colour-matched
		     to the border. -->
		<h1 class="mt-4 text-6xl font-semibold wrap-break-word {style.heading}">
			{style.title}
		</h1>
		<!-- The raw matched screen, for detail beneath the plain-language title. -->
		<p class="mt-3 font-mono text-xs tracking-widest text-neutral-500 uppercase">
			{match?.screen ?? '…'}
		</p>

		{#if match?.times}
			<!-- Stats overlay is on screen (e.g. while a save is pending): surface the
			     matched times so they're visible on the page. The run time is always
			     present; the target and best times only appear when the game shows
			     them (see the `times` docs in api.ts). -->
			<div class="mt-6 flex flex-wrap justify-center gap-6 font-mono text-neutral-100">
				<span class="flex flex-col items-center">
					<span class="text-xs tracking-widest text-neutral-500 uppercase">time</span>
					<span class="text-4xl">{formatTime(match.times.time)}</span>
				</span>
				{#if match.times.target_time != null}
					<span class="flex flex-col items-center">
						<span class="text-xs tracking-widest text-neutral-500 uppercase">target</span>
						<span class="text-4xl">{formatTime(match.times.target_time)}</span>
					</span>
				{/if}
				{#if match.times.best_time != null}
					<span class="flex flex-col items-center">
						<span class="text-xs tracking-widest text-neutral-500 uppercase">best</span>
						<span class="text-4xl">{formatTime(match.times.best_time)}</span>
					</span>
				{/if}
			</div>
		{/if}

		<p class="mt-6 text-sm text-neutral-400">press escape or space to stop monitoring</p>

		{#if lastSaved}
			<!-- The most recent clip saved out of the replay buffer this session. -->
			<div class="mt-8 max-w-full font-mono text-xs text-neutral-400">
				<p class="tracking-widest text-emerald-400 uppercase">Saved clip</p>
				<p class="mt-1 break-all text-neutral-300">{lastSaved.path}</p>
				<p class="mt-1 text-neutral-500">
					{lastSaved.durationSecs.toFixed(1)}s{lastSaved.failed ? ' · failed' : ''}
				</p>
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
			<p class="font-mono text-sm text-neutral-500">Verifying source…</p>
		{/if}
	</WizardFrame>
{/if}
