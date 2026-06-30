<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { settings, STORAGE_KEY } from '../lib/settings.svelte';

	let { children } = $props();

	$effect(() => {
		console.debug('Settings changed, saving to localStorage:', settings.savedState);
		localStorage.setItem(STORAGE_KEY, settings.savedState);
	});

	const liClass =
		'inline-block rounded border border-amber-500 px-2 py-1 font-mono text-sm \
		text-white hover:text-black hover:bg-amber-600 text-amber-400 dark:text-amber-400';
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>

<header class="flex items-center border-b border-b-amber-400">
	<pre
		class="inline-block border-r p-2 text-left font-mono text-xs leading-[1.17] whitespace-pre text-amber-400 dark:text-amber-400">
┏┳┓┓     ┏┓  ┓ ┓      ┏┓
 ┃ ┣┓┏┓  ┃┓┏┓┃┏┫┏┓┏┓  ┣ ┓┏┏┓
 ┻ ┛┗┗   ┗┛┗┛┗┗┻┗ ┛┗  ┗┛┗┫┗
                         ┛</pre>

	<ul class="ml-4 inline-flex gap-4 font-mono text-sm text-amber-400 dark:text-amber-400">
		<li class={liClass}><a href="/">Home</a></li>
		{#if import.meta.hot}
			<li class={liClass}><a href="/developer">dev utils</a></li>
		{/if}
	</ul>
</header>

{@render children()}
