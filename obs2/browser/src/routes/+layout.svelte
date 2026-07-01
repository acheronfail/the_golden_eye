<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { settings } from '$lib';
	import { replayBuffer, refreshReplayBuffer } from '$lib/replayBuffer.svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount, tick } from 'svelte';

	let { children } = $props();
	let contentScroller: HTMLDivElement | undefined;

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
	});

	// While the replay buffer is confirmed unavailable, force the user back to `/`
	// (which explains how to enable it). `/`, `/options`, and the dev-only
	// `/developer` tools are exempt so the user has somewhere to land and
	// debugging still works. An unknown status (null) never redirects — we only
	// act on a definitive "off".
	$effect(() => {
		const path = page.url.pathname;
		const exempt = path === '/' || path === '/options' || path === '/developer';
		if (replayBuffer.status?.available === false && !exempt) {
			goto('/');
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

	const bannerClass =
		'inline-block border-r p-2 text-left font-mono text-xs leading-[1.17] whitespace-pre text-amber-400';
	const bannerText = `\
┏┳┓┓     ┏┓  ┓ ┓      ┏┓
 ┃ ┣┓┏┓  ┃┓┏┓┃┏┫┏┓┏┓  ┣ ┓┏┏┓
 ┻ ┛┗┗   ┗┛┗┛┗┗┻┗ ┛┗  ┗┛┗┫┗
                         ┛`;

	const liCommon = 'inline-block rounded border border-amber-500 px-2 py-1 font-mono text-sm';
	const liClass = `${liCommon} text-amber-400 hover:text-black hover:bg-amber-600`;
	const liActiveClass = `${liCommon} bg-amber-600 text-black hover:text-black hover:bg-amber-700`;

	const links = [
		{ href: '/', label: 'Home' },
		{ href: '/options', label: 'Options' },
		...(import.meta.hot ? [{ href: '/developer', label: 'Developer' }] : [])
	];
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>

<div class="flex h-screen min-h-0 flex-col overflow-hidden">
	<header class="flex shrink-0 items-center border-b border-b-amber-400">
		<a href="/">
			<pre class={bannerClass}>{bannerText}</pre>
		</a>

		<ul class="ml-4 inline-flex gap-4 font-mono text-sm text-amber-400">
			{#each links as link}
				{@const isActive = page.url.pathname === link.href}
				<a class={isActive ? liActiveClass : liClass} href={link.href}> <li>{link.label}</li></a>
			{/each}
		</ul>
	</header>

	<div bind:this={contentScroller} class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
		{@render children()}
	</div>
</div>
