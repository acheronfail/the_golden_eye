<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { settings, STORAGE_KEY } from '../lib/settings.svelte';

	let { children } = $props();

	$effect(() => {
		console.debug('Settings changed, saving to localStorage:', settings.savedState);
		localStorage.setItem(STORAGE_KEY, settings.savedState);
	});
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>

<header class="border-b border-b-amber-400">
	<pre class="text-xs inline-block font-mono whitespace-pre leading-[1.17] text-left p-2 text-amber-400 dark:text-amber-400">
┏┳┓┓     ┏┓  ┓ ┓      ┏┓
 ┃ ┣┓┏┓  ┃┓┏┓┃┏┫┏┓┏┓  ┣ ┓┏┏┓
 ┻ ┛┗┗   ┗┛┗┛┗┗┻┗ ┛┗  ┗┛┗┫┗
                         ┛
</pre>

<ul class="inline-block ml-4 text-sm text-amber-400 dark:text-amber-400">
	<li class="inline-block mr-4"><a href="/">Home</a></li>
	<li class="inline-block"><a href="/obs">OBS Connection</a></li>
</ul>

</header>

{@render children()}
