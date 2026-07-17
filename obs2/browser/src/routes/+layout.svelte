<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { settings } from '$lib';
	import { monitor, monitorHref, monitorPhaseStyle } from '$lib/monitor.svelte';
	import { startAppSocket, stopAppSocket } from '$lib/appSocket.svelte';
	import KiaDeathOverlay from '$lib/KiaDeathOverlay.svelte';
	import NotificationFlags from '$lib/NotificationFlags.svelte';
	import { addNotificationFlag } from '$lib/notifications.svelte';
	import { replayBuffer, refreshReplayBuffer } from '$lib/replayBuffer.svelte';
	import { youtube } from '$lib/youtube.svelte';
	import { page } from '$app/state';
	import { afterNavigate, goto } from '$app/navigation';
	import { onMount, tick } from 'svelte';

	let { children } = $props();
	let contentScroller: HTMLDivElement | undefined;
	let menuButton = $state<HTMLButtonElement>();
	let menuPanel = $state<HTMLElement>();
	let welcomeButton = $state<HTMLButtonElement>();
	let menuOpen = $state(false);
	let windowFocused = $state(true);
	let pendingNavigation = $state<string | null>(null);

	onMount(() => {
		windowFocused = document.hasFocus();

		startAppSocket();
		void youtube.load().catch((err) => console.warn('Failed to load YouTube status', err));
		void settings
			.load()
			.then(() => {
				if (settings.fileError) {
					addNotificationFlag({
						key: 'settings-config-error',
						title: 'Config file invalid',
						detail: settings.fileError,
						meta: 'Click to open options.',
						tone: 'error',
						sticky: true,
						href: '/options'
					});
				}
			})
			.catch((err) => {
				console.warn('Failed to load settings', err);
			});

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

	const toggleMenu = () => {
		menuOpen = !menuOpen;
	};

	const closeMenu = () => {
		menuOpen = false;
	};

	const onWindowClick = (event: MouseEvent) => {
		if (!menuOpen) return;

		const target = event.target;
		if (!(target instanceof Node)) return;
		if (menuButton?.contains(target) || menuPanel?.contains(target)) return;

		closeMenu();
	};

	const onWindowKeydown = (event: KeyboardEvent) => {
		if (event.key !== 'Escape' || !menuOpen) return;

		event.preventDefault();
		closeMenu();
		menuButton?.focus();
	};

	const onWindowFocus = () => {
		windowFocused = true;
	};

	const onWindowBlur = () => {
		windowFocused = false;
	};

	const bannerClass =
		'obs-banner inline-block max-w-full p-2 text-left font-mono text-[10px] leading-[1.17] whitespace-pre';
	const bannerText = `\
┏┳┓┓     ┏┓  ┓ ┓      ┏┓
 ┃ ┣┓┏┓  ┃┓┏┓┃┏┫┏┓┏┓  ┣ ┓┏┏┓
 ┻ ┛┗┗   ┗┛┗┛┗┗┻┗ ┛┗  ┗┛┗┫┗
                         ┛`;

	const menuButtonClass =
		'obs-icon-button obs-phase-gold-button inline-flex h-8 w-8 shrink-0 items-center justify-center';
	const menuPanelClass =
		'obs-menu-panel absolute top-full right-2 z-40 mt-2 w-[min(20rem,calc(100vw-1rem))] rounded p-2 text-sm';
	const menuLinkCommon =
		'obs-menu-link flex min-h-11 items-center justify-end rounded px-3 py-2 text-right transition-colors';
	const menuLinkClass = menuLinkCommon;
	const menuLinkActiveClass = `${menuLinkCommon} obs-menu-link-active`;

	const links = $derived([
		{ href: '/', label: 'Monitor' },
		{ href: '/runs', label: 'Runs' },
		{ href: '/options', label: 'Options' },
		...(settings.showDeveloperSettings ? [{ href: '/developer', label: 'Developer' }] : [])
	]);

	const pluginVersion = $derived(settings.pluginVersion);
	const activeMonitorHref = $derived(monitorHref(monitor.status));
	const activeMonitorStyle = $derived(monitorPhaseStyle(monitor.recordingState));
	const showWelcomeModal = $derived(settings.loaded && settings.fileError === null && !settings.welcomeModalShown);

	const dismissWelcomeModal = () => {
		settings.welcomeModalShown = true;
	};

	$effect(() => {
		if (!showWelcomeModal) return;

		tick().then(() => {
			if (showWelcomeModal) welcomeButton?.focus();
		});
	});
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
<svelte:window onclick={onWindowClick} onkeydown={onWindowKeydown} onfocus={onWindowFocus} onblur={onWindowBlur} />

<div
	class="obs-app-shell flex h-screen min-h-0 min-w-100 flex-col overflow-hidden"
	class:obs-window-focused={windowFocused}
>
	<header class="obs-app-header relative flex shrink-0 items-center">
		<a href="/" aria-label="The Golden Eye home" class="block min-w-0 shrink overflow-hidden">
			<pre class={bannerClass}>{bannerText}</pre>
		</a>

		{#if menuOpen}
			<nav bind:this={menuPanel} id="global-navigation-menu" class={menuPanelClass} aria-label="Primary navigation">
				<ul class="flex flex-col gap-1">
					{#each links as link}
						{@const isActive = page.url.pathname === link.href}
						<li>
							<a
								class={isActive ? menuLinkActiveClass : menuLinkClass}
								href={link.href}
								aria-current={isActive ? 'page' : undefined}
								onclick={closeMenu}
							>
								{link.label}
							</a>
						</li>
					{/each}
				</ul>
				<div class="obs-menu-footer mt-2 px-3 pt-2 pb-1 text-right text-xs">
					v{pluginVersion}
				</div>
			</nav>
		{/if}

		<div class="ml-auto flex shrink-0 items-center gap-2 px-2 font-mono text-sm">
			{#if activeMonitorHref}
				<a
					href={activeMonitorHref}
					class="obs-button obs-phase-button inline-flex items-center gap-2 px-2 py-1 {activeMonitorStyle.button}"
					aria-label="Return to monitoring screen"
				>
					<span class="obs-phase-dot h-2 w-2 rounded-full {activeMonitorStyle.dot}" aria-hidden="true"></span>
					<span>Monitoring</span>
				</a>
			{/if}
			<button
				bind:this={menuButton}
				type="button"
				class={menuButtonClass}
				aria-label={menuOpen ? 'Close navigation menu' : 'Open navigation menu'}
				aria-controls="global-navigation-menu"
				aria-expanded={menuOpen}
				onclick={toggleMenu}
			>
				<span class="flex flex-col gap-1.5" aria-hidden="true">
					<span class="block h-0.5 w-5 rounded bg-current"></span>
					<span class="block h-0.5 w-5 rounded bg-current"></span>
					<span class="block h-0.5 w-5 rounded bg-current"></span>
				</span>
			</button>
		</div>
	</header>

	<div bind:this={contentScroller} class="obs-content-scroller min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
		{@render children()}
	</div>

	<NotificationFlags />
	<KiaDeathOverlay trigger={monitor.kiaEffectId} />

	{#if showWelcomeModal}
		<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
			<div
				class="obs-dialog w-full max-w-md overflow-hidden rounded"
				role="dialog"
				aria-modal="true"
				aria-labelledby="welcome-dialog-title"
				aria-describedby="welcome-dialog-body"
			>
				<div class="obs-dialog-header px-4 py-3">
					<h2 id="welcome-dialog-title" class="obs-heading text-lg font-semibold">Welcome to The Golden Eye</h2>
				</div>
				<div id="welcome-dialog-body" class="flex flex-col gap-4 px-4 py-4 text-sm leading-6">
					<p>
						This plugin helps save clips from GoldenEye 007 speedruns by watching your capture and managing
						replay-buffer saves around runs.
					</p>
					<p class="obs-alert-warning obs-alert-warning-body rounded px-3 py-2">
						Do not rely completely on this plugin as your only copy. You are strongly recommended to stream or record
						your gameplay somewhere reliable, such as YouTube or Twitch, so a missed detection or local recording issue
						does not cost you the run.
					</p>
				</div>
				<div class="flex justify-end gap-2 px-4 pb-4">
					<button
						bind:this={welcomeButton}
						type="button"
						class="obs-button obs-button-gold px-4 py-2"
						onclick={dismissWelcomeModal}
					>
						I understand
					</button>
				</div>
			</div>
		</div>
	{/if}
</div>
