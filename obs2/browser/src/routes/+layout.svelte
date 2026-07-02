<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { settings } from '$lib';
	import { monitor, monitorHref, monitorPhaseStyle, refreshMonitor } from '$lib/monitor.svelte';
	import NotificationFlags from '$lib/NotificationFlags.svelte';
	import { replayBuffer, refreshReplayBuffer } from '$lib/replayBuffer.svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount, tick } from 'svelte';

	let { children } = $props();
	let contentScroller: HTMLDivElement | undefined;
	let menuButton = $state<HTMLButtonElement>();
	let menuPanel = $state<HTMLElement>();
	let menuOpen = $state(false);

	onMount(() => {
		void settings.load().catch((err) => {
			console.warn('Failed to load settings', err);
		});
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
	// its status on load and on every navigation. Re-runs whenever the path
	// changes (referenced below so it's tracked as a dependency).
	$effect(() => {
		page.url.pathname;
		refreshReplayBuffer();
		void refreshMonitor().catch(() => {
			// Keep the global indicator hidden if the backend is unavailable.
		});
	});

	// While the replay buffer is confirmed unavailable, force the user back to `/`
	// (which explains how to enable it). `/`, `/runs`, `/options`, and the dev-only
	// `/developer` tools are exempt so the user has somewhere to land and
	// debugging still works. An unknown status (null) never redirects — we only
	// act on a definitive "off".
	$effect(() => {
		const path = page.url.pathname;
		const exempt = path === '/' || path === '/runs' || path === '/options' || path === '/developer';
		if (replayBuffer.status?.available === false && !exempt) {
			goto('/');
		}
	});

	const isMonitorSetupPath = (path: string): boolean =>
		path === '/' ||
		path === '/source' ||
		path.startsWith('/source/') ||
		path === '/sources' ||
		path.startsWith('/sources/');

	// If monitoring is already active, the setup flow should collapse back to the
	// live monitor page. Keep non-monitoring pages such as runs/options reachable.
	$effect(() => {
		const path = page.url.pathname;
		const href = monitorHref(monitor.status);
		if (href && path !== href && isMonitorSetupPath(path)) {
			goto(href, { replaceState: true });
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

	const bannerClass =
		'obs-banner inline-block max-w-full p-2 text-left font-mono text-xs leading-[1.17] whitespace-pre';
	const bannerText = `\
┏┳┓┓     ┏┓  ┓ ┓      ┏┓
 ┃ ┣┓┏┓  ┃┓┏┓┃┏┫┏┓┏┓  ┣ ┓┏┏┓
 ┻ ┛┗┗   ┗┛┗┛┗┗┻┗ ┛┗  ┗┛┗┫┗
                         ┛`;

	const menuButtonClass =
		'obs-icon-button obs-phase-gold-button inline-flex h-10 w-10 shrink-0 items-center justify-center';
	const menuPanelClass =
		'obs-menu-panel absolute top-full right-2 z-40 mt-2 w-[min(20rem,calc(100vw-1rem))] rounded p-2 text-sm';
	const menuLinkCommon =
		'obs-menu-link flex min-h-11 items-center justify-end rounded px-3 py-2 text-right transition-colors';
	const menuLinkClass = menuLinkCommon;
	const menuLinkActiveClass = `${menuLinkCommon} obs-menu-link-active`;

	const links = [
		{ href: '/', label: 'Monitor' },
		{ href: '/runs', label: 'Runs' },
		{ href: '/options', label: 'Options' },
		...(import.meta.hot ? [{ href: '/developer', label: 'Developer' }] : [])
	];

	const pluginVersion = import.meta.env.VITE_GE_PLUGIN_VERSION ?? '0.0.0';
	const activeMonitorHref = $derived(monitorHref(monitor.status));
	const activeMonitorStyle = $derived(monitorPhaseStyle(monitor.recordingState));
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
<svelte:window onclick={onWindowClick} onkeydown={onWindowKeydown} />

<div class="obs-app-shell flex h-screen min-h-0 min-w-[400px] flex-col overflow-hidden">
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
</div>
