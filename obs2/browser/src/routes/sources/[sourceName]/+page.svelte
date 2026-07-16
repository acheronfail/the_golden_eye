<script lang="ts">
	import { goto } from '$app/navigation';
	import { monitorHref } from '$lib/monitor.svelte';
	import { monitor } from '$lib/monitor.svelte';
	import { runModePath } from '$lib/singleSegment';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();

	$effect(() => {
		const activeHref = monitorHref(monitor.status);
		if (activeHref) {
			void goto(activeHref, { replaceState: true });
			return;
		}
		void goto(`/sources/${encodeURIComponent(params.sourceName)}/${runModePath('clips')}`, { replaceState: true });
	});
</script>

<svelte:head>
	<title>Monitor | {params.sourceName}</title>
</svelte:head>
