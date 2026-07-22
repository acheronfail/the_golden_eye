<script lang="ts">
	import type { RunClip } from '$lib/api';
	import RunList from '$lib/components/RunList.svelte';

	let {
		loading = false,
		clips = [],
		visibleClips = clips,
		scannedDirectoryCount = 2,
		directoryCount = 2,
		hasActiveFilters = false
	}: {
		loading?: boolean;
		clips?: RunClip[];
		visibleClips?: RunClip[];
		scannedDirectoryCount?: number;
		directoryCount?: number | null;
		hasActiveFilters?: boolean;
	} = $props();
</script>

<main class="mx-auto w-full max-w-3xl px-3 py-6 sm:px-4">
	<div class="mb-4">
		<h1 class="obs-heading text-xl font-semibold">Runs</h1>
		<p class="obs-dim mt-1 font-mono text-xs">
			{visibleClips.length} of {clips.length}{loading ? ' | scanning...' : ''}
		</p>
	</div>
	<RunList
		{loading}
		{clips}
		{visibleClips}
		{scannedDirectoryCount}
		{directoryCount}
		{hasActiveFilters}
		fileBrowserLabel="Show in file browser"
		clearFilters={() => {}}
		open={() => {}}
		rename={() => {}}
		reveal={() => {}}
		remove={() => {}}
	/>
</main>
