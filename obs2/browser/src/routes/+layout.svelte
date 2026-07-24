<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { settings } from '$lib/stores/settings.svelte';
	import {
		monitor,
		monitorHref,
		monitorPhaseStyleForPhase,
		monitorPresentationPhase
	} from '$lib/stores/monitor.svelte';
	import { startAppSocket, stopAppSocket } from '$lib/stores/appSocket.svelte';
	import AppHeader from '$lib/components/AppHeader.svelte';
	import KiaDeathOverlay from '$lib/components/KiaDeathOverlay.svelte';
	import NotificationFlags from '$lib/components/NotificationFlags.svelte';
	import WelcomeDialog from '$lib/components/WelcomeDialog.svelte';
	import ManualUpdateDialog from '$lib/components/ManualUpdateDialog.svelte';
	import RunCatalogSyncDialog from '$lib/components/RunCatalogSyncDialog.svelte';
	import { replayBuffer, refreshReplayBuffer } from '$lib/stores/replayBuffer.svelte';
	import { updates } from '$lib/stores/updates.svelte';
	import { youtube } from '$lib/stores/youtube.svelte';
	import { runCatalog } from '$lib/stores/runCatalog.svelte';
	import { page } from '$app/state';
	import { afterNavigate, goto } from '$app/navigation';
	import { onMount, tick } from 'svelte';

	let { children } = $props();
	let contentScroller: HTMLDivElement | undefined;
	let menuOpen = $state(false);
	let windowFocused = $state(true);
	let pendingNavigation = $state<string | null>(null);

	onMount(() => {
		windowFocused = document.hasFocus();

		startAppSocket();
		void youtube.load().catch((err) => console.warn('Failed to load YouTube status', err));

		return () => {
			stopAppSocket();
		};
	});

	$effect(() => {
		const savedState = settings.savedState;
		const loaded = settings.loaded;
		const lastSavedState = settings.lastSavedState;

		if (loaded && savedState !== lastSavedState) {
			settings.saveImmediately();
		}
	});

	// The wizard can't do anything useful without the replay buffer, so re-check
	// its status on load and on every navigation.
	afterNavigate(() => {
		pendingNavigation = null;
		void refreshReplayBuffer();
	});

	const navigate = (href: string, options: { replaceState?: boolean } = {}) => {
		if (page.url.pathname === href || pendingNavigation === href) return;
		pendingNavigation = href;
		goto(href, options);
	};

	// When the replay buffer is confirmed unavailable, force the user back to `/`
	// (which explains how to enable it); `/`, `/runs`, `/options`, and `/developer`
	// are exempt. An unknown status (null) never redirects.
	$effect(() => {
		const path = page.url.pathname;
		const exempt = path === '/' || path === '/runs' || path === '/options' || path === '/developer';
		if (replayBuffer.status?.available === false && !exempt) {
			navigate('/', { replaceState: true });
		}
	});

	const isMonitorSetupPath = (path: string): boolean =>
		path === '/' || path === '/sources' || path.startsWith('/sources/');

	// If monitoring is already active, the setup flow should collapse back to the
	// live monitor page. Keep non-monitoring pages such as runs/options reachable.
	$effect(() => {
		const path = page.url.pathname;
		const href = monitorHref(monitor.status);
		if (href && path !== href && isMonitorSetupPath(path)) {
			navigate(href, { replaceState: true });
		}
	});

	$effect(() => {
		const path = page.url.pathname;
		tick().then(() => {
			if (page.url.pathname === path) {
				contentScroller?.scrollTo({ top: 0, left: 0 });
			}
		});
	});

	$effect(() => {
		page.url.pathname;
		page.url.search;
		menuOpen = false;
	});

	const onWindowFocus = () => {
		windowFocused = true;
	};

	const onWindowBlur = () => {
		windowFocused = false;
	};

	const links = $derived([
		{ href: '/', label: 'Monitor' },
		{ href: '/runs', label: 'Runs' },
		{ href: '/options', label: 'Options' },
		...(settings.showDeveloperSettings ? [{ href: '/developer', label: 'Developer' }] : [])
	]);

	const pluginVersion = $derived(settings.pluginVersion);
	const activeMonitorHref = $derived(monitorHref(monitor.status));
	const activeMonitorPhase = $derived(
		monitor.chromePhase ?? (activeMonitorHref ? monitorPresentationPhase(monitor.recordingState) : null)
	);
	const activeMonitorStyle = $derived(monitorPhaseStyleForPhase(activeMonitorPhase ?? 'complete'));
	const showWelcomeModal = $derived(settings.loaded && settings.fileError === null && !settings.welcomeModalShown);
	const manualUpdate = $derived(!showWelcomeModal && !monitor.status?.enabled ? updates.manualUpdate : null);

	const dismissWelcomeModal = () => {
		settings.welcomeModalShown = true;
	};
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
<svelte:window onfocus={onWindowFocus} onblur={onWindowBlur} />

<div
	class="obs-app-shell flex h-screen min-h-0 min-w-100 flex-col overflow-hidden {activeMonitorStyle.border}"
	class:obs-window-focused={windowFocused}
>
	<AppHeader
		{links}
		currentPath={page.url.pathname}
		{pluginVersion}
		{activeMonitorHref}
		recordingState={monitor.recordingState}
		monitorPhase={activeMonitorPhase}
		bind:menuOpen
	/>

	<div bind:this={contentScroller} class="obs-content-scroller min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
		{@render children()}
	</div>

	<NotificationFlags />
	<KiaDeathOverlay trigger={monitor.kiaEffectId} />

	{#if runCatalog.sync}
		<RunCatalogSyncDialog sync={runCatalog.sync} />
	{:else if showWelcomeModal}
		<WelcomeDialog dismiss={dismissWelcomeModal} />
	{:else if manualUpdate}
		<ManualUpdateDialog
			update={manualUpdate}
			dismiss={() => updates.dismissManualUpdate()}
			openRelease={() => updates.openAvailableRelease()}
		/>
	{/if}
</div>
