<script lang="ts">
	import { afterNavigate, goto } from '$app/navigation';
	import { page } from '$app/state';
	import { backend } from '$lib/api';
	import MonitorView, { type MonitorTransition } from '$lib/components/MonitorView.svelte';
	import { settings } from '$lib/stores/settings.svelte';
	import { monitor, monitorPresentationPhase } from '$lib/stores/monitor.svelte';
	import { refreshReplayBuffer } from '$lib/stores/replayBuffer.svelte';
	import { obsSources } from '$lib/stores/sources.svelte';
	import { recentRuns } from '$lib/stores/recentRuns.svelte';
	import { onDestroy } from 'svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();

	let monitoring = $state(false);
	let verified = $state(false);
	let statusChecked = $state(false);
	let transition = $state<MonitorTransition>(null);
	let pendingNavigation = $state<string | null>(null);

	const sourcePath = $derived(`/sources/${encodeURIComponent(params.sourceName)}`);
	const isCurrentPage = $derived(page.url.pathname === sourcePath);
	const sourceExists = $derived((obsSources.items ?? []).some((source) => source.name === params.sourceName));

	$effect(() => {
		if (!isCurrentPage) return;
		monitor.chromePhase = monitorPresentationPhase(monitor.recordingState, transition !== null, verified);
	});

	onDestroy(() => {
		monitor.chromePhase = null;
	});

	const navigate = (href: string, options: { replaceState?: boolean } = {}) => {
		if (page.url.pathname === href || pendingNavigation === href) return;
		pendingNavigation = href;
		void goto(href, options);
	};

	const syncMonitorStatus = () => {
		if (!isCurrentPage || !monitor.loaded) return;
		statusChecked = true;
		const status = monitor.status;
		if (!status?.enabled) return;
		if (status.sourceName !== params.sourceName) {
			navigate(`/sources/${encodeURIComponent(status.sourceName)}`, { replaceState: true });
			return;
		}
		monitoring = true;
	};

	afterNavigate(async () => {
		pendingNavigation = null;
		if (!isCurrentPage) return;
		void recentRuns.refresh();

		verified = false;
		statusChecked = false;
		monitoring = false;
		syncMonitorStatus();
	});

	$effect(() => {
		monitor.loaded;
		monitor.status;
		syncMonitorStatus();
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
		if (
			!monitor.loaded ||
			monitor.status?.enabled ||
			!statusChecked ||
			monitoring ||
			transition ||
			pendingNavigation ||
			!verified
		)
			return;
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
		if (monitor.status.sourceName !== params.sourceName) {
			navigate(`/sources/${encodeURIComponent(monitor.status.sourceName)}`, { replaceState: true });
		}
	});

	const startMonitor = async () => {
		if (monitoring || transition || pendingNavigation) return;
		transition = 'starting';
		try {
			await settings.saveNow();
			await recentRuns.refresh();
			await backend.startMonitor(params.sourceName);
			void refreshReplayBuffer();
			monitoring = true;
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
			navigate('/', { replaceState: true });
		} finally {
			transition = null;
		}
	};

	const stopMonitor = async () => {
		if (transition) return;
		transition = 'stopping';
		try {
			await backend.stopMonitor();
			void refreshReplayBuffer();
			monitoring = false;
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
			backend.stopMonitor();
		}
	};
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
