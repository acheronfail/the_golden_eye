<script lang="ts">
	import { afterNavigate, goto } from '$app/navigation';
	import { page } from '$app/state';
	import { backend } from '$lib/api';
	import MonitorView, { type MonitorTransition } from '$lib/features/monitor/MonitorView.svelte';
	import { MonitorSessionController } from '$lib/features/monitor/monitorSession.svelte';
	import ReplayBufferStopDialog from '$lib/features/monitor/ReplayBufferStopDialog.svelte';
	import { settings } from '$lib/stores/settings.svelte';
	import { monitor, monitorPresentationPhase } from '$lib/stores/monitor.svelte';
	import { refreshReplayBuffer } from '$lib/stores/replayBuffer.svelte';
	import { obsSources } from '$lib/stores/sources.svelte';
	import { recentRuns } from '$lib/stores/recentRuns.svelte';
	import { onDestroy, untrack } from 'svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();

	const sourcePath = $derived(`/sources/${encodeURIComponent(params.sourceName)}`);
	const isCurrentPage = $derived(page.url.pathname === sourcePath);
	const sourceExists = $derived((obsSources.items ?? []).some((source) => source.name === params.sourceName));
	const session = new MonitorSessionController({
		saveSettings: () => settings.saveNow(),
		refreshRecentRuns: () => recentRuns.refresh(),
		startMonitor: (sourceName) => backend.startMonitor(sourceName),
		stopMonitor: () => backend.stopMonitor(),
		refreshReplayBuffer: () => void refreshReplayBuffer(),
		navigate: (href, options) => void goto(href, options),
		reportError: (message) => alert(message),
		stopPromptShown: () => settings.stopReplayBufferPromptShown,
		saveStopPreference: async (stopReplayBuffer) => {
			settings.stopReplayBufferWhenMonitorStopped = stopReplayBuffer;
			settings.stopReplayBufferPromptShown = true;
			await settings.saveNow();
		}
	});
	const monitoring = $derived(session.monitoring);
	const verified = $derived(session.verified);
	const transition = $derived<MonitorTransition>(session.transition);

	$effect(() => {
		if (!isCurrentPage) return;
		monitor.chromePhase = monitorPresentationPhase(
			monitor.recordingState,
			session.transition !== null,
			session.verified
		);
	});

	onDestroy(() => {
		monitor.chromePhase = null;
	});

	afterNavigate(async () => {
		session.navigationSettled();
		if (!isCurrentPage) return;
		void recentRuns.refresh();
	});

	$effect(() => {
		const snapshot = {
			sourceName: params.sourceName,
			currentPath: page.url.pathname,
			monitorLoaded: monitor.loaded,
			monitorStatus: monitor.status,
			sourcesLoaded: obsSources.loaded,
			sourceExists
		};
		untrack(() => session.reconcile(snapshot));
	});

	const stopMonitor = () => session.requestStop();
	const chooseReplayBufferStop = (stopReplayBuffer: boolean) => session.chooseStopPreference(stopReplayBuffer);
	const onkeydown = (event: KeyboardEvent) => session.handleKeydown(event);
</script>

<svelte:head>
	<title>Monitor | {params.sourceName}</title>
</svelte:head>

<svelte:window {onkeydown} />

<MonitorView
	design={settings.monitorDesign}
	sourceName={params.sourceName}
	{verified}
	{monitoring}
	{transition}
	recordingState={monitor.recordingState}
	cvLanguage={monitor.cvLanguage}
	replaySaves={monitor.replaySaves}
	match={monitor.match}
	fps={monitor.fps}
	showMonitorFps={settings.showMonitorFps}
	recentRuns={recentRuns.items}
	recentRunsBusyId={recentRuns.busyRunId}
	recentRunsError={recentRuns.error}
	onKeepRun={(runId) => void recentRuns.keep(runId)}
	onStop={stopMonitor}
/>

{#if session.promptOpen}
	<ReplayBufferStopDialog busy={session.promptBusy} error={session.promptError} choose={chooseReplayBufferStop} />
{/if}
