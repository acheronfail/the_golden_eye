<script lang="ts">
	import type { RecordingStatus } from '$lib/api';
	import { monitorPhaseStyle, monitorPhaseStyleForPhase, type MonitorPhase } from '$lib/stores/monitor.svelte';

	export interface AppHeaderLink {
		href: string;
		label: string;
	}

	let {
		links,
		currentPath,
		pluginVersion,
		activeMonitorHref = null,
		recordingState = null,
		monitorPhase = null,
		menuOpen = $bindable(false)
	}: {
		links: AppHeaderLink[];
		currentPath: string;
		pluginVersion: string;
		activeMonitorHref?: string | null;
		recordingState?: RecordingStatus | null;
		monitorPhase?: MonitorPhase | null;
		menuOpen?: boolean;
	} = $props();

	let menuButton = $state<HTMLButtonElement>();
	let menuPanel = $state<HTMLElement>();

	const activeMonitorStyle = $derived(
		monitorPhase
			? monitorPhaseStyleForPhase(monitorPhase)
			: activeMonitorHref
				? monitorPhaseStyle(recordingState)
				: monitorPhaseStyleForPhase('complete')
	);
	const bannerClass =
		'obs-banner inline-block max-w-full p-2 text-left font-mono text-[10px] leading-[1.17] whitespace-pre';
	const bannerText = `\
┏┳┓┓     ┏┓  ┓ ┓      ┏┓
 ┃ ┣┓┏┓  ┃┓┏┓┃┏┫┏┓┏┓  ┣ ┓┏┏┓
 ┻ ┛┗┗   ┗┛┗┛┗┗┻┗ ┛┗  ┗┛┗┫┗
                         ┛`;
	const menuButtonClass = 'obs-icon-button inline-flex h-8 w-8 shrink-0 items-center justify-center';
	const menuPanelClass =
		'obs-menu-panel absolute top-full right-2 z-40 mt-2 w-[min(20rem,calc(100vw-1rem))] rounded p-2 text-sm';
	const menuLinkCommon =
		'obs-menu-link flex min-h-11 items-center justify-end rounded px-3 py-2 text-right transition-colors';
	const menuLinkClass = menuLinkCommon;
	const menuLinkActiveClass = `${menuLinkCommon} obs-menu-link-active`;
	const isCurrentLink = (link: AppHeaderLink): boolean =>
		link.href === '/'
			? currentPath === '/' || currentPath === '/sources' || currentPath.startsWith('/sources/')
			: currentPath === link.href;

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
</script>

<svelte:window onclick={onWindowClick} onkeydown={onWindowKeydown} />

<header class="obs-app-header relative flex shrink-0 items-center">
	<a href="/" aria-label="The Golden Eye home" class="block min-w-0 shrink overflow-hidden">
		<pre class="{bannerClass} {activeMonitorStyle.heading}">{bannerText}</pre>
	</a>

	{#if menuOpen}
		<nav bind:this={menuPanel} id="global-navigation-menu" class={menuPanelClass} aria-label="Primary navigation">
			<ul class="flex flex-col gap-1">
				{#each links as link}
					{@const isCurrentPage = isCurrentLink(link)}
					<li>
						<a
							class={isCurrentPage ? menuLinkActiveClass : menuLinkClass}
							href={link.href}
							aria-current={isCurrentPage ? 'page' : undefined}
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
			class="{menuButtonClass} {activeMonitorStyle.button}"
			aria-label={menuOpen ? 'Close navigation menu' : 'Open navigation menu'}
			aria-controls="global-navigation-menu"
			aria-expanded={menuOpen}
			onclick={() => (menuOpen = !menuOpen)}
		>
			<span class="flex flex-col gap-1.5" aria-hidden="true">
				<span class="block h-0.5 w-5 rounded bg-current"></span>
				<span class="block h-0.5 w-5 rounded bg-current"></span>
				<span class="block h-0.5 w-5 rounded bg-current"></span>
			</span>
		</button>
	</div>
</header>
