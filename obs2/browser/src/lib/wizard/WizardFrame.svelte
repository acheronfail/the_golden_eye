<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		subtitle,
		children
	}: {
		title: string;
		subtitle?: string;
		children: Snippet;
	} = $props();
</script>

<main class="mx-auto w-full max-w-xl px-4 py-8 sm:px-6 sm:py-12">
	<h1 class="obs-heading text-2xl font-semibold">{title}</h1>
	{#if subtitle}
		<p class="obs-subtitle mt-2 mb-8 text-sm">{subtitle}</p>
	{:else}
		<div class="mb-8"></div>
	{/if}

	{@render children()}
</main>
