<script lang="ts">
	import MonitorSession from '$lib/MonitorSession.svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();
</script>

<svelte:head>
	<title>All 60 | {params.sourceName}</title>
</svelte:head>

<MonitorSession sourceName={params.sourceName} mode="all60" title="All 60" />
