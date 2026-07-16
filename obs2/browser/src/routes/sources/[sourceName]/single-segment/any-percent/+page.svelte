<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import MonitorSession from '$lib/MonitorSession.svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();
	let difficulty = $derived(Number(page.url.searchParams.get('difficulty')));
	let valid = $derived([0, 1, 2].includes(difficulty));

	$effect(() => {
		if (!valid) void goto('/', { replaceState: true });
	});
</script>

<svelte:head>
	<title>Any% | {params.sourceName}</title>
</svelte:head>

{#if valid}
	<MonitorSession sourceName={params.sourceName} mode="anyPercent" {difficulty} title="Any%" />
{/if}
