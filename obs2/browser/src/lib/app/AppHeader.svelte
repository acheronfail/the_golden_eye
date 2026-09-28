<script lang="ts">
	import type { RecordingStatus, YouTubeUploadStatus } from '$lib/api';
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
		youtubeConnected = false,
		uploads = [],
		uploadsOpen = $bindable(false),
		menuOpen = $bindable(false)
	}: {
		links: AppHeaderLink[];
		currentPath: string;
		pluginVersion: string;
		activeMonitorHref?: string | null;
		recordingState?: RecordingStatus | null;
		monitorPhase?: MonitorPhase | null;
		youtubeConnected?: boolean;
		uploads?: YouTubeUploadStatus[];
		uploadsOpen?: boolean;
		menuOpen?: boolean;
	} = $props();

	const currentUploads = $derived(
		uploads.filter((upload) => upload.state === 'queued' || upload.state === 'uploading')
	);
	let uploadsButton = $state<HTMLButtonElement>();
	let uploadsPanel = $state<HTMLElement>();
	$effect(() => {
		if (!youtubeConnected) uploadsOpen = false;
	});

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
		'obs-menu-panel absolute top-full right-2 z-40 mt-2 w-max max-w-[calc(100vw-1rem)] rounded p-2 text-sm';
	const menuLinkCommon =
		'obs-menu-link flex min-h-11 w-full items-center justify-end rounded border border-transparent py-2 pr-4 pl-12 text-right transition-colors';
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
		const target = event.target;
		if (!(target instanceof Node)) return;
		if (!menuButton?.contains(target) && !menuPanel?.contains(target)) closeMenu();
		if (!uploadsButton?.contains(target) && !uploadsPanel?.contains(target)) uploadsOpen = false;
	};

	const onWindowKeydown = (event: KeyboardEvent) => {
		if (event.key !== 'Escape') return;
		if (uploadsOpen) {
			event.preventDefault();
			uploadsOpen = false;
			uploadsButton?.focus();
			return;
		}
		if (!menuOpen) return;
		event.preventDefault();
		closeMenu();
		menuButton?.focus();
	};
</script>

<svelte:window onclick={onWindowClick} onkeydown={onWindowKeydown} />

<header class="relative flex shrink-0 items-center obs-app-header">
	<a href="/" aria-label="The Golden Eye home" class="block min-w-0 shrink overflow-hidden">
		<pre class="{bannerClass} {activeMonitorStyle.heading}">{bannerText}</pre>
	</a>

	{#if menuOpen}
		<nav bind:this={menuPanel} id="global-navigation-menu" class={menuPanelClass} aria-label="Primary navigation">
			<ul class="flex flex-col gap-1">
				{#each links as link}
					{@const isCurrentPage = isCurrentLink(link)}
					<li class="w-full text-right">
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
			<div class="mt-2 obs-menu-footer px-3 pt-2 pb-1 text-right text-xs">
				v{pluginVersion}
			</div>
		</nav>
	{/if}

	<div class="ml-auto flex shrink-0 items-center gap-2 px-2 font-mono text-sm">
		{#if activeMonitorHref}
			<a
				href={activeMonitorHref}
				class="obs-button inline-flex h-8 items-center justify-center gap-2 obs-phase-button px-2 py-1 {activeMonitorStyle.button}"
				class:w-8={youtubeConnected}
				aria-label="Return to monitoring screen"
			>
				<span class="obs-phase-dot h-2 w-2 rounded-full {activeMonitorStyle.dot}" aria-hidden="true"></span>
				{#if !youtubeConnected}<span>Monitoring</span>{/if}
			</a>
		{/if}
		{#if youtubeConnected}
			<button
				bind:this={uploadsButton}
				type="button"
				class="{menuButtonClass} relative {activeMonitorStyle.button}"
				class:obs-icon-button-open={uploadsOpen}
				aria-label={`Uploads${currentUploads.length ? ` (${currentUploads.length} active)` : ''}`}
				aria-controls="current-uploads"
				aria-expanded={uploadsOpen}
				title="Uploads"
				onclick={() => {
					uploadsOpen = !uploadsOpen;
					closeMenu();
				}}
			>
				<svg
					class="h-5 w-5"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"
					aria-hidden="true"
				>
					<path d="M12 16V3m-5 5 5-5 5 5M4 16v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4" />
				</svg>
				{#if currentUploads.length}
					<span
						aria-hidden="true"
						class="absolute -top-1.5 -right-1.5 min-w-4 rounded-full bg-(--obs-gold) px-1 text-center text-[10px] leading-4 font-semibold text-[#111318]"
						>{currentUploads.length}</span
					>
				{/if}
			</button>
			{#if uploadsOpen}
				<section
					bind:this={uploadsPanel}
					id="current-uploads"
					aria-label="Current uploads"
					class="absolute top-full right-2 z-40 mt-2 w-80 max-w-[calc(100vw-1rem)] rounded obs-menu-panel p-3 font-sans text-sm"
				>
					<h2 class="mb-3 font-semibold">Uploads</h2>
					{#if currentUploads.length === 0}
						<p class="text-(--obs-text-muted)">No current uploads.</p>
					{:else}
						<ul class="flex max-h-[min(24rem,calc(100dvh-9rem))] flex-col gap-4 overflow-y-auto">
							{#each currentUploads as upload (upload.id)}
								{@const title = upload.title || upload.fileName}
								{@const progress =
									upload.state === 'queued'
										? 0
										: upload.progressRatio === null
											? undefined
											: Math.round(Math.max(0, Math.min(1, upload.progressRatio)) * 100)}
								<li class="min-w-0">
									<a
										class="block rounded obs-menu-link px-1 py-1 wrap-anywhere underline underline-offset-2"
										href={`/runs?runId=${encodeURIComponent(upload.runId)}`}
										onclick={() => (uploadsOpen = false)}>{title}</a
									>
									<div class="mt-1 mb-2 px-1 text-xs text-(--obs-text-muted)">
										{upload.state === 'queued'
											? 'Queued'
											: progress === undefined
												? 'Uploading…'
												: `Uploading ${progress}%`}
									</div>
									<div
										role="progressbar"
										aria-label={title}
										aria-valuemin={0}
										aria-valuemax={100}
										aria-valuenow={progress}
										aria-valuetext={upload.state === 'queued' ? 'Queued' : undefined}
										class="h-1.5 overflow-hidden rounded-full bg-(--obs-control)"
									>
										<div
											class="h-full w-(--upload-progress) rounded-full bg-(--obs-gold) transition-[width] duration-300 motion-reduce:animate-none motion-reduce:transition-none"
											class:animate-pulse={progress === undefined}
											style:--upload-progress={`${progress ?? 100}%`}
										></div>
									</div>
								</li>
							{/each}
						</ul>
					{/if}
				</section>
			{/if}
		{/if}
		<button
			bind:this={menuButton}
			type="button"
			class="{menuButtonClass} {activeMonitorStyle.button}"
			aria-label={menuOpen ? 'Close navigation menu' : 'Open navigation menu'}
			aria-controls="global-navigation-menu"
			aria-expanded={menuOpen}
			onclick={() => {
				menuOpen = !menuOpen;
				uploadsOpen = false;
			}}
		>
			<span class="flex flex-col gap-1.5" aria-hidden="true">
				<span class="block h-0.5 w-5 rounded bg-current"></span>
				<span class="block h-0.5 w-5 rounded bg-current"></span>
				<span class="block h-0.5 w-5 rounded bg-current"></span>
			</span>
		</button>
	</div>
</header>
