<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { settings, STORAGE_KEY } from '../lib/settings.svelte';
	import { page } from '$app/state';

	let { children } = $props();

	$effect(() => {
		console.debug('Settings changed, saving to localStorage:', settings.savedState);
		localStorage.setItem(STORAGE_KEY, settings.savedState);
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
		...(import.meta.hot ? [{ href: '/developer', label: 'Developer' }] : [])
	];

	$effect(() => {
		console.log();
	});
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>

<div class="flex min-h-screen flex-col">
	<header class="flex items-center border-b border-b-amber-400">
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

	{@render children()}
</div>
