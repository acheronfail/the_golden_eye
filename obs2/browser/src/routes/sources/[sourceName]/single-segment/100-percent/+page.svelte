<script lang="ts">
	import MonitorSession from '$lib/MonitorSession.svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();
</script>

<svelte:head>
	<title>100% | {params.sourceName}</title>
</svelte:head>

<MonitorSession sourceName={params.sourceName} mode="hundredPercent" title="100%" />
