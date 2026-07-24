<script lang="ts">
	import type { RunClip, RunSort } from '$lib/api';
	import RunList from '$lib/components/RunList.svelte';

	let {
		loading = false,
		clips = [],
		visibleClips = clips,
		scannedDirectoryCount = 2,
		directoryCount = 2,
		hasActiveFilters = false,
		sort = 'newest'
	}: {
		loading?: boolean;
		clips?: RunClip[];
		visibleClips?: RunClip[];
		scannedDirectoryCount?: number;
		directoryCount?: number | null;
		hasActiveFilters?: boolean;
		sort?: RunSort;
	} = $props();
</script>

<main class="mx-auto w-full max-w-3xl px-3 py-6 sm:px-4">
	<div class="mb-4">
		<h1 class="text-xl font-semibold obs-heading">Runs</h1>
		<p class="mt-1 font-mono text-xs obs-dim">
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
		{sort}
		onSortChange={() => {}}
		fileBrowserLabel="Show in file browser"
		clearFilters={() => {}}
		open={() => {}}
		rename={() => {}}
		reveal={() => {}}
		remove={() => {}}
	/>
</main>
